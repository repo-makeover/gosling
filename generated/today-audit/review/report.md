# Independent repair review — checkpoint

Date: 2026-09-08. Base HEAD: a48108750945e42509164980e49ad452c3e12e79.
Scope: read-only source review of reliability, dataflow, locale/test-label, folder-policy propagation and IPC contract repairs. Security audit excluded. Workflow lane still changing at this checkpoint.

Skill: catalog `audit-reliability`, discovered by task search, with shared audit/evidence/calibration/engagement contracts. Repository AGENTS.md read first; README, docs index, architecture and compaction plan inspected. GEMINI.md absent; Giles metadata treated as advisory. Source hashes are in checkpoint-sha256.txt. This is not a final whole-union approval.

## Surface and boundary inventory

| Operation | Dependency / failure | Guard / signal inspected |
|---|---|---|
| Manual compact | Provider error, caller cancellation | Cancellation before save, terminal-error metadata |
| Automatic compact entry / loop / context recovery | Provider failure, cancellation | Existing bounded compact helper; cancellation distinguished from failure |
| ACP prompt consumption | Error represented as ordinary text | Metadata projection after visible content; Failed persisted before return |
| CLI consumption | In-band terminal failure | Existing terminal_error_reason metadata reader and failed machine output |
| Revision capture | Preimage read/scan limits | Unobserved set + incomplete scan suppress attribution |
| Saved revision fetch | Live bytes oversized/unreadable | Saved DB bytes returned; optional live hash; restore remains strict |
| Working directories | Server policy vs renderer snapshot | Add/remove response carries effective policy through typed SDK and menu state |
| Artifact title refresh | File revision, same display basename, >200 paths | Absolute paths, per-revision cache, bounded batches |
| IPC channels | Main/preload drift | Same registry values preserve deployed wire strings |

## Review result

No additional confirmed defect in the reliability patch from source review. This does not establish full ACP/CLI runtime coverage. Manual cancellation is checked both in biased select and after provider result; failure text carries terminal metadata; ACP prioritizes structured content errors then metadata. The ACP terminal-state branch persists Failed for stream_error and Cancelled for cancellation. Existing CLI metadata handling consumes the new signal.

Saved revision read now tolerates a failed live snapshot while retaining the existing path/session gate; OutputHistory disables Restore without currentHash, and restore_output_revision still reads and compares current bytes. Export/preview remains possible for the repaired oversized-live-file scenario.

One repair-completeness gap was independently escalated to parent and dataflow owner before closure:

### REVIEW-DAT-001: Canonicalization uncertainty bypasses incomplete-preimage protection

Severity: Medium. Confidence: Likely. Evidence basis: simulation-reasoned.

At this checkpoint, output_revisions_storage.rs prepare loop still uses `let Ok(path) = canonical_output_path(...) else { continue; }`, while output_revisions.rs output_roots uses `.filter_map(|root| root.canonicalize().ok())`. These failures do not contribute to scan_complete. A temporarily inaccessible existing output root/candidate that becomes readable during the operation can therefore reappear without a before image and be assigned Created attribution. Filesystem permission/interleaving manifestation was not independently executed. Missing parents for genuinely new output are a legitimate countercase and must not be confused with failed observation.

Repair recommendation: carry non-NotFound root/candidate resolution uncertainty into observation completeness; preserve established-absent direct targets. Regression: isolated temporary root made inaccessible before prepare and restored before finish; unchanged human file must not acquire revision attribution or footer. Owner is adding coverage; closure pending.

## Non-findings and break-it review

- REL-009/010/011/015: terminal compaction errors have explicit consumers and failed persistence; no false-success route found in reviewed branches.
- REL-007/008/012: provider-side cancellation before compact result does not save; post-commit cancellation is not proved reversible and is outside the guarantee reviewed.
- REL-005/013: title requests chunk at 200; revision capture retains 8 MiB per-file and 32 MiB aggregate bounds.
- REL-001/002/004/006/014: startup/health/retry storms/general timeout/default configuration surfaces not changed by these repairs and not reviewed in this bounded pass.
- Credits-exhausted structured error retains priority over generic terminal metadata.
- ArtifactFileList label-only edits preserve behavioral assertions for selection, cancel, partial failures, absent files and duplicate submit.
- Locale diff refreshes English fallback messages and one added key; source hash changes accompany extraction. No claim of translation quality.
- Main/preload registry conversion preserves all four literal channel values.

