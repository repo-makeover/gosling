# System and surface audit repairs

Date: 2026-09-08
Target: `/Users/eric/Work/vscode/forked/gosling`, `main` at
`68702285568466382b069c443aa3043f6c735a66`.
Workflow: catalog `repair-defect-patchset`; supplied seven findings only, no security scan.
Existing `.gitignore`, `docs/INDEX.md`, and untracked source audit/session report are user work.
They are preserved; the audit will receive a dated disposition addendum.

## Plan and baseline

1. DAT-GSL-001 (data integrity, P1, medium) and DAT-GSL-002 (data integrity, P2,
   medium): Markdown body parsing in core and history preview, restore transaction,
   output revision integration and UI tests. Preserve exact body bytes, saved export bytes,
   attribution and successful restore behavior; reject incomplete footers. Baseline source
   confirms rigid LF matching and a committed baseline before replacement.
2. WFG-GSL-001 and REL-GSL-001 (UI/reliability, P2, low): verify existing dismissal,
   missing-file acknowledgement, persistence and regenerated versions through ArtifactPane,
   workbench and main IPC tests. Source trace shows requested paths echoed unchanged, version
   dismissal derived from the same inventory, and reactive filtering before displayedArtifacts.
   Both appear not-a-defect: ADR-0013 explicitly retains backend provenance, including missing
   files. Do not delete database records or add a redundant renderer inventory.
3. WFG-GSL-002 (configuration correctness, P3, medium): validate the resulting threshold/
   reduction pair before saving ACP preferences; cover partial and batch updates, defaults,
   zero reduction and atomic rejection. Preserve other preference behavior.
4. REL-GSL-002 (performance/reliability, P3, low): bounded incremental UTF-8 decoding in
   copy-artifact-contents, reject binary bytes before whole-file allocation, retain size/change
   checks and clipboard all-or-nothing semantics. Test chunk boundaries and early rejection.
5. REL-GSL-003 (performance, P3, low): use the already-canonical target for scope membership;
   preserve scratch exclusions, canonical root resolution and failure behavior.

Groups have no schema/API dependencies on one another. Core and Desktop footer semantics must
agree. Deletion and copy share artifact authorization, which stays unchanged. Source edits are
staged by group; independent reads/checks may run concurrently. No delegation is needed.

Authoritative declarations: AGENTS.md; accepted `docs/architecture.md`, ADR-0013 (inventory,
Trash, copy), ADR-0018 (history/restore); existing ACP preference definitions and context defaults.
Baseline: footer recognition drifts from managed-footer equality; restore can commit partial
history on failure; deletion is conformant. SQLite/filesystem crash atomicity remains the explicit
ADR-0018 limitation. No declaration amendment is planned. `.giles` YAML is advisory and predates
these surfaces. GEMINI.md is absent. Tests not yet run at intake.

Validation: targeted Rust output/ACP/scope regressions, focused Desktop Vitest, TypeScript,
scoped lint/format, cargo fmt and diff checks. These required targeted checks follow the later
AGENTS.md execution clause requiring validation for code changes; no full build/suite is planned.

## Progress

All five stages and both closing inspections complete. Five findings repaired; two closed as not-a-defect with source/test evidence.

## Stage evidence and decisions

- Stages 1–2: 18 output-revision integration tests passed. New cases cover edited CRLF/LF
  footers, footer-only changes without invented revisions, and an unwritable output directory
  during restore with unchanged file/history after reopening storage. Two parser unit tests
  cover body-byte preservation, earlier markers, non-Markdown files and incomplete/nonterminal
  footers. Desktop history preview/export regression tests passed. The initial Rust invocation
  completed 16 original tests while source work began; it is a smoke baseline, not an isolated
  proof of the original source revision. The new regression executions cover the patched source.
- WFG-GSL-001: not-a-defect. Existing ArtifactPane test already asserted row/count/preview removal;
  expanded it to both `trashed` and `missing`. Main IPC echoes the requested path, not its
  canonicalized authorization path, so the audit's hypothesized path mismatch is absent.
  Dismissal and filtering use the same `lastSeenAt` string, without a timestamp conversion.
- REL-GSL-001: not-a-defect. ADR-0013 explicitly retains backend provenance. Workbench tests
  verify dismissal survives remount and a new version reappears; missing results reach that
  same dismissal path and use informational feedback, not a false Trash-success toast.
- Stage 3: 4 ACP preference tests passed. Validation considers defaults, saved companion values,
  all batch updates (last value wins), and zero reduction. Review found that removing one
  preference could recreate an invalid pair; the same validation now checks reset requests
  before deletion. Combined reset remains allowed. No schema or generated client change.
