# Independent reliability audit and repair checkpoint

2026-09-08; main @ a48108750945e42509164980e49ad452c3e12e79; initially clean.
Scope: today's commit inventory, deep review of compaction, tool dispatch, cancellation and prompt error lifecycle. Security excluded. Approximate budget: 25 relevant files, prioritized state mutation and failure paths. Other today's UI/output changes are delegated separate lenses, not certified here.
Skill: audit-reliability with shared evidence, calibration, format and execution contracts; then repair-defect-patchset. Read-only discovery frozen before repair. No live provider/outage drill. Shell source trace is the baseline oracle, focused Rust regressions will validate repairs.

## Surface inventory and failure map

| Operation | Dependency/failure | Boundary | Recovery and signal |
|---|---|---|---|
| Automatic compaction | Provider failure/cancel/metrics | bounded hierarchical summarization, cancellation guard, session replace | failure text; missing terminal contract below |
| Manual compaction | same provider | separate execute-command path | cancellation boundary absent |
| ACP prompt lifecycle | agent stream | terminal error metadata/content → run state | metadata dropped below |
| Tool denial | inspection result | policy reason projected to rejected tool response | policy/current-permissions differentiated |
| Partial resume | database history | in-memory fold must not replace unloaded messages | guarded in perform_compact_with_provider |

## Findings

### REL-TODAY-001: Terminal compaction/provider failures can report successful completion

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Reliability

Evidence:
- `crates/gosling/src/agents/agent/reply_entry.rs:396-402`: failure yields `.with_text(crate::context_mgmt::auto_compaction_failure_message(&e))` then `return;` without terminal metadata.
- `crates/gosling/src/agents/agent/reply_stream.rs:241-247`: same proactive failure pattern; hard overflow at 841-846 already uses `.with_terminal_error(e.to_string())`.
- `crates/gosling/src/agents/execute_commands.rs:202-205`: manual failure returns `Ok(Some(user_only_assistant_text(...)))`.
- `crates/gosling/src/acp/server/message_projection.rs:17-28`: `prompt_error_from_message_content` recognizes only CreditsExhausted; prompt_execution 298-302 checks content, not terminal metadata.
- `crates/gosling/src/acp/server/prompt_execution.rs:453-460`: no stream error/cancel selects `AcpPromptRunState::Completed`.
- `crates/gosling-cli/src/session/mod.rs:1838-1839`: terminal_error_reason reads only `message.metadata.terminal_error.clone()`.

Observed behavior: source paths encode failed operations as successful stream exhaustion.
Expected boundary: explicit terminal errors must survive core→ACP/CLI outcome projection.
Failure mechanism: compaction producer omissions and ACP metadata consumer omission.
Break-it angle: provider fails compaction, or sends a marked refusal; inspect returned error and stored run state.
Impact: session run status and automation exit outcome misrepresent failed work.
Operational impact: Workflow; user-visible; reversible; UI-visible; rerun safety unknown.
Adjacent failure modes: manual command failure and hard-overflow/provider refusal metadata share outcome boundary.
Recommended mitigation: mark failure messages; consume metadata in ACP while preserving specialized credit errors and cancellation precedence.
Implementation assessment: workflow_protocol; M; codex; modules/tests; core and two outcome adapters must agree.
Validation: failing-provider compaction, marked ACP message, ordinary successful text, credit-specific payload regression.
Non-goals: provider selection/retry changes.

### REL-TODAY-002: Manual compaction ignores prompt cancellation

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Reliability

Evidence:
- `crates/gosling/src/agents/agent/reply_entry.rs:189-192`: `.execute_command(&message_text, &session_config.id).await` has no cancellation argument.
- `crates/gosling/src/agents/execute_commands.rs:191-211`: `compact_messages(...).await` then `.replace_conversation(session_id, &compacted_conversation)` with no cancellation check.
- `crates/gosling/src/acp/server/prompt_execution.rs:210-224,248-252`: reply is awaited before stream cancellation select exists.