## Validation executed independently

Ambient pnpm failed engine preflight (10.6.4 versus required >=10.30.0); no test ran in that attempt. After sourcing repo Hermit, `pnpm test:run src/components/artifacts/ArtifactFileList.test.tsx src/components/artifacts/ArtifactPane.test.tsx src/components/WorkingDirectoriesMenu.test.tsx src/acp/__tests__/sessions.test.ts` passed 4 files / 61 tests. Log: /tmp/gosling-evening-independent-review-ui.log; start 20:33:32 local, duration 10.92s. Source reviewed alongside tests; this is a mocked renderer harness, not a fresh installed Desktop run.

Compaction fixtures were inspected: session rows are independently created, threshold override uses env_lock and serial tests, no database pragma mutation is present in the setup functions. Rust builds/tests are parent-coordinated; none claimed as independently rerun here.

## Skill escalation and patch order

| Finding | Primary | Secondary | Action |
|---|---|---|---|
| REVIEW-DAT-001 | Data integrity | Reliability / IO | Resolve preimage uncertainty before final union validation |

Next checkpoint: review final dataflow correction and workflow union, then bind report to final file hashes and parent test logs. No source files edited by this reviewer.

## Second review checkpoint — internal cancellation and workflow

### REVIEW-REL-001: Lease cancellation is invisible to terminal outcome consumers

Severity: Medium. Confidence: Likely. Evidence basis: simulation-reasoned.

Source trace: reply_entry.rs:164–169 replaces the caller token with the lease child token; session_leases.rs:121–128 cancels that child on lease revocation. Added compaction cancellation branches return without terminal error (reply_entry.rs:193–195, 402–404; reply_stream.rs:242–244, 843–846). ACP prompt_execution.rs:390 checks its original token only; lines 455–461 then choose Completed absent stream_error. CLI session/mod.rs:1370–1385 likewise checks its original token. Child cancellation does not cancel its parent. Consequently a lease-revoked compaction can end without a failed/cancelled outcome reaching the consumer. Source confirms missing signal; an actual competing-process lease takeover was not reproduced. The general reply loop already shares this adjacent weakness; the new cancellation suppression exposes the compaction variant.

Recommended guardrail: distinguish caller cancellation from internal lease revocation and publish a terminal failure for the latter, or expose a typed cancellation outcome to callers. Test should revoke the child lease while caller token remains live, then assert ACP fails/cancels and never persists Completed. User-cancel path must retain Cancelled behavior. Escalated to parent before approval.

Workflow source review: BaseChat emits latest visible text assistant ID through sessionActivity; NavigationPanel compares against pre-stream ID, ignores user-count-only changes, acknowledges focused visible chat on navigation/focus, and retains other sessions' unread state. Stored message IDs are supplied by ACP adapter messages.ts and metadata path. Manual /compact confirmation is visible text with a fresh stored ID and therefore counts as a readable reply; automatic compact summaries/statuses do not. This behavior was explicitly reported to parent as product semantics, not silently assumed excluded.

ArtifactPane uses resolved absolute paths both for IPC requests and title lookup, invalidates on lastSeenAt/modifiedAt or focus/restore event, and ignores stale async results using effect cancellation. OutputHistory restore dispatches the same ARTIFACT_TIMESTAMPS_REFRESH_EVENT consumed here. New test cases cover basename collision, revision refresh, focus/restore, and superseded reads. No additional blocker identified in these source seams.

SDK/IPC review: Rust effective folder roots serialize as optional workspaceFolderRoots; generated types and Zod consumer include that exact field; add/remove adapter forwards it and menu merges it with old-server fallback. All four promoted IPC constants match existing string values and main/preload references agree. No changed wire names or dependency-direction regression found.

## Third checkpoint — workflow and preimage guard review

Independent workflow verification: after Hermit activation, `pnpm exec vitest run src/components/Layout/NavigationPanel.test.tsx src/utils/sessionActivity.test.ts` passed 2 files / 17 tests (20:36:07, 1.25s). Log: /tmp/gosling-evening-independent-review-workflow.log. No new blocker from final source pass of BaseChat, NavigationPanel, sessionActivity, ArtifactPane, OutputHistory restore-event consumer, SDK/Zod roots and IPC registry.

