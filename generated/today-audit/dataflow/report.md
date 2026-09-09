# Independent dataflow audit — 2026-09-08

## Executive verdict

Two Medium, source-evidenced defects remain in today's output-history work: retrieval of immutable saved bytes depends on the live file remaining readable and under the capture limit, and bounded before-observation can be mistaken for proof of creation. Both merit narrow repairs. No security audit was performed. Static evidence supports deterministic code properties; no runtime reproduction has yet been claimed.

## Scope and draft prompt assessment

Target: `/Users/eric/Work/vscode/forked/gosling`, main, clean at `a48108750`. Change range: `a6ee677a6^..a48108750`, commits since 2026-09-08 midnight America/Denver. Applied catalog `audit-dataflow-integrity` and its shared contracts. Budget: focused review of output revisions, ACP producer/consumer seams, compaction preference/state changes; about 20 source/document files. This independent pass read earlier repair records for orientation but independently traced current source; it is not a blind consensus vote. Only report artifacts were written during audit.

Read AGENTS.md first, then README.md and docs/INDEX.md (GEMINI.md absent), relevant architecture, ADR-0018 and compaction design, .giles/repo.yaml advisory metadata, and today's repair log. Other .giles advisory mirrors and unrelated architecture documents were not material to the scoped dataflow mechanisms. Commands: git log/date range, git diff/stat, git status --short, rg, cat and numbered source reads. No builds/tests run during initial audit. Fresh-process production replay remains unperformed; existing passing tests were not used as runtime evidence.

## Surface inventory and boundary map

| Entity/surface | Source and owner | Writers/readers | Scope and provenance | Integrity boundary |
|---|---|---|---|---|
| Output revisions | Tool before/after snapshots; core | capture/restore; ACP history/get | canonical path/version; request-message inference and chat identity | immutable rows, bounded capture, unknown attribution when evidence absent |
| Revision bytes | source file and SQLite content | capture/restore; Desktop export | canonical path/version; base64 preserves bytes | saved retrieval independent of live file fitness |
| Session artifacts | successful tool discovery; core | capture upsert; inventory/authorized_output | session ID/resolved path | inventory membership plus canonical folder scope |
| Compaction preferences | Desktop/ACP settings; Config | typed and generic config setters/removers; context runtime | threshold/reduction pair | validate resulting pair before mutation |
| Compacted conversation | provider summary; Agent | perform_compact; persisted resume | session ID and compacted_context flag | never overwrite unloaded durable history with paged tail |

## Findings table

| ID | Severity | Confidence | Basis | Domain | Priority | Blast radius | Complexity/cost |
|---|---|---|---|---|---|---|---|
| DAT-TODAY-001 | Medium | Confirmed | source-evidenced | Data-Integrity | 1 | Workflow | local_guardrail / S |
| DAT-TODAY-002 | Medium | Confirmed | source-evidenced | Data-Integrity | 2 | Local | local_guardrail / S |

## Detailed findings

### DAT-TODAY-001: Saved revision export depends on live-file capture eligibility

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Data-Integrity

Evidence:
- `crates/gosling/src/session/session_manager/output_revisions_storage.rs:443-455`: SQLite saved content is read first, then `let current_hash = tokio::task::spawn_blocking(move || read_snapshot(&path)).await??` propagates any current-file snapshot error before returning saved bytes.
- `crates/gosling/src/session/output_revisions.rs:161-164`: `metadata.len() <= MAX_OUTPUT_REVISION_BYTES as u64` rejects a current file over 8 MiB.
- `ui/desktop/src/components/artifacts/OutputHistory.tsx:186-191,229-236`: get failure never populates `saved`; export returns immediately without `saved`.

Observed behavior:
- A valid saved small revision cannot be fetched/exported after the current regular file grows above 8 MiB, even though the saved blob is unchanged and authorization still holds.

Expected boundary:
- Saved revision retrieval serves committed bytes; optional current hash controls restore eligibility, not availability of immutable history.