Observed behavior: cancellation token is unavailable to manual compaction; its completed summary is saved even if prompt cancellation was requested during summarization.
Expected boundary: cancellation before compaction commit preserves original history and releases active run without reporting failure/success.
Failure mechanism: manual entrypoint bypasses the new automatic-compaction cancellation boundary.
Break-it angle: mock provider cancels during manual summarization; assert no HistoryReplaced and original messages/visibility retained.
Impact: cancelled action still consumes work and mutates conversation.
Operational impact: Workflow; DB; compensatable; UI-visible; rerun safety unknown.
Recommended mitigation: internal cancellation-aware command dispatch retaining public command API, select cancellation against summarization, recheck before save; return empty cancelled stream.
Implementation assessment: local_guardrail; S; codex; modules/tests; narrow command path shares existing token.
Validation: cancellation-in-provider regression plus successful manual compaction existing test.
Non-goals: abort a commit already in progress or distributed transactional cancellation.

## Taxonomy and break-it review

REL-001/002 startup/health: not reviewed (no startup changes in prioritized scope).
REL-003/009/010/011/015: findings REL-TODAY-001/002.
REL-004 retry storm: held source boundary; mid_stream retries cap and delay at reply_stream 898-935; compaction retry_operation uses default bounded policy.
REL-005/013 resource/unbounded work: bounded request chunks and 12 reduction rounds in context_mgmt 51-55,808; whole-history memory pressure not measured (no OOM claim).
REL-006 timeout: manual cancellation finding; provider transport timeouts not exhaustively reviewed.
REL-007/008 crash/non-atomic recovery: partial resume guard at reply_entry 460-467 preserves unloaded history; post-save metrics error typed; process-kill not performed.
REL-012 cleanup: cancellation review covered active-run return path; process lifecycle not reviewed.
Policy denial held: reply_context 208-222 selects actual policy reason/current permissions. No permission decisions changed.
Empty provider completion held for truly empty messages: gosling-providers/base.rs 541-542 rejects zero content; whitespace-only summary is not certified.

## Skill escalation

| Finding | Primary | Secondary | Reason |
|---|---|---|---|
| REL-TODAY-001 | Reliability | State Transition / Workflow | run-state and exit semantics |
| REL-TODAY-002 | Reliability | Temporal / State Transition | cancellation before mutation |

## Repair plan (gates 0–3)

One shared compaction/outcome stage, REL-TODAY-001 P1 medium complexity and REL-TODAY-002 P2 medium complexity. Touch reply_entry/reply_stream/execute_commands, ACP prompt_execution/message_projection/server imports, compaction and ACP tests. Baseline: focused compaction command in baseline.log running; source behavior captured above. Preserve public execute_command API, success responses, credit payloads, provider-owned context, cancellation outcome and full/partial-history behavior. Governing declarations: AGENTS core/client ownership, docs/build/context-compaction-failsafe-plan.md original-history acceptance, existing terminal_error metadata and ACP run-state schema. Preexisting drift is these findings; no schema changes planned. Parent owns aggregate session documentation. Next: regression, repair, focused tests, final re-audit, distinct completeness pass.

## Repair checkpoint (gates 4–6, validation pending)

Baseline focused suite: 8 passed (`baseline.log`). Added manual cancellation regression failed before source repair at the assertion that cancelled commands must not publish completion (`regression-before.log`). Fixture review: each test creates a fresh hidden session using SessionManager; threshold RAII helper restores configuration; no SQLite pragma/reset toggles were introduced. Actual native Desktop/ACP transport integration is not exercised by this mock-provider fixture.

Repairs now staged in working tree: private cancellation-aware command dispatch preserves public execute_command; cancellation select and second check occur before manual save; cancelled command returns an empty reply stream; automatic failures stop silently for cancellation and mark genuine errors terminal. Manual compaction provider failures and command errors carry terminal metadata. ACP checks the complete message after normal content handling and preserves specialized CreditsExhausted reason/url ahead of generic terminal metadata. No protocol/schema change.

Regression additions: manual cancellation preserves exact two original visible messages; manual, entry-auto and loop-auto failed compaction retain original visible history and publish terminal error; ACP ordinary success produces no error, metadata failure produces error, credit-specific reason remains intact. Cargo sequence shared with dataflow agent to avoid competing compiler processes.

Final re-audit to complete after green checks: core producer→CLI metadata consumer→ACP projection→Failed state, cancellation→empty stream→ACP cancellation state, public command API success parity, manual full-history/partial-resume persistence. Distinct completeness pass will map both findings back to original triggers and compare final diff to governing contracts. No additional security scan performed.

## Final source re-audit (gate 8)

