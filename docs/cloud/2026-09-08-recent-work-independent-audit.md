# Independent change-set audit — last ~20 commits (2026-09-07/08)

Authority: `read_only`. No target code was modified. Prior `docs/cloud/*audit*` reports and `docs/logs/session/` entries were not used as evidence. Security lenses were excluded by operator request.

## Executive Verdict

Today's window ships three product surfaces at once: output-revision custody (ADR-0018), budget-capped auto-compaction with `autoCompactReduction`, and Desktop artifact trash/copy/filter/history. One confirmed High/Critical defect should pause **Desktop auto-compact after a compacted session load**: `reply()` can `replace_conversation` from a 50-message tail while `check_if_compaction_needed` still uses full-session token usage, which deletes older `messages` rows. That is durable chat-history loss, not a summary overlay.

The rest of the window is mixed. Output history, copy-contents, hash-checked restore, and trash-without-unlink are carefully built and mostly honest. They still have a persist-before-commit crash window (restore can rewrite the file without a committed baseline), one bound failure aborting a whole observation, and operator-facing copy that says Delete while the effect is OS Trash. Compaction preference pair-validation was added on the ACP preferences API that Desktop does not use; AlertBox writes the same keys through unvalidated `config/upsert`, and runtime then silently falls back to full collapse. Permission-prompt work improved inspector text, but session chrome still says tools are unrestricted while workspace folder policy continues to deny or prompt.

Patching is recommended now for the compacted-resume persist path. Do not treat the preferences-API tests as covering the live Desktop sliders.

## Scope

- Repository: `gosling` (`/Users/eric/Work/vscode/forked/gosling`)
- Branch / commit: `main` @ `1a2504e0588fe73720b90b673ca81122064df7d0`
- Prompt: independent multi-lens audit of the latest work (last ~20 commits, 2026-09-07/08); no security scan; no prior logs/scans; write to the repo audit location
- Change-set window: `0105cd449` … `1a2504e05` (plus one dependency hop). Clusters: output revisions, auto-compaction/reduction, artifact trash/copy/timestamps/filter, permission-prompt readability / working-dir inspector, workspace unread indicator, z-index, ACP schema/config
- Skills (lenses) invoked: architecture-seam, architecture-drift (bootstrap; no `.architecture/` registry), dataflow cascade/concurrency/integrity/input-output/state-transition/temporal, invariant-sync, negative-space, reliability, workflow-gui, failsafe-readiness, recovery-idempotency, operator-signal, agent-orchestration-code, contract-internalapi, dataflow-pipeline-graph (static graph only)
- Explicitly excluded: all `audit-security*` skills, `audit-compliance-posture` (operator: no security scan)
- Files/directories inspected: listed in Surface Inventory; source read of the change-set plus one hop. Prior audits under `docs/cloud/` and `docs/logs/session/` were not read for findings
- Commands/tests run: none against the target (read-only; no cargo/pnpm execution). Evidence basis is `source-evidenced` unless noted
- Effort budget: ~6 parallel source walks + parent corroboration of high-risk files (`output_revisions.rs`, `output_revisions_storage.rs`, `context_mgmt/mod.rs`, `reply_entry.rs`, `config.rs`, `fileIpc.ts`, `OutputHistory.tsx`, `ArtifactPane.tsx`, `AlertBox.tsx`). Remaining inventory codes outside the change-set are `Not Reviewed`
- Constraints: `read_only`; no target mutation; no network; no live Desktop playtest

## Draft Prompt Assessment

Intended mission: a fresh, rigorous audit of **this week's product work**, not a fleet-wide security or whole-repo re-audit.

Explicit restrictions honoured: no security scan; no reuse of prior scan/session-log findings; report in `docs/cloud/` per `docs/INDEX.md`.

Assumptions (inferred, not told):

- “Last ~20 commits” means 2026-09-07/08 through HEAD `1a2504e05`, including the HEAD commit that mixed an earlier audit report with code
- “Data reliability/custody” includes SQLite vs filesystem dual-store and session-message replacement
- Compacted session load is in scope as the one-hop consumer of today's auto-compact persist
- Involvement level `L2` from request sophistication; zero questions asked

Angles applied: producer/consumer (ACP vs Desktop config; SQLite vs file; inspector vs session chrome), sibling implementations (preferences save vs config upsert; restore vs capture write order), failure-halfway (persist then commit), replay, operator interpretation.

Adjacent work not performed: live Desktop playtest, crash-injection, security of working-dir canonicalization, MCP server construction, Node backend architecture, path-relocation, performance.

## Coverage matrix