Failure mechanism:
- The endpoint combines independent saved-content retrieval with mandatory bounded live capture via double error propagation.

Break-it angle:
- Capture a small report, grow its live contents beyond 8 MiB, fetch version 1. Authorization succeeds; saved blob fetch succeeds; snapshot limit aborts response.

Impact:
- Recovery/export is unavailable when live output exceeds a capture bound. Current unreadable regular files have the same coupling.

Operational impact:
- Blast radius: Workflow; side-effect class: user-visible; reversibility: reversible; operator visibility: UI-visible; rerun safety: safe.

Adjacent failure modes:
- Unreadable live file blocks historical preview; restore must remain disabled when no reliable current hash exists.

Recommended mitigation:
- Pattern: separate durable retrieval from optional live metadata. Return valid saved bytes with `current_hash: None` if live snapshot cannot be obtained, after retaining existing authorization and saved-row checks. Keep restore's strict independent snapshot and expected-hash checks.
- Local guardrail: UI already disables restore without a hash; preserve that semantics.

Implementation assessment:
- Complexity: local_guardrail; cost: S; cost drivers: modules, tests; nominal implementation agent: codex.
- Rationale: one endpoint and targeted integration assertions; no schema or access-policy change required.

Validation:
- Save known bytes, enlarge live file beyond limit, verify fetched base64 bytes and metadata exactly match saved revision and current_hash is absent.
- Normal current-file hash remains present; missing-file export remains available.
- Restore still rejects oversized/unreadable current files and incorrect expected hashes without changing bytes/history.

Non-goals:
- Raise snapshot limits, weaken authorization, change restore policy, or redesign UI.

### DAT-TODAY-002: Skipped pre-images become false creation attribution

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Data-Integrity

Evidence:
- `crates/gosling/src/session/session_manager/output_revisions_storage.rs:138-156`: over-budget and failed pre-reads `continue` without recording that the file existed; known missing and unobserved share absence from `before`.
- Same file `205-220`: finish rescans roots and may admit a previously skipped file after sizes change.
- Same file `237-239,292-309`: `before.is_none() && history.is_empty()` selects `OutputRevisionAction::Created`; absent concurrent history selects the current contributor and Observed/Tool attribution.
- Same file `315-324`: those claims are persisted and Markdown can receive a managed footer.

Observed behavior:
- When before/after budgets admit different sets, an unchanged pre-existing file can receive a Created revision credited to the current agent.

Expected boundary:
- A skipped observation is unknown evidence, not proof of absence or a content change. ADR-0018 excludes unchanged files from authorship.

Failure mechanism:
- Absence from a bounded snapshot map conflates missing, skipped, and unknown. A second independently bounded scan feeds that ambiguous state into canonical provenance.

Break-it angle:
- Create a.txt,b.txt,c.txt,d.txt at 8 MiB each and z.txt at one byte under Outputs. A hosted write shrinking a.txt to one byte admits four big pre-images but skips z.txt before; afterward all five fit. Untouched z.txt is recorded as Created by this agent.

Impact:
- False durable attribution and created inventory relation; a Markdown variant can be rewritten to add false history.

Operational impact:
- Blast radius: Local; side-effect class: DB; reversibility: compensatable; operator visibility: silent (generic bound warning does not identify false attribution); rerun safety: safe.

Adjacent failure modes:
- Transient pre-read error followed by readable after-image; entry/document scan limits causing unknown pre-images.

Recommended mitigation:
- Pattern: explicit observation state. Preserve known-missing versus skipped/unknown pre-state; do not credit, invent Created/Modified, or append a footer when an unobserved pre-image cannot establish a change. Preserve normal known creations and independently observed siblings.
- Local guardrail: report bounds while refusing unsupported provenance promotion.

Implementation assessment:
- Complexity: local_guardrail; cost: S; cost drivers: modules, tests; nominal implementation agent: codex.
- Rationale: capture state and selection logic plus deterministic real-file boundary tests; no migration needed.

