# Recent change-set audit repairs

Date: 2026-09-08
Baseline: main at d9482f9d186c0449c3f0b394209d622507e7815c, clean tree.
Source: docs/cloud/2026-09-08-recent-work-independent-audit.md (20 findings).
Workflow: repair-defect-patchset. Scope is the 20 enumerated findings, excluding the
report's explicitly deferred risks. Local repairs and required targeted validation
are authorized; no publication or merge. Involvement L1 inferred from “Patch all”.

## Plan and baseline

1. Compaction: REL-GOS-001 (P0/high), FSR-GOS-003/004 (P2/medium),
   WFG-GOS-001 (P1/medium), INV-GOS-002 (P2/medium). reply_entry/reply_stream,
   context_mgmt, ACP config, CLI config diagnostics, AlertBox and regression tests.
   Baseline: partial resume is persisted as full replacement; cancellation is ignored
   during summarization; metrics errors claim intact history; upsert bypasses validation.
   Preserve ordinary full-history compaction, zero reduction, provider-owned context.
2. Revision custody: FSR-GOS-001 (P1/high), CAS-GOS-001 (P2/medium),
   IAPI-GOS-001 and INV-GOS-001 (P2/medium). output_revisions/storage, ACP dispatch,
   DTO/schema descriptions and output regression tests. Preserve authorization, hashes,
   exact-byte export and attribution; contain per-file bounds and preserve pre-restore bytes.
3. Permission truth: WFG-GOS-004/005 and STT-GOS-001 (P2/medium).
   reply_context, SessionInfoSummary, WorkingDirectoriesMenu and related tests.
   Preserve policy decisions; correct reasons and claims about directory access.
4. Outputs: DAT-GOS-001, WFG-GOS-002/003 (P2/medium); STT-GOS-002,
   WFG-GOS-006/007, IOP-GOS-001 (P3/low). ArtifactFileList/Pane, OutputHistory,
   workbench, settings, workspace activity chrome/tests. Preserve Trash (never unlink),
   inventory provenance and export access; correct defaults, labels and comparison errors.
5. Documentation: ARC-GOS-001 (P3/low), derived docs, audit disposition addendum.
   Review the union of patches and separately check all original IDs for completeness.

The core owns persistence/config/policy; Desktop presents their contracts. Stage 2's
history behavior underpins stage 4. No unrelated refactoring or dependency changes.
Baseline declarations: AGENTS.md, docs/architecture.md, accepted ADR-0013/0018,
context-compaction-failsafe-plan, ACP definitions and current regression tests.
Known drift is exactly the supplied findings. .giles metadata is advisory; GEMINI.md
is absent. Independent reads/checks can overlap; edits to shared surfaces stay serial.
Validation uses pinned Hermit tools: targeted Rust/ACP tests, Desktop Vitest/typecheck,
cargo fmt, scoped lint and git diff --check. Full suite and native Desktop not assumed.

## Progress

All five stages and both closing inspections are complete. All 20 supplied findings
are repaired and closed in the source audit's dated addendum. No deferred item was reopened.
Baseline: compaction 6/6, revisions 18/18, original Desktop subset 51/51.
Intermediate compile errors in observation-loop editing were corrected. Initial UI
failures were obsolete label/filter assertions; a final empty-history assertion was
made specific to the dialog copy to avoid racing the same text in the row. Final
results below supersede the intermediate runs. No failed check remains unexplained.

Restore ordering is the audit-authorized amendment: stage/sync, commit snapshots,
then CAS replacement. Failure after commit explicitly says snapshots were saved but
replacement failed. Crash before publication leaves history ahead of file, preserving
all bytes. ADR-0018 records the revised boundary. Compacted-resume context folds stay
in memory, preserving durable history; ordinary full-history compaction still persists.

## Finding dispositions and evidence

All 20 findings are closed with the final command results below.

