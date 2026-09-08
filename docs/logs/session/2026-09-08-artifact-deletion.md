# Outputs and Research Library deletion

Date: 2026-09-08

## Implementation plan

Target: `/Users/eric/Work/vscode/forked/gosling`, clean `main` at `6f4e8e3a9`.
User request: single and batch deletion in Outputs and Library. The operator
explicitly chose moving the underlying files to Trash. The earlier no-application-
backup preference applies to installation. Catalog `plan-task-approach`, mode B,
supplies this compact plan; implementation is explicitly authorized by the request.
Standard involvement; only the destructive-semantics fork needed clarification.

Inspected AGENTS, README, docs index, architecture, ADR-0013/0016, Giles advisory
metadata, the preceding preview repair, ArtifactPane/workbench, file IPC/preload,
Research Library scanner and existing tests. GEMINI.md is absent. The Library is
a bounded filesystem scan; Outputs are backend-owned historical metadata plus
Desktop presentation state. The generic delete-file API permanently unlinks files
and must not be reused for this feature.

1. Add a typed, bounded Trash IPC operation beside the file handlers. Use the
   existing per-window artifact guard, require regular files, report each result,
   and never fall back to permanent deletion. Test actual handler wiring with
   temporary files and a mocked operating-system Trash call.
2. Add a shared file-list presentation with checkboxes, select-all/clear, row
   delete and batch delete. A scrollable confirmation lists the exact selected
   paths once; errors retain failed items and selection. The list's scope key
   resets selection on tab/session changes, and in-flight callbacks retain their
   original targets. Test cancellation, partial failure, selection and navigation.
3. Wire both existing pane lists to those controls. Successful deletes close
   matching previews. Library refreshes from disk. Outputs persist dismissal of
   the deleted artifact version in Desktop presentation state, preserving backend
   provenance and allowing later updated outputs at the same path to reappear.
   Test restart persistence, update/recreation and session isolation.
4. Update the relevant ADR/architecture behavior, run targeted tests, full Desktop
   tests, typecheck, scoped lint/format, cargo fmt check and diff checks. Package,
   sign and reinstall using the unchanged Rust sidecar, then inspect both lists
   and exercise Trash only on disposable test files created for this run.

Risks/acceptance: files move to OS Trash, never permanent unlink; directories and
unauthorized paths remain protected; failures cannot be counted as deleted;
selection cannot leak to another chat; deleted rows/tabs cannot return merely
because a session reloads. Existing preview authorization must continue to pass.
No unrelated refactors, Rust schema changes, user-file deletion during testing,
or changes to input attachments are included.

Rollback: revert this feature's scoped source/docs patch and repackage; original
files deleted by a user remain recoverable through OS Trash. Backend history is
unchanged. Independent reads/checks may run concurrently; dependent source edits,
validation and installation stay sequential. Checkpoint this log after verified
source work and installed-app checks. Checks above are planned, not yet run.

## Source implementation checkpoint

Implemented the shared `ArtifactFileList`, typed `trash-artifact-files` bridge,
existing-guard/regular-file checks in the actual file IPC registration, versioned
Output dismissals in the workbench and Library refresh in ArtifactPane. Batches
larger than 500 use bounded sequential requests under the original confirmation;
a later failed request does not erase earlier successes. Selection is scoped to
the visible Outputs session or Library list, and independent row opening remains
separate from selection.

Review caught and fixed two failure-state issues before packaging: an ENOENT
error returned by the OS Trash operation must remain a failure rather than being
reported as an already-missing source file; background Library refresh must not
unmount the list and discard partial-failure selection/errors. Removed paths now
also clear stale error text before any retry/recreation.

Initial source validation: 44 focused tests passed, and the additional pane
integration cases passed (21 tests in that file). The first full Desktop run
passed 1,183 tests but timed out in the new 501-row stress test. Its repeated
accessibility role searches were scanning the whole synthetic DOM; direct label
and text queries kept the same interaction assertions and all 8 file-list tests
then passed in 2.86 seconds without increasing timeouts. A full rerun is pending.