Validation:
- Execute described 32 MiB boundary case; assert no revision or artifact creation for unchanged z.txt, and its exact bytes survive.
- Known missing explicit target still receives Created; normal mutation still records baseline/Modified; eligible siblings remain recorded when another before-read fails.
- Include observation-limit and unreadable-before variants as practical.

Non-goals:
- Continuous filesystem tracking, retrospective history correction, or raising observation bounds.

## Inventory result / non-findings

| Required item | Disposition |
|---|---|
| DAT-001 scope leakage | Not confirmed in reviewed path: authorized_output checks session membership after canonical folder validation (storage.rs 371-396). Security analysis excluded. |
| DAT-002 duplicate entity | Not confirmed: PRIMARY KEY(path,version), UNIQUE(path,event_id), storage.rs 35-39. |
| DAT-003 orphaned record | Not confirmed: revision retention independent of session deletion is deliberate ADR-0018 behavior. |
| DAT-004 lost provenance | DAT-TODAY-002 concerns false provenance promotion, not message inference loss; request metadata fields are carried at storage.rs 95-113. |
| DAT-005 corrupt merge | No new supported defect in sampled paths; updates append rather than overwrite revisions. |
| DAT-006 incorrect normalization | No new supported defect: canonical path used in shared read/write path; Markdown body parser preserves pre-footer bytes (output_revisions.rs 197-218). |
| DAT-007 partial persistence | Existing SQLite/filesystem limit acknowledged in ADR-0018; restore commits durable snapshots before replacement (storage.rs 535-542). No crash drill performed. |
| DAT-008 migration meaning loss | No new supported defect: v32 introduces a new table (migrations.rs 783), not reinterpretation of existing revision rows. |
| DAT-009 round-trip loss | DAT-TODAY-001; export blocked even though stored bytes exist. |
| DAT-010 stale derived data | DAT-TODAY-002; wrong provenance is persisted and shown in history. |
| DAT-011 evidence misclassification | DAT-TODAY-002. |
| DAT-012 advisory misrepresented | DAT-TODAY-002. |
| DAT-013 silent constraint violation | No new supported defect: version ceiling checked by insert_revision; uniqueness enforced by schema. |
| DAT-014 cross-batch contamination | No new supported defect in sampled session artifact membership path. |
| DAT-015 weak data promoted | DAT-TODAY-002. |

Compaction non-findings: resulting-pair validation precedes set_param_values in config.rs 62-64 and removals in 83; generic update/reset routes also call validation (141,176). Paged compacted_context avoids replacing unloaded durable history in reply_entry.rs 459-466. These are source observations, not test-passed claims.

## Break-it review and patch order

Attacked live-file drift against immutable retrieval, observation-budget drift against provenance, repeated capture against uniqueness, failed restore against durable recovery, partial config update against runtime pair constraints, and paged-session compaction against history replacement. Repair DAT-TODAY-001 first, DAT-TODAY-002 second, then run the focused integration suite and source review. No cross-file concurrency runtime claim was made.

## Skill escalation

| Finding | Primary lens | Secondary lens | Reason |
|---|---|---|---|
| DAT-TODAY-001 | Data integrity | Reliability / workflow | Failed optional metadata blocks recovery/export UI. |
| DAT-TODAY-002 | Data integrity | Temporal / input-output | Incomplete before scan becomes after-state authorship and footer mutation. |

## Validation limits, residual risk, next action

Initial audit is static only; no new fresh-process replay, production data, live Desktop, or platform failure drill. Full ACP transport and all compaction algorithms were sampled, not exhaustively verified. Observation-scan entry limits and transient failures need regression extensions. Non-scope: security, unrelated provider routes, whole-repository migrations. Final confidence: high in the two deterministic source properties; runtime validation pending. Next action: authorized narrow repair and isolated integration regression runs.

## Authorized repair handoff / stage plan