- Stage 4: 24 main IPC tests passed. Large binary rejection takes one 64 KiB read, closes the
  handle and leaves clipboard untouched. Split UTF-8, incomplete UTF-8 at EOF, late NUL bytes,
  complete large text, empty files, authorization and size checks pass. Valid text still needs
  a full final string for Electron's clipboard API; this is not constant-memory clipboard output.
- Stage 5: 32 scope unit tests and 6 workspace-scratch integration tests passed. Canonical
  target is reused, canonical allowed-root checks remain, and empty-root failures/scratch
  exclusions/read-only roots/symlink behavior are preserved.

## Verification commands

Run from repository root with `source bin/activate-hermit` for Rust and from `ui/desktop`
with `source ../../bin/activate-hermit` for the pinned Desktop toolchain:

- `cargo test -p gosling --test output_revisions_test --locked`: 18 passed.
- `cargo test -p gosling --lib session::output_revisions --locked`: 2 passed.
- `cargo test -p gosling --test acp_custom_requests_test test_custom_preferences --locked`:
  4 passed, 16 unrelated tests filtered out.
- `cargo test -p gosling --lib permission::working_dir_scope_inspector --locked`:
  32 passed, unrelated tests filtered out.
- `cargo test -p gosling --test permission_audit_regressions workspace_scratch --locked`:
  6 passed, 17 unrelated tests filtered out.
- `pnpm exec vitest run src/components/artifacts/OutputHistory.test.tsx src/components/artifacts/ArtifactPane.test.tsx src/contexts/ArtifactWorkbenchContext.test.tsx src/main/artifactAccessIntegration.test.ts src/main/fileIpc.test.ts`:
  75 passed across 5 files. After the final ES2020-compatible array access change, the
  affected OutputHistory suite passed again (9 tests).
- `pnpm run typecheck`: passed with pinned Hermit pnpm. First attempt with host pnpm 10.6.4
  was rejected by the package's >=10.30.0 engine requirement. The first pinned run caught
  an ES2022 Array.at use; replaced it with ES2020-compatible indexing and reran successfully.
- Scoped ESLint (`--max-warnings 0`) and Prettier over the five changed TypeScript files:
  passed. `cargo fmt`, final `cargo fmt --check`, and `git diff --check` passed.
- Scoped Clippy initially caught the repository's `clippy::string_slice` lint; changed
  regex-boundary access to `split_at`, without suppressing the lint. Final command
  `cargo clippy -p gosling --lib --test output_revisions_test --test acp_custom_requests_test --locked -- -D warnings` passed.

Local command logs: `/tmp/gosling-audit-repair-*.log`. No full suite, packaged app, live OS
clipboard/Trash test, reinstall, commit, publication, security scan or other-platform run.
Tests mock native clipboard/Trash while exercising actual main IPC/filesystem authorization
and React state/render behavior. Unix write-failure regression ran as the normal macOS user.

## Closing review

Gate 8: scoped self-review traced read_snapshot → body hash → capture/restore → saved history,
ACP dispatch → restore errors, main IPC → deletion callback → persisted workbench projection,
copy IPC → clipboard → UI feedback, and permission candidates → canonical target/root checks.
No separate independent reviewer was used or claimed. No new schema, API, permission grant,
dependency, or generated-source change. Ordinary hosted-tool capture still has its separate
baseline checkpoint; unlike restore it records a tool write that already occurred, and was
not changed by this restore-specific finding.

Gate 9: separate completeness pass checked all seven original IDs against their reproductions
and tests, retained historical audit text, and prepared a dated disposition addendum. Same IDs
in older audit reports describe unrelated issues and were left unchanged. No stale in-code
TODO marker for these repairs was found.

Contract comparison: ADR-0018 body equality/restore preservation gaps are corrected;
ADR-0013 inventory/Trash/copy contracts remain intact. ACP defaults and individual ranges
are unchanged; incompatible resulting pairs now return invalid params. Scope decisions keep
existing root canonicalization and scratch boundaries. Drift delta: **no new drift**.
SQLite and filesystem still cannot commit atomically across a crash (ADR-0018); this patch
rolls back both DB revisions on ordinary restore failure, not that documented crash window.

Records: source audit and source session report receive dated addenda (historical observations
retained); this log is explicitly allowlisted under the repository's existing log policy.

Final record validation: `git diff --check` and the required AGENTS governance-marker search
passed. GEMINI.md remains absent. Reviewed the dated addenda and their local links; existing
user changes to the index and audit log allowlist were retained. Source/tests are ready for
review in the working tree; installation and publication were outside this task.