Locale extraction added 14 feature messages. The sync also detected an existing
source-hash/catalog mismatch from prior permission work: two obsolete
`toolApprovalButtons.alwaysAllowed*` keys remained in all translated catalogs,
while their replacements were already present in source English. Reviewed both
keys across every locale and synchronized the generated catalogs with the
documented `--accept-source-changes` option. No existing source message was
changed by this feature. Locale validation now passes for all 15 locales with
1,140 messages; new messages use the catalog's English fallback pending translation.

Scoped ESLint, Desktop typecheck, Prettier, Rust formatting and the 21 i18n
transaction/recovery tests passed. Source and UI self-review covered the actual
IPC path, per-file outcomes, folder/symlink/window denials, duplicate requests,
selection cancellation, session switches, persisted dismissal and new versions.
This is scoped self-review, not an independent repository audit. Installation
and real OS Trash checks remain pending.

Final source verification: **1,184/1,184 Desktop tests across 154 files passed**,
including 18 new cases; typecheck and scoped ESLint also passed. The large-batch
test still exceeded the default timeout under full-suite DOM contention, so the
batch orchestration was named as a local helper in the same new component file.
The test now verifies real 501-path IPC batching and partial results directly;
separate UI tests retain selection, confirmation and partial-failure assertions.
No test timeout was raised. The full final suite completed in 29.68 seconds.
Evidence: `/tmp/gosling-delete-final-{tests,types,lint}.log`,
`/tmp/gosling-delete-{format,rustfmt,i18n-check}.log`.

## Installation checkpoint

Packaged with `just copy-binary` and `pnpm --dir ui/desktop run package`; signed
ad hoc with the repository entitlements. Quit the idle installed application
normally and used `ditto` to replace it after verifying the old bundle contained
no extra files. No application backup was created. Installed deep/strict signing
verification passed. Packaged and installed `app.asar` SHA-256:
`adb9dbb2e36ffb2c0d8805ea39055f92cd360b54d5889ae66ec6fbbcc850699c`.
Unchanged packaged/installed sidecar SHA-256:
`b419c384f54625113108d9a2611bf123f12f85c3177a6be718c4ca59423f8f2c`.

Prepared three disposable Markdown files under the existing Research Library's
`Gosling deletion smoke test 20260908-0908` folder, named
`gosling-delete-smoke-20260908-0908-{single,batch-a,batch-b}.md`. They contain only
smoke-test text. No original user document was selected for deletion.

The installed app initially waited on macOS Keychain before creating its window.
Read-only process sampling (`/tmp/gosling-delete-startup-sample.txt`) shows its main
thread in `SecItemCopyMatching` / `SecKeychainItemCopyContent`; this is separate
from the new Trash IPC. The operator completed the system prompt and confirmed
it was done; Gosling then started successfully.

## Installed-app verification

Used the installed application's Library controls to move the single fixture to
Trash, then selected the two batch fixtures and moved them together. Each dialog
listed only the exact intended fixture paths. Success messages reported one and
two files moved to Trash respectively, and Library returned from 13 entries to
its original 10. All three source files were absent afterward; removed their
empty fixture directory with `rmdir`. No original user document was moved or
deleted during testing.

The operator reported emptying Trash before the final Finder inspection. Thus
the live evidence is the successful native Trash operation, UI results and
source-file disappearance; recovery from Trash was not independently tested.
No permanent deletion fallback exists in the implementation.

Outputs live checks passed: Select all selected all 28 entries, Clear selection
reset the selection and disabled Delete selected, and the row-delete dialog
listed the expected single report path. Canceled that dialog, preserving all
28 outputs. Actual Outputs deletion is covered by the automated pane/IPC tests;
the live physical Trash operations above used only Library fixtures. Opened the
original Muninn repair plan in Outputs afterward and verified its content
rendered directly without an access-grant prompt.

Source validation, packaging, installation and the scoped live checks are
complete. No application backup was created. Remaining limits: new locale
messages use English fallback, and recovery after the operator emptied Trash
could not be tested. This log records scoped validation, not a full repository
audit.