| Skill | Disposition | Reason |
|---|---|---|
| audit-architecture-seam | applied | seams of revision service, ACP, Electron IPC, compaction persist |
| audit-architecture-drift | applied (bootstrap) | no `.architecture/` registry; ADR-0018 / architecture.md vs code |
| audit-dataflow-cascade | applied | bound failure, compaction persist blast |
| audit-dataflow-concurrency | applied (sampled) | write guard, restore CAS; races capped Likely |
| audit-dataflow-integrity | applied | path-keyed revisions, dual-store, trash vs snapshots |
| audit-dataflow-input-output | applied | copy, trash, restore, footer, export |
| audit-dataflow-state-transition | applied | restore/hash/missing file; permission deny vs decline |
| audit-dataflow-temporal | applied (sampled) | hash TOCTOU, stale timestamps, footer vs SQLite |
| audit-invariant-sync | applied | reduction/threshold copies; revision DTO vs hash semantics |
| audit-negative-space | applied (survey) | compacted-resume composition; cancel during compact |
| audit-reliability | applied | compaction persist, empty/failure signals |
| audit-workflow-gui | applied | trash copy, filters, history row, permission chrome |
| audit-failsafe-readiness | applied (survey of today's workflows) | |
| audit-recovery-idempotency | applied | persist/commit, restore retry |
| audit-operator-signal | applied | compaction complete, deny-as-decline, bound notes |
| audit-agent-orchestration-code | applied (compaction / HistoryReplaced) | |
| audit-contract-internalapi | applied | ACP revision errors; preferences vs upsert |
| audit-dataflow-pipeline-graph | applied (static; Gates 7–11 unexecuted) | |
| audit-security, audit-security-*, audit-compliance-posture | excluded | operator: no security scan |
| audit-mcp-server | not_applicable (this change-set) | MCP server construction not touched |
| audit-architecture-nodejs | not_applicable | Electron UI is not a Node backend; seam lens covers main/renderer |
| audit-design-webapp | deferred | operator asked workflow/UX truth, not six-gate visual design |
| audit-playtest-app | deferred | no running instance; static source audit requested |
| audit-repo-path-consistency | not_applicable | relocation/provenance not in the window |
| audit-deadcode-cleanup, audit-pipeline-externalapi, audit-dependency-criticality, optimization/perf/memory/resource | deferred | budget; not named by the operator |
| audit-contract-crossrepo | not_applicable | goose-compat docs only |
| audit-multiagent-consensus | not requested | |

## Surface Inventory

| Surface | Actor | Input/Trigger | State/Output | Boundary | Reviewed |
|---|---|---|---|---|---|
| Output capture | hosted mutating tool | `prepare_output_capture` / `finish_output_capture` | SQLite `output_revisions` + optional markdown footer | session folders, bounds, tool success | yes |
| Output history/get/restore | Desktop / ACP | `_gosling/unstable/session/outputs/{history,revision,restore}` | DTO + blobs; file replace | inventory row + folder policy + hash | yes |
| Copy artifact contents | Desktop renderer | `copy-artifact-contents` IPC | clipboard UTF-8 | renderer artifact grant, 20 MiB | yes |
| Artifact trash | Desktop | `trash-artifact-files` | OS Trash; workbench `deletedArtifacts` | no unlink fallback | yes |
| Outputs filter | Desktop | extension list + hide-repository | displayed inventory | UI-owned | yes |
| Auto-compact | agent `reply` / tool loop | threshold + reduction | `replace_conversation` | provider, cancel, compacted load | yes |
| Compaction prefs | ACP preferences vs config upsert vs AlertBox vs CLI vs runtime | `GOSLING_AUTO_COMPACT_*` | yaml/env | pair validation | yes |
| Working-dir inspector | tool inspection | paths / shell | RequireApproval / Deny | workspace policy vs restrict flag | workflow/logic only |
| Session chrome | Desktop | restrict toggle | copy + listed dirs | UI vs inspector | yes (one hop) |
| Workspace green dot | Desktop nav | stream idle | unread indicator | not workspace health | yes |
| CLI / HTTP reply | CLI, gosling-server | `compacted_context: false` | full conversation compact | sampled | |

## Boundary Map

| Surface | Intended Boundary | Enforced At | Status |
|---|---|---|---|
| Revision access | inventory + session folders; restore needs write | `authorized_output` | held |
| Restore CAS | live file hash | `restore_output_revision` + `replace_if_unchanged` | held for detected change; crash window open |
| Capture vs tool success | history failure must not retry the tool | `tool_dispatch.rs` appends text | held (ADR); one bound is global |
| SQLite vs file | SQLite is saved history | UI reads ACP only | held for UI; file footer can lead |
| Auto-compact persist | compact the **session** conversation | `replace_conversation` | **broken** under compacted resume |
| Reduction vs threshold | reduction `<` threshold or 0 | preferences API only | **bypassed** by Desktop upsert |
| Workspace folder policy | always on for workspace sessions | inspector | held in Rust; **lied about** in session summary |
| Trash | reversible OS trash, never unlink | `shell.trashItem` | held in main; chrome says Delete |
| Copy contents | complete UTF-8 or error | `fileIpc.ts` | held |

## Findings Table

| ID | Severity | Confidence | Evidence Basis | Domain | Title | Patch Priority | Blast Radius | Complexity | Cost | Nominal Agent |
|---|---|---|---|---|---|---|---|---|---|---|
| REL-GOS-001 | Critical | Confirmed | source-evidenced | Reliability | Compacted-resume auto-compact deletes unloaded history | 1 | Service | persistence_recovery | M | codex |
| FSR-GOS-001 | High | Confirmed order; manifestation Likely | source-evidenced | Failsafe | Restore persist-before-commit can rewrite the file with no SQLite baseline | 2 | Workflow | persistence_recovery | M | codex |
| WFG-GOS-001 | High | Confirmed | source-evidenced | Workflow-GUI | Desktop sliders bypass compaction pair-validation; runtime silently full-collapses | 3 | Workflow | local_guardrail | S | codex |
| DAT-GOS-001 | Medium | Confirmed | source-evidenced | Data-Integrity | Path-keyed revisions survive chat delete/trash; Desktop hides the export surface | 5 | Workflow | operator_ux | S | grok |
| CAS-GOS-001 | Medium | Confirmed | source-evidenced | Cascade | One observation bound fails capture for every file in the turn | 4 | Workflow | local_guardrail | S | codex |
| WFG-GOS-002 | Medium | Confirmed | source-evidenced | Workflow-GUI | Row chrome says Delete; effect is OS Trash; history remains | 5 | Local | operator_ux | XS | grok |
| WFG-GOS-003 | Medium | Confirmed | source-evidenced | Workflow-GUI | Default Outputs extension filter silently drops live ADR-0018 types | 5 | Local | operator_ux | S | grok |
| WFG-GOS-004 | Medium | Confirmed | source-evidenced | Workflow-GUI | Session summary says tools are unrestricted while workspace policy still gates | 4 | Workflow | operator_ux | S | grok |
| STT-GOS-001 | Medium | Confirmed | source-evidenced | State-Transition | Policy Deny is recorded as “the user declined” | 4 | Workflow | local_guardrail | S | codex |
| WFG-GOS-005 | Medium | Confirmed | source-evidenced | Workflow-GUI | Read-only workspace roots listed as full read/write/run | 4 | Workflow | operator_ux | S | grok |
| FSR-GOS-003 | Medium | Confirmed | source-evidenced | Failsafe | Auto-compact does not observe cancel before persist | 3 | Workflow | workflow_protocol | S | codex |
| FSR-GOS-004 | Medium | Likely | source-evidenced | Failsafe | `compaction_failure_message` can claim the original session is intact after `replace_conversation` | 3 | Workflow | local_guardrail | S | codex |
| IAPI-GOS-001 | Medium | Confirmed | source-evidenced | Architecture | Revision ACP errors all collapse to `invalid_params` | 6 | Local | local_guardrail | S | codex |
| INV-GOS-001 | Medium | Confirmed | source-evidenced | Invariant-Sync | `contentHash` is body; `currentHash` is on-disk bytes including footer | 6 | Local | local_guardrail | XS | codex |
| INV-GOS-002 | Medium | Confirmed | source-evidenced | Invariant-Sync | Reduction/threshold legal range disagrees across ACP, CLI, UI, runtime, docs | 3 | Workflow | local_guardrail | S | grok |
| ARC-GOS-001 | Low | Confirmed | source-evidenced | Architecture | architecture.md claims ACP export; export is Electron `saveArtifact` | 7 | Local | governance_decision | XS | human-owner |
| STT-GOS-002 | Low | Confirmed | source-evidenced | State-Transition | Empty history row labeled “Unknown” | 7 | Local | operator_ux | XS | grok |
| WFG-GOS-006 | Low | Confirmed | source-evidenced | Workflow-GUI | “Hide repository files” also hides source-like Outputs (html/js/…) | 6 | Local | operator_ux | S | grok |
| IOP-GOS-001 | Low | Confirmed | source-evidenced | Input-Output-Path | Compare-with-previous failure blanks the selected revision | 6 | Local | local_guardrail | XS | grok |
| WFG-GOS-007 | Low | Confirmed | source-evidenced | Workflow-GUI | Green “ready” dot is unread chat activity, not workspace health | 7 | Local | operator_ux | XS | grok |

## Detailed Findings

### REL-GOS-001: Compacted-resume auto-compact deletes unloaded history

Severity: Critical
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Reliability

Evidence:
- `ui/desktop/src/acp/sessions.ts:43,318-327` — Desktop `loadSession` always uses `loadMode: 'compacted'` and `tailLimit: 50`
- `crates/gosling/src/acp/server/prompt_execution.rs:194-208` — prompt copies `compacted_context` / `tail_limit` into `SessionConfig`
- `crates/gosling/src/agents/agent/reply_entry.rs:304-317` — `reply()` then loads `get_session_for_compacted_resume`
- `crates/gosling/src/session/session_manager/summary_storage.rs:204-240` — resume replaces `session.conversation` with a tail page (optional summary stub); `session.usage` is kept from `get_session`
- `crates/gosling/src/context_mgmt/mod.rs:543-546` — `current_tokens = stored_usage.max(estimated(conversation))`
- `crates/gosling/src/agents/agent/reply_entry.rs:324-333,437-449` — if compact is needed, `compact_messages` runs on that tail conversation, then `replace_conversation`
- `crates/gosling/src/session/session_manager/message_storage.rs:465-481` — `replace_conversation_in_tx` `DELETE FROM messages/session_summaries/session_summary_facts`

Observed behavior:
- After Desktop reopens a long chat, the next user turn can auto-compact. Need-check uses **full-session** stored tokens against a **tail-only** conversation. Compact then persists that tail (plus a new summary) as the entire session. Older rows never loaded into the conversation are deleted.

Expected boundary:
- Auto-compact may fold agent-visible history, but it must load the full conversation (or compact only the already-loaded tail **without** deleting unloaded rows). Compacted resume is a presentation/context budget, not a license to drop durable messages.

Failure mechanism:
- Two independently reasonable features compose: compacted load (tail for the model) and `replace_conversation` (persist whatever conversation compact returned). Today's reduction budget still calls that persist path. CLI/HTTP keep `compacted_context: false` and do not hit this composition.

Break-it angle:
- Create a session with well over 50 messages and usage above `GOSLING_AUTO_COMPACT_THRESHOLD`. Quit Desktop, reopen the chat, send one message. After “Compaction complete”, `messages` for that session contain only the compacted tail.

Impact:
- Unrecoverable loss of user-visible older turns, tool transcripts, and previous compact originals that were not in the tail. Operator is told compaction succeeded.

Operational impact:
- Blast radius: Service
- Side-effect class: DB
- Reversibility: irreversible (unless an external backup of `sessions.db` exists)
- Operator visibility: UI-visible lie (“Compaction complete”)
- Rerun safety: unsafe (further replies operate on the truncated session)

Adjacent failure modes:
- FSR-GOS-004 (failure text after persist)
- CAS-GOS-001 (local failure becoming global) as the same persist primitive

Recommended mitigation:
- Remediation patterns: `transaction_boundary`, `false_success_guard`
- Minimal repair: when `compacted_context` is true, either skip auto-compact persist, or load the full conversation before `compact_messages` / `replace_conversation`
- Local guardrail: `replace_conversation` must refuse a payload whose message count is far below `session.message_count` unless the caller passed an explicit full-history compact
- Behavior test: compacted resume + high stored usage + 80 stored messages / 50 tail → post-compact SQL count still 80+ (or compact is skipped)

Implementation assessment:
- Complexity: persistence_recovery
- Cost: M
- Cost drivers: modules, tests, runtime_verification
- Nominal implementation agent: codex
- Rationale: one persist-guard plus a session-fixture test; Desktop load mode stays

Validation:
- Test: Desktop-equivalent `compacted_context=true`, `tail_limit=50`, stored usage above threshold, `total_count > 50` → `replace_conversation` is not called with the tail, or unloaded rows remain
- Test: CLI `compacted_context=false` still auto-compacts full history

Non-goals:
- Do not remove compacted load for ACP replay
- Do not redesign the summarizer in this slice

### FSR-GOS-001: Restore persist-before-commit can rewrite the file with no SQLite baseline

Severity: High
Confidence: Confirmed for write order; manifestation Likely (process death / commit fail)
Evidence basis: source-evidenced
Domain: Failsafe

Evidence:
- `crates/gosling/src/session/session_manager/output_revisions_storage.rs:456-486` — insert restore (+ optional unknown baseline) in an open tx, `replace_if_unchanged` (file persist), then `tx.commit()`
- `crates/gosling/src/session/output_revisions.rs:262-291` — `NamedTempFile` write + `persist(path)`
- Same order on capture annotate: `output_revisions_storage.rs:297-332`
- ADR-0018 documents the capture crash window; restore is the same order with worse loss (untracked current bytes)

Observed behavior:
- If persist succeeds and commit does not, the live file already holds restored bytes. SQLite has neither the unknown baseline of the pre-restore contents nor a `Restored` row. UI reports restore failure. Retry with the original `expectedCurrentHash` fails “Output changed”.

Expected boundary:
- `fail_idempotent` / `fail_rollback`: a failed restore leaves the live file unchanged **or** both the overwritten bytes and the new revision are committed. Capture may leave a footer ahead of SQLite (ADR), but restore must not destroy untracked current contents without a committed baseline.

Failure mechanism:
- Dual-store write is not one transaction. The comment at 456 only covers “file cannot be replaced,” not “file replaced and commit lost.”

Break-it angle:
- Fault after `persist`, before `commit`. File matches vN; history does not; operator retries restore and is blocked.

Impact:
- Loss of untracked edits; lying failure signal; no user-restore provenance.

Operational impact:
- Blast radius: Workflow. Side-effect class: file+DB. Reversibility: compensatable only if the operator still has the pre-restore bytes. Operator visibility: UI-visible (false failure). Rerun safety: unsafe.

Adjacent failure modes:
- REC restore retry; SIG false failure

Recommended mitigation:
- Remediation patterns: `transactional_write`, `checkpoint_resume`
- Minimal repair: persist only after commit using a sibling temp name, or re-read after persist and commit if bytes match intended restore; on any restore error after persist, commit the baseline+restore rather than drop the tx
- Behavior test: inject commit failure after persist; assert either rolled-back file or committed restore+baseline

Implementation assessment:
- Complexity: persistence_recovery. Cost: M. Nominal agent: codex.

Validation:
- Existing `failed_restore_does_not_commit_the_external_edit_baseline` covers persist **fail**. Add persist-success/commit-fail.

Non-goals:
- Do not require SQLite+filesystem distributed transactions.

### WFG-GOS-001: Desktop sliders bypass compaction pair-validation; runtime silently full-collapses

Severity: High
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `crates/gosling/src/acp/server/config.rs:49-65,318-348` — `preferences/save` calls `validate_compaction_preferences` (`reduction > 0 && reduction >= threshold` rejected)
- `crates/gosling/src/acp/server/config.rs:123-150` — `on_config_upsert` writes any JSON with no prepare/pair check
- `ui/desktop/src/components/ConfigContext.tsx:61-64` — `upsert` → `acpUpsertConfig`
- `ui/desktop/src/components/alerts/AlertBox.tsx:95-107,218-232` — threshold/reduction edited as 0–100% and saved via `upsert`; reduction `minPercent={0}` allows 100%
- `crates/gosling/src/context_mgmt/mod.rs:566-587` — `reduction <= 0.0 || reduction >= threshold` → `Ok(None)` (full eligible collapse), no operator signal
- Tests in `crates/gosling/tests/acp_custom_requests_test.rs:863-988` cover the **preferences** API only

Observed behavior:
- Operator can save Reduce-by 80% with Auto-compact-at 80% (or 100%) from the live AlertBox. Save succeeds. Later auto-compact ignores the reduction and folds the whole eligible region. The ACP preference tests are green and do not protect this path.

Expected boundary:
- The operator-facing control and the runtime must share one validator. A setting that cannot be honored must fail closed at save, or the UI must show “full collapse” when reduction is disabled.

Failure mechanism:
- Pair validation was added on the unused preferences surface. Desktop still uses generic config upsert. Runtime treats misconfiguration as a successful full compact.

Break-it angle:
- Set threshold 80%, reduction 80% in AlertBox. Trigger auto-compact. Observe a full eligible collapse and “Compaction complete” with no “reduction unused” notice.

Impact:
- Operator believes incremental compaction is configured; history is fully folded. False confidence in the new v1.2.3 control.

Operational impact:
- Blast radius: Workflow. Side-effect class: DB. Reversibility: compensatable only if user-visible originals remain (not under REL-GOS-001). Operator visibility: silent. Rerun safety: unsafe.

Adjacent failure modes:
- INV-GOS-002 (CLI/docs/runtime copies)
- REL-GOS-001 (full collapse on a tail conversation)

Recommended mitigation:
- Remediation patterns: `typed_config_validation`, `degraded_status_signal`
- Minimal repair: run `validate_compaction_preferences` from `on_config_upsert`/`on_config_remove` for those two keys; AlertBox should use preferences or the same validator; show when reduction is disabled
- Behavior test: upsert `GOSLING_AUTO_COMPACT_REDUCTION=0.8` while threshold is 0.8 → invalid_params; AlertBox surfaces the error and does not persist

Implementation assessment:
- Complexity: local_guardrail. Cost: S. Nominal agent: codex.

Validation:
- Test config upsert pair rejection matching preferences tests
- Test runtime emits a visible notice if it still falls back

Non-goals:
- Do not remove the 0 = full-collapse contract.

### DAT-GOS-001: Path-keyed revisions survive chat delete/trash; Desktop hides the export surface

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Data-Integrity

Evidence:
- `output_revisions_storage.rs:36-41` — `PRIMARY KEY(path, version)`; no `session_id` column
- `authorized_output` (`:350-352`) — any later session with a `session_artifacts` row for that path may list/get/restore
- ADR-0018: snapshots survive chat deletion; trash does not erase them; export should remain while the parent directory is authorized
- `ui/desktop/src/contexts/ArtifactWorkbenchContext.tsx:138-144,270-310` — trash/missing writes `deletedArtifacts[path]=lastSeenAt` and filters the Outputs list
- `OutputHistory.tsx:414` — Restore disabled without `currentHash`; History lives on the Outputs row that trash just removed

Observed behavior:
- Chat delete does not delete `output_revisions`. A later chat that registers the same path continues that history (intended). Desktop Trash also removes the only UI that can export those snapshots. Restoring the file from OS Trash does not clear `deletedArtifacts` until `lastSeenAt` changes.

Expected boundary:
- Retention across chats is allowed if disclosed. Trashing a file must not hide the only export path for committed snapshots, or the confirm copy must say snapshots remain and how to export them.

Failure mechanism:
- Storage identity is path-global; presentation identity is session inventory + localStorage hide list. ADR presentation contract (“saved bytes can still be exported”) is not met in Desktop after Trash.

Break-it angle:
- Capture history, Move to Trash, try to export vN. The row is gone. Restore the file from Trash; the row stays gone until a new observation.

Impact:
- Operator believes history is gone; SQLite still grows; later chats inherit authorship tables without UI warning on the empty state (`OutputHistory.tsx:22-26`).

Operational impact:
- Blast radius: Workflow. Side-effect class: DB + user-visible. Reversibility: compensatable (ACP get still works). Operator visibility: silent. Rerun safety: safe.

Recommended mitigation:
- Disclose retention on history empty/note strings; keep an export-from-history action after trash, or clear `deletedArtifacts` when the file reappears on disk
- Test: after trash, ACP get still returns bytes; Desktop either offers export or documents why not

Non-goals:
- Do not add cross-chat ACL in this slice.

### CAS-GOS-001: One observation bound fails capture for every file in the turn

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Cascade

Evidence:
- `output_revisions.rs:303-315` — `ensure!(visited <= 2000)` / `ensure!(files.len() <= 200)` abort the scan
- `output_revisions_storage.rs:134-139,197-202` — 32 MiB total aborts prepare/finish
- `read_snapshot` 8 MiB `ensure!` (`output_revisions.rs:149-175`)
- `history_in_tx` 1000-revision `ensure!` (`:501-504`)
- `tool_dispatch.rs:288-290` — capture `Err` is appended to a **successful** tool result

Observed behavior:
- One oversized sibling under `Outputs/` (or the 201st document, or 8 MiB+1 file) fails `finish_output_capture` for **all** paths in that tool. The tool still succeeds; history UI has no banner. Depth 4 is the opposite: silent skip (`scan_output_roots` `depth < 4`).

Expected boundary:
- Per-file skip with a counted warning; explicit mutation targets still record. Bound failure stays non-retryable for the tool (ADR) but must not poison sibling files.

Failure mechanism:
- `?` on a shared `ensure!` over the whole candidate set. No per-path quarantine.

Break-it angle:
- `Outputs/huge.pdf` at 8 MiB+1 beside `report.md`; write `report.md`. Capture errors; `report.md` gets no revision.

Impact:
- History holes that look like “gosling didn’t see the write,” with the reason only in the tool transcript.

Operational impact:
- Blast radius: Workflow. Side-effect class: DB. Reversibility: compensatable (next observation). Operator visibility: log-only (tool card). Rerun safety: safe.

Recommended mitigation:
- Per-file skip + warning; keep explicit tool-write targets even when the scan fails
- Test: two files, one over 8 MiB; small file still records

Non-goals:
- Do not raise the 8 MiB cap here.

### WFG-GOS-002: Row chrome says Delete; effect is OS Trash; history remains

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `ArtifactFileList.tsx:26-27,36-37,39-64` — `Delete {name}` / `Delete selected` / confirm `Move to Trash` / failure `Unable to delete`
- `fileIpc.ts:574-576` — `shell.trashItem` only; comment “must never fall back to permanent deletion”

Observed behavior:
- Operators see Delete on the row. Confirm says Trash. History snapshots remain (DAT-GOS-001). Failure toast says delete.

Expected boundary:
- Destructive chrome matches the actual effect and the remaining custody (OS Trash + SQLite snapshots).

Recommended mitigation:
- Rename row actions to Trash; failure copy “Unable to move to Trash”; mention snapshots in the confirm description.

Non-goals:
- Do not switch to unlink.

### WFG-GOS-003: Default Outputs extension filter silently drops live ADR-0018 types

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `ui/desktop/src/utils/settings.ts:14-24` — default extensions `pdf, md, txt, doc, docx, jpg, png, yaml, json`
- `ArtifactPane.tsx:499-505` — Outputs list is `hasDisplayedFileExtension` with **no** “N hidden” for this filter (the repo switch has one)
- ADR-0018 supported set includes csv/tsv/html/svg/xlsx/pptx/webp/rtf/odt

Observed behavior:
- A live `report.csv` or `slides.pptx` stays on disk and in session inventory; the Outputs pane looks empty of it. Tests lock the default filter.

Expected boundary:
- Either defaults include ADR-0018 types, or the pane shows how many inventory files the extension filter hid.

Recommended mitigation:
- Align defaults with ADR-0018, or add a hidden-count status like the repository filter.

### WFG-GOS-004: Session summary says tools are unrestricted while workspace policy still gates

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `working_dir_scope_inspector.rs:45-48` — early-return only when `!restrict_tools_to_working_dirs && workspace_context.is_none()`
- `working_dir_scope_inspector.rs:64-98,147-152` — workspace sessions still Deny read-only mutations and RequireApproval for out-of-scope mutations when restrict is off
- `ui/desktop/src/components/SessionInfoSummary.tsx:190-192` — restrict off → “Tools are not restricted to the listed directories.”
- `WorkingDirectoriesMenu.tsx:81-84` — workspace description correctly says folder policy is enforced either way

Observed behavior:
- Same flag, two operator stories. Summary is the weaker, false one for workspace sessions.

Expected boundary:
- Session chrome must match inspector classification.

Recommended mitigation:
- Reuse the workspace menu sentence in the summary when `workspace_context` is present.

### STT-GOS-001: Policy Deny is recorded as “the user declined”

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: State-Transition

Evidence:
- `working_dir_scope_inspector.rs:73-80,87-98` — Deny with inspector reason
- `crates/gosling/src/agents/agent/reply_context.rs:196-208` — every denied tool gets `DECLINED_RESPONSE`
- `crates/gosling/src/agents/tool_execution.rs:79-81` — “The user has declined to run this tool. DO NOT attempt to call this tool again.”

Observed behavior:
- A read-only workspace root mutation never becomes a prompt. The transcript tells the model (and the operator reading the tool error) that the **user** refused. Approval is skipped.

Expected boundary:
- Policy Deny and user Deny are distinct states and distinct strings.

Recommended mitigation:
- Surface the inspector reason; do not use `DECLINED_RESPONSE` for inspector Deny.

Validation:
- Test: read-only mutation → error text contains “forbids mutation” and does not contain “user has declined”.

### WFG-GOS-005: Read-only workspace roots listed as full read/write/run

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `WorkingDirectoriesMenu.tsx:63-70` — “full read/write/run access inside every directory listed here”
- Desktop `Session` has no folder-access field; workspace roots are flattened into `additional_working_dirs`
- Inspector still denies mutations under `WorkspaceFolderAccess::Read` (`working_dir_scope_inspector.rs:87-98`)

Observed behavior:
- A listed read-only folder looks fully writable. The operator cannot see why a later Deny happened (and STT-GOS-001 then blames them).

Expected boundary:
- Listed directories must show the access the inspector will enforce.

### FSR-GOS-003: Auto-compact does not observe cancel before persist

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Failsafe

Evidence:
- `reply_entry.rs:106-108` — cancel checked at `reply()` entry
- `reply_entry.rs:155-162` — later work uses the turn-lease child token (caller cancel **can** fire it)
- `perform_compact_with_provider` (`reply_entry.rs:429-452`) — `compact_messages` then `replace_conversation` with no `is_token_cancelled` check
- `context_mgmt` compaction has no `CancellationToken`

Observed behavior:
- Cancel during “Performing auto-compaction…” does not abort the summarizer HTTP call. If `compact_messages` returns `Ok`, persist still runs.

Expected boundary:
- `fail_visible`: cancel stops persist; original conversation remains.

Recommended mitigation:
- Check the turn-lease token between `compact_messages` and `replace_conversation`; pass cancel into provider complete where the stack already supports it.

### FSR-GOS-004: Failure message can claim the original session is intact after persist

Severity: Medium
Confidence: Likely
Evidence basis: source-evidenced
Domain: Failsafe

Evidence:
- `reply_entry.rs:437-452` — `compact_messages` `?`, then `replace_conversation` `?`, then `update_session_metrics` `?`
- `context_mgmt/mod.rs:200-203` — `compaction_failure_message` always says “Your original session is intact”
- Callers emit that string on any `perform_compact` `Err` (`reply_entry.rs:395-400`)

Observed behavior:
- If `replace_conversation` succeeds and `update_session_metrics` fails, the operator is told the original session is intact while messages were already replaced.

Expected boundary:
- Failure copy must distinguish “compact did not persist” from “compact persisted, metrics failed.”

Recommended mitigation:
- Split errors; only use the intact sentence when `replace_conversation` did not commit.

### IAPI-GOS-001: Revision ACP errors all collapse to `invalid_params`

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Architecture

Evidence:
- `custom_dispatch.rs:661-691` — history/get/restore `.map_err(|error| Error::invalid_params().data(error.to_string()))`
- `docs/architecture.md:159-166` — taxonomy: validation → invalid params; not found → resource not found; conflict → conflict code; storage → internal error

Observed behavior:
- Unregistered path, missing version, missing file, stale hash, and 8 MiB/1000-rev bounds share one JSON-RPC class. UI shows `error.toString()` so the **string** is still useful; clients cannot branch.

Expected boundary:
- Hash conflict ≠ not found ≠ bound exceeded.

Recommended mitigation:
- Map hash mismatch to conflict, missing file/version to resource not found, bounds/storage to internal or a stable code.

### INV-GOS-001: `contentHash` is body; `currentHash` is on-disk bytes including footer

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Invariant-Sync

Evidence:
- `revision()` `content_hash: digest(body)` (`output_revisions_storage.rs:529-532`)
- `read_snapshot` `hash: digest(&bytes)` (`output_revisions.rs:177-182`)
- Restore correctly compares `expected_current_hash` to the file hash (`:422-425`)

Observed behavior:
- Two fields named “hash” in one get-response do not hash the same bytes. Restore is implemented correctly **if** callers use `currentHash`. A client that sent `revision.contentHash` as `expectedCurrentHash` would fail on every markdown file with a footer.

Expected boundary:
- Names or schema descriptions must say which bytes each hash covers; Desktop already uses `currentHash`.

Recommended mitigation:
- Rename or document in the DTO/schema; add a contract test that `contentHash != currentHash` when a footer is present and restore still succeeds with `currentHash`.

### INV-GOS-002: Reduction/threshold legal range disagrees across copies

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Invariant-Sync

Evidence:
- ACP reduction: finite `[0.0, 1.0)` (`config.rs:367-377`); pair check only on preferences
- ACP threshold: `> 0 && <= 1` (`:359-362`) so **1.0 is valid**
- CLI: warn unless 0 or in `[0.0, 1.0)` (`cli.rs:54-79`) so **1.0 is invalid**
- Runtime: `threshold >= 1.0` **disables** auto-compact (`context_mgmt/mod.rs:486-497`); `reduction >= threshold` → full collapse (`:580-582`)
- Docs: “Float between 0.0 and 1.0 (disabled at 0.0)” / reduction “less than the threshold”
- AlertBox: 1–100% threshold, 0–100% reduction via upsert (WFG-GOS-001)

Observed behavior:
- Saving threshold 100% via preferences is allowed, CLI warns, runtime disables compaction. Reduction 100% is rejected on preferences, accepted on upsert, and runtime full-collapses.

Expected boundary:
- One registry of legal values consumed by ACP, CLI doctor, UI clamp, and runtime.

### ARC-GOS-001: architecture.md claims ACP export

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Architecture

Evidence:
- `docs/architecture.md:149-154` — “typed ACP requests for history, comparison, export, and hash-checked restore”
- No `_gosling/.../outputs/export` method; UI export is `window.electron.saveArtifact` (`OutputHistory.tsx:222-232`)

AID-010 documentation drift. Fix the sentence; do not add a spurious ACP export.

### STT-GOS-002: Empty history row labeled “Unknown”

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: State-Transition

Evidence:
- `OutputHistory.tsx:251-253` — no latest revision → `unknown` unless `latestError` (`unavailable`)
- Same `unknown` string is used for missing model names (`:21`)

Empty SQLite history is not unknown authorship.

### WFG-GOS-006: “Hide repository files” also hides source-like Outputs

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `artifactRepository.ts:85-91` — `isSourceCodeFile` includes `html`/`htm`/`js`/`css`/…
- `ArtifactPane.tsx:542-551` — hide-repository filter drops those paths without a git check
- ADR-0018 treats HTML as a saved output document

### IOP-GOS-001: Compare-with-previous failure blanks the selected revision

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Input-Output-Path

Evidence:
- `OutputHistory.tsx:176-194` — `setSaved(null)` then `Promise.all([get current, get selected-1])`; any reject sets error and leaves `saved` null
- Export/Restore disable when `!saved`

### WFG-GOS-007: Green “ready” dot is unread chat activity, not workspace health

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `NavigationPanel.tsx:279-291` — `readyWorkspaceIds` = unread + not streaming/error
- Tooltip is honest (`chatReady`: “A chat in this workspace has a new reply”, `WorkspaceSidebarSection.tsx:29-32,300-306`)
- Tests and commit message still say “workspace readiness”; a green dot next to validation warnings reads as health

## Non-Findings / Checked But Not Confirmed

| Seam | Why it held | Line |
|---|---|---|
| Restore will not silently recreate a missing file | `read_snapshot` None → error; UI disables Restore without `currentHash` | `output_revisions_storage.rs:417-421`, `OutputHistory.tsx:414` |
| Restore hash mismatch is refused and surfaced | CAS on live hash; UI shows error; no `onRestored` | `:422-425`, `OutputHistory.test.tsx` |
| Failed **persist** does not commit restore | tested `failed_restore_does_not_commit_the_external_edit_baseline` | tests |
| Copy contents does not toast success or write clipboard on failure | checks before `clipboard.writeText`; renderer toasts error | `fileIpc.ts:328-388`, `ArtifactPane.tsx:707-733` |
| Trash never falls back to unlink | `shell.trashItem` only | `fileIpc.ts:574-576` |
| Capture failure does not retry the tool | appended to successful tool content | `tool_dispatch.rs:288-290` |
| UI saved-history is SQLite/ACP, not the live footer | `outputRevisions.ts` ACP only | `outputRevisions.ts:7-57` |
| Footer-only edits do not invent a contributor when the footer parses | equality uses `body` | `output_revisions_storage.rs:211-214` |
| Same-event duplicate revisions | `UNIQUE(path, event_id)` | schema + `insert_revision` |
| Revision ACP method names / DTO field lists agree | schema, Rust, generated SDK, Desktop client | INV-001..007 held |
| `ui/desktop/src/api` not reintroduced | no OpenAPI client for new ACP methods | AGENTS.md |
| Compact `compact_messages` Err does not persist | `replace_conversation` only after Ok | `reply_entry.rs:437-449`; unit test preserves original |
| Manual `/compact` loads full conversation | CLI/HTTP `compacted_context: false` | `gosling-cli`, `reply_service.rs` |
| Inspector failure does not claim in-scope | RequireApproval with failure sentence | `tool_inspection.rs` (workflow only) |
| Always Allow hidden when a prompt exists | Desktop and ACP withhold tool-wide always | `ToolApprovalButtons`, `tool_events.rs` |
| Repository-filter IPC failure fail-opens (files stay listed) | catch sets unavailable, does not hide | `ArtifactPane.tsx:524-526` |

## Inventory dispositions (change-set only)

Codes not listed are `Not Reviewed — out of change-set scope` unless marked N/A.

**Reliability REL-001..015:** REL-007 Crash Mid-Operation / REL-011 Partial Output → REL-GOS-001. REL-010 Error Swallowed → STT-GOS-001. REL-015 Missing Operator Signal → WFG-GOS-001 / FSR-GOS-004. Failed compact before persist: non-finding.

**Failsafe FSR-001..016:** FSR-005/007/015 → FSR-GOS-003. FSR-009/014 → FSR-GOS-001. FSR-012/013 → FSR-GOS-004 / WFG-GOS-001. Trash unlink fallback: held.

**Data integrity DAT-001..015:** DAT-001/015 → DAT-GOS-001. DAT-007 → FSR-GOS-001 / CAS-GOS-001. DAT-010 footer vs SQLite: noted under FSR-GOS-001 capture (Low).

**Cascade CAS-001..015:** CAS-006/010 → CAS-GOS-001 and REL-GOS-001. CAS-013 stale artifact: `deletedArtifacts` (DAT-GOS-001). Bound note on tool result: degraded-honest, not fake tool success.

**Concurrency CON-001..018:** restore/capture `acquire_write_guard`: held. Dual-process TOCTOU on `persist`: ADR-accepted, Likely only, not a separate High finding.

**Temporal TMP-001..015:** TMP-007 restore hash re-check: held for detected change. TMP-001 timestamps without watcher: Low, not filed separately (IOP timestamps).

**State STT-001..012:** STT-002/005 → STT-GOS-001, STT-GOS-002. Restore gate: held.

**Workflow WFG-001..015:** WFG-001 fake success → copy/trash/restore mostly held; compaction complete on full-collapse-when-reduction-ignored → WFG-GOS-001. WFG-006 → WFG-GOS-002. WFG-008 → WFG-GOS-007. WFG-004 stale display → DAT-GOS-001 hide list.

**Invariant INV-001..015:** INV-004/009/011 → INV-GOS-002, WFG-GOS-001. INV-005 hash field semantics → INV-GOS-001. Revision method names: non-finding.

**Architecture ARC-001..025 / AID-001..014:** ARC-010 UI owns grant buttons (permission options not consumed) — stubbed under WFG-GOS-004/005, not deep-audited as ARC-010 exploit. ARC-013 frozen ACP revision DTO: held (generated SDK). AID-001 N/A (no registry). AID-009/010 → ARC-GOS-001. AID-013 N/A (no baseline).

**IAPI-001..016:** IAPI-006 → IAPI-GOS-001. IAPI-015/016 → WFG-GOS-001. IAPI-004 revision DTO: non-finding (single generated type).

**AOC:** HistoryReplaced dropped on ACP prompt loop (`prompt_execution.rs` `Ok(_) => {}`) — Desktop uses notices, not a history rewrite. Recorded as non-finding for Desktop notices; CLI/HTTP apply HistoryReplaced. Compacted-resume persist is filed as REL-GOS-001, not AOC.

**NEG:** Compacted-resume + auto-compact composition is REL-GOS-001 (reachable today, not speculative). Future multi-user path-keyed history: Speculative, not filed.

**IOP-001..015:** Copy path traversal/symlink: held (`O_NOFOLLOW`, artifact grant). IOP-012 partial copy: held. IOP-015 CLI/API/UI: compaction prefs (INV-GOS-002).

## Pipeline graph (static)

```
Desktop loadSession(compacted,50) → ACP register compacted_context
  → prompt → reply() → get_session_for_compacted_resume
  → check_if_compaction_needed(full usage) → compact_messages(tail)
  → replace_conversation (DELETE messages) → "Compaction complete"
```

```
Mutating tool → prepare_output_capture → tool execute → finish_output_capture
  → SQLite insert + file persist + commit → ACP history/get/restore
  → Desktop OutputHistory / Electron saveArtifact export
```

Deliberate paths (unexecuted): A success capture+restore; B bound exceeded; C restore missing file. Randomized paths not generated (static/no-tests mode).

## Break-It Review

| Attack | Surface | Result |
|---|---|---|
| Compacted resume + high usage | auto-compact persist | **REL-GOS-001** (static trace) |
| Persist then kill before commit | restore/capture | **FSR-GOS-001** (order confirmed; drill not run) |
| Save reduction ≥ threshold via AlertBox | config upsert | **WFG-GOS-001** |
| Oversized sibling in Outputs | capture | **CAS-GOS-001** |
| Restore missing file | restore | held (error + disabled button) |
| Restore stale hash | restore | held |
| Copy binary / oversize / mid-change | copy IPC | held (throw before clipboard) |
| Trash symlink/directory | trash IPC | held (refused) |
| Double restore same hash | restore | held (CAS) |
| Footer-only rewrite | capture | held (body equality) |
| Workspace restrict-off mutation outside folders | inspector | still prompts (held); summary lies (**WFG-GOS-004**) |
| Read-only root mutation | inspector | Deny as user decline (**STT-GOS-001**) |
| Cancel during compact | reply | persist can still run (**FSR-GOS-003**) |

Runtime races, kill drills, and live Desktop were not executed.

## Skill Escalation

| Trigger | Sibling | Action |
|---|---|---|
| Working-dir canonicalize / scratch / symlink | audit-security | **not run** (operator exclusion); stub only |
| ACP option list vs UI buttons (Always Deny unused) | audit-security / IAPI | stub; workflow filed as WFG/INV |
| Provider summarizer hallucination | audit-security-llm / AOC-004 | not a new control; compaction trusts model output by design |
| `delete-file` IPC still unlinks | audit-workflow-gui | not used by artifact pane; session archive only; Not Reviewed |
| Compaction retry cartesian product | audit-reliability REL-004 | noted Low/Medium; not a separate High (caps exist) |
| Deadcode / unused Always Deny option | audit-deadcode-cleanup | deferred |

## Recommended Patch Order

1. **REL-GOS-001** — refuse tail-conversation `replace_conversation` (or load full history first). Highest severity, Desktop default path.
2. **FSR-GOS-001** — restore/capture commit vs persist ordering.
3. **WFG-GOS-001 / INV-GOS-002** — validate compaction pair on `config/upsert` and AlertBox; honest full-collapse signal.
4. **FSR-GOS-003 / FSR-GOS-004** — cancel check before persist; honest failure copy.
5. **WFG-GOS-004 / WFG-GOS-005 / STT-GOS-001** — session chrome and Deny string match the inspector.
6. **CAS-GOS-001** — per-file observation skip.
7. **DAT-GOS-001 / WFG-GOS-002 / WFG-GOS-003** — trash/history/filter operator truth.
8. Docs/schema: ARC-GOS-001, IAPI-GOS-001, INV-GOS-001, Low UX labels.

## Regression Test Strategy

| Test | Purpose | Finding |
|---|---|---|
| Compacted resume + usage above threshold + `total_count > tail_limit` does not DELETE unloaded messages | persist guard | REL-GOS-001 |
| Persist-success / commit-fail restore leaves file **or** committed baseline+restore | dual-store | FSR-GOS-001 |
| `config/upsert` of reduction ≥ threshold is rejected | live UI path | WFG-GOS-001 |
| Two Outputs files, one over 8 MiB: small file still recorded | containment | CAS-GOS-001 |
| Read-only mutation error is not `DECLINED_RESPONSE` | status truth | STT-GOS-001 |
| Session summary workspace+restrict-off does not say “not restricted” | chrome | WFG-GOS-004 |
| Cancel between compact_messages Ok and replace is a no-op persist | interrupt | FSR-GOS-003 |
| Metrics fail after replace does not say “original intact” | signal | FSR-GOS-004 |
| Restore with `contentHash` vs `currentHash` on footered markdown | hash semantics | INV-GOS-001 |

## Deferred Risks

- Empty-text summarizer response treated as compact success (Plausible; provider `complete` rejects empty **content array**, not `Text("")`)
- Tool-pair hide overwritten by later in-memory compact in the same turn (Low; CAS-GOS-002 from compaction walk)
- Hash-conflict restore does not auto-refresh `currentHash` (Low)
- Path/reveal/open-external ignore IPC results (Low; copy-contents is held)
- Shared dialog z-index 10000 (residual; in-flight locks exist)
- `GOSLING_COMPACT_PROTECT_LAST_N_TURNS` docs “default 2” vs code default 10
- Permission Always Deny on the wire but never in Desktop/CLI
- Session-config hand type vs generated `SessionInfo` (pre-existing IAPI-004)
- No revision pruning UI (ADR-disclosed; database growth)

## Validation Limits

- No `cargo test` / `pnpm test` / live Desktop run in this engagement (read_only, no-tests)
- Oracle-integrity fresh-process entrypoint was **not** run; do not treat existing green suites as proof REL-GOS-001 is absent
- Crash manifestation of FSR-GOS-001 capped Likely (`requires-authorized-drill`)
- Security of path canonicalization, scratch dirs, and permission grants: Not Reviewed
- Ink/CLI artifact UI: N/A (no parallel history UI)
- `ui/text` compaction UX: Not Reviewed
- Transport policy / session lease (`cb72aabce`): Not Reviewed (security-adjacent)
- HEAD commit `1a2504e05` already contains a same-day system-surface audit report and some preference-pair tests; this document does not reuse that report’s findings and treats the **current tree** as the target

## Final Confidence

**Medium-High** for the change-set’s data-flow/custody/workflow claims that were source-traced end-to-end (compacted-resume persist, dual-store order, upsert bypass, inspector vs chrome). **Medium** overall because runtime drills and Desktop playtest were not run, and security was excluded.

## Assumptions register

- Involvement `L2` from request sophistication; autonomous execution of a read-only audit
- Compacted ACP load is in scope as one hop from today’s `reply_entry.rs` auto-compact budget work
- “Pause” in the verdict applies to Desktop auto-compact-after-resume until REL-GOS-001 is guarded, not to unrelated artifact UI