REVIEW-DAT-001 source mechanism is closed by refined patch: output_roots_with_warnings records non-NotFound failures, prepare combines these with scan warnings, and non-NotFound candidate canonicalization failures mark scan_complete false. Known missing-parent creation remains supported. Permission test restores the exact original permissions in a Drop guard before finishing and assertions; the new-parent regression protects the countercase. Runtime drill depends on Unix and a non-root user. Owner test results remain separate evidence until log is inspected.

### REVIEW-REL-002: CLI can choose stream EOF before simultaneous caller cancellation

Severity: Medium. Confidence: Likely. Evidence basis: simulation-reasoned.

CLI session/mod.rs awaits agent.reply before entering its unbiased tokio::select. A manually cancelled compact now returns an empty stream. Both `stream.next()` and `cancel_token_clone.cancelled()` may be ready; selecting the `None => break` arm bypasses the sole assignment of `Run cancelled by user`. Later JSON status is completed and stream JSON emits Complete when terminal_error remains absent. ACP has a final caller-token check and does not share this edge. Source confirms branch ordering permits the outcome; scheduler choice not independently executed. Escalated to parent/reliability owner. Repair should check original token after EOF/before success and preserve interrupted-message cleanup.

## Review ownership change and final core critique

Parent authorized this reviewer to implement REVIEW-REL-002 in CLI session/mod.rs after discovery; its separate stage plan is cli-repair.md. Therefore this report does not claim independent approval of that CLI implementation. Parent owns its independent review.

REVIEW-REL-001 source correction reviewed: reply_entry retains the original caller token, ensure_turn_not_revoked distinguishes a cancelled lease child from cancelled caller, and calls the guard at the post-command exit, entry auto-compaction cancellation exit and after reply_internal drains. These paths now deliver errors to existing ACP/CLI error consumers for internal revocation while preserving normal user cancellation. Source mechanism is closed; parent-coordinated Rust checks cover actual compilation/tests. No fresh-process ACP takeover drill claimed.

REVIEW-DAT-001 final source correction was further narrowed by the parent/dataflow owner to genuine IO errors, preserving existing policy exclusions; owner reports 28/28 output revision tests pass including permission recovery and read-only sibling capture. This review does not substitute that reported result for independently inspected raw log evidence.

Independent locale inspection parsed every old/new JSON: exactly six intended keys differ in all 16 catalogs, no keys removed. `git diff --check` passed at that checkpoint. No translated-language correctness claim.

## Final review disposition

REVIEW-DAT-001 closed: final source now distinguishes IO uncertainty from static policy exclusions. Independently inspected raw final-output-tests-v2.log: 28 passed, zero failed; final-core-tests.log ends in 10/10 compaction passes. Those runs are owner-executed evidence, not reviewer reruns. Known-new-parent and read-only sibling controls protect against overpatching.

REVIEW-REL-001 closed at source/targeted-test level: caller/child distinction and outer stream guard deliver lease-loss failures. Actual competing-process ACP takeover not executed. No further source blocker.

REVIEW-REL-002 closed: implemented under explicit parent delegation, reproduced before patch with actual CliSession, repaired and verified 2/2 targeted tests, then 20/20 session tests with a fresh process and isolated GOSLING_PATH_ROOT. Parent independently reviewed implementation. Exact stage, ambient configuration failure, isolated result and closure in cli-repair.md.

Final inspected source hashes are in final-source-sha256.txt. All reviewer logs copied beside this report. No unresolved repair blocker identified in reviewed changed workflows. Remaining limits: no installed Desktop run, no fresh real-provider compaction, no actual multi-process takeover drill, no security audit, and parent final Clippy/whole-Desktop results are outside this independent checkpoint. The scan was bounded to changed source seams and their immediate consumers, not every repository surface.

Final union check addendum: parent completed `cargo clippy -p gosling -p gosling-cli --lib --tests --locked -- -D warnings` with exit 0 (28.82s); reviewer inspected /tmp/gosling-evening-clippy.log completion. Parent also reports cargo fmt --check, git diff --check, full Desktop 1271 tests, TypeScript, lint, formatting and i18n checks green. No source edits followed this review's final-source hash snapshot. All three review subcases are resolved with the validation/runtime limits retained above.