| Finding | Repair and regression surface |
|---|---|
| REL-GOS-001 | Shared compact helper never replaces durable messages for partial resumes; 80 original messages survive; ordinary full-history automatic/manual/recovery compaction tests retained. |
| FSR-GOS-001 | Stage/sync replacement, commit baseline plus restore snapshot, then publish with a second hash check. Deferred-foreign-key commit failure preserves exact live bytes and original history; unwritable-directory failure still rolls back. Capture commits before footer publication. |
| WFG-GOS-001 | Desktop config/upsert and remove validate the resulting compaction pair. ACP rejects invalid writes/resets and accepts explicit disable; AlertBox keeps rejected edits open and displays the server error. |
| INV-GOS-002 | Shared Rust finite [0,1) validator; zero disables threshold / requests full reduction. ACP and CLI use it; runtime rejects invalid values; Desktop edits 0–99%; docs agree. |
| FSR-GOS-003 | Shared helper selects cancellation against summarization and checks again before persistence. Provider-triggered cancellation regression preserves history without HistoryReplaced. |
| FSR-GOS-004 | Typed post-save metrics failure produces saved-compaction copy; before-save failure retains intact-history copy. Existing public generic failure formatter preserved. |
| CAS-GOS-001 | Scan/byte/per-file storage failures aggregate bounded warnings after eligible siblings record; explicit targets have budget priority. Oversized sibling and injected per-file DB failure regressions pass. |
| IAPI-GOS-001 | Revision responses have stable validation/not-found/conflict/limit/storage codes, appropriate JSON-RPC classes, and actionable main messages. Mapping contract tests cover each class. |
| INV-GOS-001 | Rust DTO, regenerated schema and SDK describe body hash versus complete live-file hash. Footered Markdown rejects contentHash and restores with currentHash. |
| STT-GOS-001 | Transcript policy denials use inspector reason; stored-permission denials say current permissions. User-prompt rejection remains separate. Denial regression checks forbids-mutation reason and absence of user-declined claim. |
| WFG-GOS-004 | Workspace summary explains enforced folder policy with restriction off; non-workspace unrestricted copy remains. |
| WFG-GOS-005 | ACP session metadata carries pinned effective folder roots/access; Desktop conversion preserves it and directory rows label read-only/read-write or unknown workspace policy. No permission decision/grant changed. |
| DAT-GOS-001 | Removed output inventory remains in a distinct history/export section; tabs and live-output count still dismiss. History and Trash copy disclose path-based retention across chats. Existing missing-file export regression retained. |
| WFG-GOS-002 | Row/batch actions and failures consistently say Trash; confirmation discloses retained copies/revisions. Native shell.trashItem boundary unchanged. |
| WFG-GOS-003 | Default display list includes all revision-supported document types; saved custom lists preserved. Extension-hidden count explains omitted inventory. |
| STT-GOS-002 | Empty latest-history row says No saved revisions; Unknown remains for missing identity. |
| WFG-GOS-006 | Repository filtering uses authorized repository classification, not source-like extensions; source-like outputs outside repositories remain listed. Fail-open and stale-result tests retained. |
| IOP-GOS-001 | Selected and previous revisions load independently; comparison failure preserves selected content, Export and Restore. |
| WFG-GOS-007 | Blue chat icon indicates unread activity; internal variables/test descriptions no longer call it workspace readiness. Existing unread/stream/error filtering retained. |
| ARC-GOS-001 | architecture.md correctly places history/get/restore in ACP and export in Electron saveArtifact. |

## Closing inspections

Gate 8 (self-review): traced partial load → all three automatic compaction callers → shared
persist guard; cancellation and metrics-failure branches; config preference/upsert/reset →
shared validator → CLI/runtime/UI; prepare/finish capture → transaction/file publication →
ACP error/data → Desktop history; pinned policy metadata → session conversion → labels;
Trash → dismissed-version state → live list plus saved-history export; repository and extension
filter intersections; previous-revision errors and saved-content actions. Ordinary provider,
manual compaction, attribution, exact-byte export, missing-file refusal, stale-hash rejection,
read-only/symlink checks, and successful tool results remain covered by existing tests.