Checked changed producer and consumers together: manual errors retain text + terminal metadata through with_visibility; automatic failure producers now supply the same metadata the CLI reads; ACP keeps specialized credit errors before the metadata fallback, projects ordinary content before deciding the generic error outcome, and records Failed from stream_error. Successful messages have no terminal metadata and retain the prior completed path. Cancellation produces no failure/completion message from compaction and reaches the existing ACP cancellation branch. The command result cancellation check is necessary: a cancelled handler returns None, which must never fall through to the ordinary user-message branch.

Persistence reviewed: public execute_command still defaults to no cancellation; full manual history is loaded before summarization; successful replace and metrics ordering are unchanged; auto compacted-resume guard remains intact. A cancellation arriving after database replacement starts is outside the pre-commit cancellation guarantee; no claim of cross-resource atomic cancellation. Current regression fixtures do not run a live ACP transport or CLI executable; source tracing covers those consumers.

Architecture comparison: existing terminal_error and cancellation contracts are used without schema or interface changes; core remains owner of compaction and ACP remains outcome projection. Drift delta: no new drift. Marker scan found no stale fixed-defect TODO at modified branches. Separate final completeness pass and execution evidence remain pending union checks.

## Verified repair disposition (2026-09-08)

REL-TODAY-001: repaired; automatic/manual compaction failure messages retain terminal metadata, and ACP recognizes it. Actual core failure regression covers all three compaction entry shapes; ACP projection test checks generic error, success and credit-specific preservation.
REL-TODAY-002: repaired; cancelled manual compaction preserves both original visible messages and emits no completion event. The regression failed before repair and passed afterward.

Validation: `cargo test -p gosling --lib --test compaction --test output_revisions_test --locked` passed 1,883 library tests (3 ignored), 10 compaction tests and 25 output revision tests. Shared evidence: `../dataflow/union-after.log`; the three new reliability tests appear at lines 184,1900,1909. Existing successful manual/auto/recovery and partial-resume regressions passed unchanged. `cargo fmt` performed; final format/diff checks separately logged. Shared Clippy pending at this checkpoint.

Gate 9 separate completeness pass: mapped both original triggers to test evidence and reviewed the union of this lane's edits after the passing run. REL-TODAY-001 covers producer marker omissions, manual command errors and ACP consumer; REL-TODAY-002 covers pending summarization plus same-poll cancellation and the None fallthrough. No provider selection, retry/backoff, persisted message schema or public execute_command signature changed. Parent reviewer owns independent critic and aggregate session/docs closure. Report status: completed_with_partial_verification (no live ACP/CLI process invocation, native Desktop, process-kill or full-workspace suite; scoped tests passed). Source record updated in this dated disposition without deleting original findings.

## Independent critic completeness amendment

The parent/reviewer found two gaps in the initial REL-TODAY-001/002 closure: lease revocation cancels the internal child token without cancelling the caller's token, and CLI can select EOF before an already-ready user-cancellation branch. The initial verified status above therefore covered the original regressions, not these newly identified variants.

Core repair retains caller token identity and checks for lease-only cancellation after command dispatch, when initial compaction exits on cancellation, and after the inner reply stream drains. Revocation is an actionable error instead of successful exhaustion; genuine caller cancellation still yields the cancellation path. A token-lifecycle regression checks normal, child-only cancellation with/without a caller, and caller cancellation. Existing lease heartbeat regression supplies real revocation-to-child-token evidence. The reviewer owns scoped CLI EOF normalization plus its regression; parent will independently critique that patch. Final union validation must be refreshed after these code changes.

## Final core closure after critic amendment

2026-09-08: independent reviewer `/root/repair_review` reviewed the core token-identity refinement and found no further source blocker. Refreshed `cargo test -p gosling --lib --test compaction --locked` passed 1,884 library tests (3 ignored) and 10 compaction tests; evidence `../dataflow/final-core-tests.log`. This supersedes the prior core execution count. `git diff --check` passed after refinement. Parent owns the combined gosling/CLI Clippy run and independent review of the reviewer-authored CLI EOF repair.

Both REL-TODAY-001/002 core repairs, including REVIEW-REL-001 lease-revocation completeness, are verified by focused tests plus caller/callee source review. Report status remains completed_with_partial_verification: live ACP takeover and fresh CLI end-to-end process outcomes were not run by this lane. CLI EOF variant and final combined lint evidence are tracked by parent/reviewer and must be included in the aggregate closure. No further core implementation work remains in this lane.