Parent authorized repair of these two findings in output_revisions_storage.rs and output_revisions_test.rs, with output_revisions.rs only if necessary. Stage 1: load repair workflow and run focused baseline suite. Stage 2: add regressions, execute them against original code to prove failure. Stage 3: repair saved retrieval and explicit capture evidence state; run regressions and focused suite. Stage 4: cargo fmt, relevant clippy, diff check, closing independent review by parent. Report precise command outcomes below; no other target files will be edited by this worker.

Repair gates 0–3 supplement: both findings are data integrity, P2, medium complexity. One locality group owns `get_output_revision`, `prepare_output_capture`, `finish_output_capture`, `OutputCapture`, and output revision integration tests. Independent retrieval and capture changes share the persistence module but no schema/API fields. Active declarations: ADR-0018 accepted (saved bytes, attribution), custom_requests.rs (optional current_hash), existing capture/restore tests; baseline has the two documented drifts. Intended delta: no new drift. Keep all folder/membership checks, restore error paths, normal hashes, baselines, known creation, failed-tool and unchanged-file behavior. Cargo execution is serialized with another worker because both use the same target directory; stages checkpoint in this report. Test fixtures create fresh temp workspaces and SessionManager state, without global pragma/reset changes. Expanded scan-state repair will preserve skipped pre-images and incomplete scan status, while keeping explicit known-missing targets eligible.

Gate 0 baseline: `source bin/activate-hermit; cargo test -p gosling --test output_revisions_test --locked` passed 22/22 (`baseline.log`). Gate 4 regression-first proof: same command with three added regressions failed exactly those three, while 22 existing tests passed (`regressions-before.log`). Oversized current file returned the capture Limit; both unknown-preimage cases appended a Created/Observed footer to unchanged or unproven content. Fixture state is per-test TempDir/SessionManager; tests do not modify global config/pragmas. Findings now have test-reproduced evidence for these specific cases. Narrow source patch applied; awaiting post-change union tests.

Stage execution: 19-line storage delta adds skipped-path set and initial scan completeness; known absent explicit candidates retain creation, captured siblings retain comparison/baseline behavior. Saved retrieval catches ordinary snapshot errors only after canonical folder and session-inventory authorization and successful saved-row retrieval; spawn failure still propagates. No restore implementation, schema, dependencies, or generated files changed by this worker. New tests cover the two defects plus incomplete scan guardrail (three total).

Gate 6 source inspection: traces `prepare` → `finish` → history/inventory → footer and ACP get → Desktop `saved` → export/restore. Snapshot failures no longer block saved bytes; live restore still snapshots independently and validates full hash. Skip state is evaluated before all DB writes and footer rewrites. Existing failure warnings remain visible; known missing explicit targets still record even with unrelated directory-scan warning. Generated current_hash documentation is handled by the parent/architecture worker, outside this worker's source ownership. Scoped `git diff --check` passed; no TODO/FIXME/HACK/XXX markers in touched source/tests. Read relevant July .giles advisory excerpts; retained their historical status without any compliance claim.

Gate 5 initial union: `cargo test -p gosling --lib --test compaction --test output_revisions_test --locked` passed 1883 library tests (3 ignored), 10 compaction tests, 25 output tests (`union-after.log`). Gate 8 independent reviewer `/root/repair_review` identified a same-finding completeness gap: prior root/candidate canonicalization errors had been silently discarded before scan completeness was computed. Parent authorized extending DAT-TODAY-002 in output_revisions.rs. Unix permission recovery regression reproduced this gap before its repair (`permissions-before.log`); target directories are fresh fixtures and permissions restored with an RAII guard before assertions.

Refined group: checked output-root resolution distinguishes NotFound from unavailable roots; candidate non-NotFound failures also invalidate before-observation completeness. Existing output_roots callers preserve their vector behavior. Missing roots/parents remain eligible for later legitimate creation, covered by an added guardrail. Repair re-enters Gates 4–6; original green evidence retained but not reused as final proof of the refined source.