Review corrections: restricted the generic upsert validation change to the two compaction
keys (other generic config behavior stays intact); kept the public generic error formatter
signature; counted failed directory entries within the scan bound; retained actionable text
in ACP's main error message; normalized Windows separators for folder labels. No new
permission grants, schema migration, dependency change, generated OpenAPI Desktop client,
revision deletion or permanent unlink was introduced. Final Clippy found two needless borrows;
removed them rather than suppressing the checks.

Gate 9 (separate completeness pass): matched every original table ID to the repaired trigger
and evidence above, inspected the combined diff and generated hash descriptions, checked
remaining TODO/FIXME/HACK/XXX markers in the touched custody/compaction/history surfaces.
No fixed-defect marker remained. New broad discovery/security work was not performed.

Architecture comparison: AGENTS core/client boundaries and ADR-0013 inventory authorization
remain conformant. The specific ADR-0018 commit-before-publication amendment is authorized by
FSR-GOS-001 and reflected in implementation, regression tests, limits and user guidance.
The ACP hash descriptions and error taxonomy are now explicit; metadata access fields are
additive. Drift delta: intentional authorized amendment complete, with no unexplained new drift.

Validation limits: native OS Trash, packaged Desktop, process-kill/power-loss drills and other
platforms were not executed. The commit-failure test uses actual SQLite deferred constraints;
UI/native bridges are mocked. SQLite and filesystem still do not share a transaction: a crash
between snapshot commit and replacement may leave saved restore bytes ahead of the live file,
with recoverable original bytes. Full workspace Rust and full Desktop suites were not run.
The audit's explicitly deferred risks remain outside this 20-finding repair scope.

## Final validation

Pinned Hermit environment, repository root unless noted. Source/test patch fingerprint:
`git diff -- crates ui | shasum -a 256` =
`de418dce1412458ded7373cc218673ce4950a19795dc314f9ff3c34e6390f276`.

- `cargo test -p gosling --lib --test compaction --test output_revisions_test --test acp_custom_requests_test --locked`:
  1,932 passed (1,882 library, 20 ACP, 8 compaction, 22 revision tests); 3 library tests ignored.
  Evidence: `/tmp/gosling-recent-rust-verified.log`.
- `cargo clippy -p gosling -p gosling-cli --lib --tests --locked -- -D warnings`: passed.
  Evidence: `/tmp/gosling-recent-clippy-verified.log`.
- `cargo fmt` and `cargo fmt --check`: passed.
- Desktop `pnpm exec vitest run` covering ACP sessions, OutputHistory, ArtifactPane,
  ArtifactWorkbenchContext, WorkspaceSidebarSection, NavigationPanel, WorkingDirectoriesMenu,
  SessionInfoSummary, settings, and AlertBox: 113 passed across 10 files.
  Evidence: `/tmp/gosling-recent-ui-final.log`.
- Desktop `pnpm run typecheck`, scoped ESLint with `--max-warnings 0`, and scoped
  Prettier check: passed. Evidence: `/tmp/gosling-recent-typecheck-final.log`,
  `/tmp/gosling-recent-eslint.log`, `/tmp/gosling-recent-prettier-final.log`.
- `just generate-acp-types`: passed; generated diff contains the three hash descriptions.
  JSON schema inspection verified body/full-file/expected-hash semantics.
- `git diff --check`, AGENTS governance-marker search, and repair-addendum link check: passed.
  GEMINI.md is absent. No local doc-lint command was found in the checked package/justfile surfaces.

Records refreshed: source audit (dated closure addendum), ADR-0018, architecture,
README, Outputs/workspace guide, compaction/configuration/environment guides,
v1.2.3 release notes, and this session record (allowlisted in .gitignore).
No external tracker, commit, merge, publication, installation, or fleet-compliance claim.

Final status: `completed_with_partial_verification` — all supplied findings repaired;
remaining verification limits are the native/process-kill/platform/full-workspace checks
explicitly listed above, not unfinished patches.