Behavioral-equivalence gate caught a regression in the first canonicalization refinement: output suite passed 26 but failed the existing read-only-root test because known policy exclusions were reported as unknown IO failure (`final-output-tests.log`). Parent independently identified the same issue. Narrowed the branch to non-NotFound std::io::Error only; policy/type exclusions retain their prior silent skip behavior. Added adjacent read-only configured output plus legitimate new shell output coverage. This failed intermediate attempt is retained as evidence; it is not the final state.

## Final dataflow repair disposition — 2026-09-08

- DAT-TODAY-001: repaired and integration-verified. Saved blobs and DTO metadata remain available when the current regular file exceeds the capture limit; hash becomes absent and restore continues rejecting the live file. Original safe retrieval, missing-file export, authorization, stale-hash and exact-byte tests remain green.
- DAT-TODAY-002: repaired and integration-verified. Budget-skipped snapshots and incomplete before scans cannot become new provenance or footers. Actual non-NotFound IO failures in root/candidate resolution contribute to incompleteness. Known policy/type exclusions keep their previous behavior, and known newly created parents remain captureable. Tests verify original bytes and persisted row counts, plus eligible sibling and known-creation behavior.

Final commands from repository root using `source bin/activate-hermit`:

| Command | Outcome | Evidence |
|---|---|---|
| `cargo test -p gosling --test output_revisions_test --locked` | 28 passed | final-output-tests-v2.log |
| `cargo test -p gosling --lib --test compaction --locked` | 1884 library tests passed, 3 ignored; 10 compaction tests passed | final-core-tests.log |
| `cargo fmt` | passed | command completed before final executions |
| Scoped `git diff --check` | passed | command completed after final executions |

Parent owns combined final Gosling/CLI Clippy and aggregate documentation/session-log closure; this worker did not claim that pending scan passed. Earlier union library count was 1883 before the reliability worker's last lease regression; final count 1884 is the current execution. Original before-fix failures and intermediate read-only behavior regression remain preserved in their logs. Final tests ran on macOS as the normal non-root user; Unix EACCES fixtures do not prove root-user or Windows parity. No live Desktop/OS export click was performed, and no whole-workspace or all-crate test claim is made.

Gate 8 independent review: `/root/repair_review` found and then source-reviewed the canonicalization uncertainty fix; parent independently caught the overly broad handling of known exclusions. Both issues were reproduced/covered and corrected. Gate 9 separate completeness inspection confirmed the original reproduction cases now pass, non-defect capture/restore paths remain covered, and no source TODO marker describes a repaired defect. No unrelated target source was changed by this worker.

Architecture/contract comparison: active ADR-0018 and typed optional current_hash contract remain authoritative; no schema, persisted format, access grant, restore policy or dependency changed. Root-vector output is preserved for existing callers; capture additionally obtains warnings. Baseline drift in recoverability and attribution is corrected; **no new drift** in the reviewed declarations. The parent coordinates current_hash description propagation through generated SDK/schema and durable aggregate docs.

Status: completed_with_partial_verification (local repair and targeted/core tests verified; parent combined Clippy/aggregate records pending at this handoff, live UI and other-platform behavior untested). Next action: parent final combined static validation and aggregate repair report. Original finding descriptions above remain historical; this disposition is their current status.

Final covered source SHA-256:

- `crates/gosling/src/session/output_revisions.rs`: `0e67b419800b9ddbb9d34f5211b5aab036d12cd9e596765ed03fb0d0bbff0a7b`
- `crates/gosling/src/session/session_manager/output_revisions_storage.rs`: `b08218ef5d839eddb6d11813a44872f4fd88adf5a35b55b0650ac737dcb968e8`
- `crates/gosling/tests/output_revisions_test.rs`: `ec99d454e74064eeec26be638abff5c833cb7b7e36edd16eb7217b5ce28d9bcc`
