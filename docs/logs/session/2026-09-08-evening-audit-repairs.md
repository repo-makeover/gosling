# Independent evening audit and repairs

Date: 2026-09-08
Target: `/Users/eric/Work/vscode/forked/gosling`, main at
`a48108750945e42509164980e49ad452c3e12e79`, initially clean.
Scope: today's 19 commits, `cb1aac7ed8df0e9661deb70957934c96550a0a1c..a48108750`,
plus adjacent consumers. Security audit excluded by the operator.

## Execution and authority

Four independent catalog audit lanes cover data flow, workflow, reliability,
and architecture/drift. Reports and checkpoints live under `generated/today-audit/`.
The parent reconciles supported findings into the catalog `repair-defect-patchset`
workflow. Local patches and necessary regression validation are authorized by
the request and AGENTS.md's required validation clause. No merge or publication.
Independent reads and disjoint repairs run concurrently; shared edits and Rust
builds are coordinated. Each lane records its stage plan before editing.

Canonical declarations: AGENTS.md, docs/architecture.md, ADR-0013/0018, the
context-compaction plan, and existing typed ACP/config contracts. GEMINI.md is
absent. Giles YAML is advisory and is not promoted. Prior repair records and
intentionally deferred architecture work are retained.

## Baseline and stages

- Desktop baseline: nine changed Vitest suites, 121 tests: 114 passed, seven
  failed. All failures are obsolete Delete labels in ArtifactFileList.test.tsx
  after the deliberate Trash terminology change. The test bodies still cover
  selection, cancellation, partial failure, missing files and duplicate submission.
- Stage UI contract regression: update those stale expected labels and failure
  wording only. ADR-0013's Trash behavior and production code remain unchanged.
  Baseline defect: seven failing tests; post-repair check: the same suite.
  The broader run also found OutputFileExtensionsSection still expecting CSV
  to be appended even though today's defaults already include it. The same
  test-contract stage uses XML as the non-default extension, preserving its
  normalization/deduplication assertions; production defaults stay intact.
- Stage locale drift: `i18n:check` fails because five changed message defaults
  and one new message were not extracted. All 15 other locales retain the old
  English fallbacks for the five changed keys. Refresh these exact fallbacks,
  regenerate English and source hashes with the existing reviewed sync workflow,
  compile catalogs and run the full i18n check. No translated text is replaced.
- Revision custody: independently confirmed saved-revision retrieval depends on
  the current file remaining snapshot-readable, and skipped pre-images can be
  misclassified as newly created outputs. The dataflow lane owns source/tests.
- Compaction lifecycle: the reliability lane is validating terminal outcome
  propagation and manual compaction cancellation, then owns bounded repairs.
- Workflow and architecture lanes continue independently; supported findings
  will receive explicit dispositions before patching.

## Intermediate checkpoint (superseded by final results below)

Desktop source is verified: all 160 suites / 1,271 tests passed. Generated SDK
source and compiled declarations have been refreshed; TypeScript passed. The
first full run found the stale CSV test and raced the in-progress image-reply
test addition; the final run used the finished source and passed. Scoped lint
caught a type-only Window reference; it now uses the existing window.electron
type. Formatting was corrected in the parent-owned artifact-list test.

The first Rust union passed 1,883 library tests (3 ignored), 10 compaction tests,
and 25 output-revision tests. Independent review extended the observation guard
to actual filesystem canonicalization errors; the intermediate broad error
guard caused a read-only exclusion regression and was narrowed before closure.
All 28 final output-revision tests now pass, including permission recovery,
missing-parent creation, and read-only sibling preservation.

Independent review also found lease-child cancellation and CLI cancellation/EOF
outcome gaps within the reliability stage. Those fixes and regression checks are
in progress. Final combined Clippy, closing review, completeness inspection and
record closure remain pending. Native Desktop and live providers are unverified.

## Final staged results

All ten findings in the [campaign report](../../cloud/2026-09-08-evening-audit-repair.md)
are repaired. The source inventory has two data-integrity, two reliability, four
workflow/test/catalog, and two architecture findings (five Medium, five Low).

| Stage / supplied IDs | Repair and unchanged adjacent behavior |
|---|---|
| Revision custody: DAT-TODAY-001/002 | Optional live snapshot failure no longer blocks immutable bytes; restore still requires a valid current hash. Unknown before-observations cannot gain authorship or a footer. Known creation, unchanged content, eligible siblings, policy exclusions and existing restore safeguards remain covered. |
| Run outcomes: REL-TODAY-001/002 | Failure metadata reaches ACP; manual cancellation prevents replacement; child-only lease cancellation reports failure; CLI cancellation and EOF share existing cleanup exactly once. Successful compaction, credits payloads, partial resume, normal CLI completion and explicit tool/elicitation behavior are preserved. |
| UI identity: WFG-TODAY-001/002 | Unread derives from real visible reply identity and foreground acknowledgement; image replies remain supported. Canonical title keys and revision/focus/restore invalidation prevent aliases/stale titles. Manual command confirmation remains readable new text; no ACP visibility-contract expansion. |
| UI drift: WFG-TODAY-003/004 | Eight stale test assertions/assumptions corrected without reverting today's Trash labels/default extensions. Five old English fallbacks and one missing message refreshed across catalogs; translated strings untouched. |
| Contracts: ARC-TODAY-001/002 | Four artifact IPC constants reused by main/preload. Directory response adds optional effective roots, generated canonically and consumed with old-server fallback. No wire channel rename or new permission decision. |

Ordering and interaction: revision metadata semantics precede UI export checks;
directory DTO generation precedes SDK compilation/typechecking; IPC constants
retain wire values. Repairs to disjoint paths ran in parallel. Shared Cargo
generation/tests/Clippy were allowed to complete without cancellation/restart.
Review refinements stayed within the original finding mechanisms.

## Final validation

Commands use `source bin/activate-hermit` from repo root, with `pnpm --dir` where
shown. Raw local logs remain under `generated/today-audit/` and
`/tmp/gosling-evening-*.log`; generated Markdown records preserve their results.

- `cargo test -p gosling --lib --test compaction`: final 1,884 library tests passed,
  3 ignored; 10 compaction tests passed. The earlier union also selected
  `--test output_revisions_test`.
- `cargo test -p gosling --test output_revisions_test --locked`: final 28 passed.
  New regressions failed before repair for live-file limit, budget-set drift,
  incomplete scan and permission recovery.
- `cargo test -p gosling-cli --lib reply_`: 2 passed, exercising actual cancelled
  JSON/stream-JSON replies 16 times each and successful `/clear` control.
- CLI `session::tests`: ambient run 18 passed / 2 failed due stored thinking
  effort; the same compiled binary in a fresh process with temporary
  `GOSLING_PATH_ROOT` passed all 20. Real user configuration untouched. Exact
  invocation and failure evidence: `generated/today-audit/review/cli-repair.md`.
- `pnpm --dir ui/desktop exec vitest run`: all 160 suites / 1,271 tests passed.
- `pnpm --dir ui/desktop run typecheck`: passed after fixing the test's type
  reference and rebuilding generated SDK declarations.
- Scoped ESLint with `--max-warnings 0` and Prettier `--check` over every changed
  Desktop TypeScript file, including new sessionActivity source/tests: passed.
- `just generate-acp-types` and `pnpm --dir ui/sdk build`: passed. Source DTO,
  schema, TypeScript and Zod all include optional `workspaceFolderRoots` and the
  safe-snapshot meaning of `currentHash`.
- Reviewed i18n source extraction, `i18n:sync -- --accept-source-changes`,
  `i18n:compile`, and `i18n:check`: passed; 21 sync tests and all 15 non-English
  catalog validations passed. Exactly six intended keys changed; none removed.
- `cargo clippy -p gosling -p gosling-cli --lib --tests --locked -- -D warnings`:
  passed, exit 0.
- `cargo fmt` and final `cargo fmt --check`, `git diff --check`, new report/session
  link validation and AGENTS governance-marker check: passed. GEMINI.md absent.

## Closing inspections and contract comparison

Gate 8: an independent reviewer traced core/ACP/CLI terminal handling, caller and
lease-child tokens, revision read/capture/restore, UI event producers/consumers,
title identity and refresh, optional folder metadata and main/preload parity.
Three completeness subcases were repaired: uncertain canonicalization,
lease-only cancellation and CLI cancelled EOF. The reviewer implemented only
the final CLI fix; the parent independently reviewed that diff, adjacent
error/tool/elicitation paths and machine-output consumers. Independent UI
replays added 78 passing tests, distinct from the full-suite parent execution.

Gate 9: the parent separately matched every original finding to its reproduced
trigger, patch and final evidence, reviewed the union diff and contract outputs,
checked marker/record freshness, and confirmed no in-scope blocker remained.
Representative regressions plus source traces are not exhaustive behavioral
equivalence over every input or a production runtime claim.

Authoritative-source map and drift delta:

- ADR-0018 governs revision custody/attribution/restore; pre-existing retrieval
  and observation defects corrected, safe-snapshot description refreshed.
- ADR-0013 governs inventory identity and Trash; metadata retention and native
  action behavior unchanged. The workspaces guide documents saved-byte access.
- Compaction fail-safe plan, terminal-error metadata and run-state contracts
  govern failure/cancellation; misleading successful outcomes corrected.
- ADR-0017 and session DTOs govern directory policy; authoritative roots now
  propagate immediately without changing grants.
- `.architecture/invariants.yaml` ARC-003 and component ownership govern IPC;
  new channel declarations now conform. Retired invariants remain retired.
- Existing UI reply/status and title contracts govern presentation fixes.

Result: no new architecture/contract drift. Additive response metadata retains
older-server compatibility. No dependency, schema migration, policy decision,
provider selection, or broad architectural rewrite was introduced.

## Records and final status

Original audit snapshots remain intact with disposition addenda. The campaign
report, docs/TODO.md, docs/INDEX.md, this session record, ADR-0018, and the
workspaces guide are current. WFG-TODAY-003/004 source closure is recorded in the
workflow report's parent addendum. Previously deferred backlog remains unchanged;
the active-only TODO mirror needs no new entry because these findings are closed.

Final status: `completed_with_partial_verification`. All requested local repairs
and source checks are complete. No full workspace Rust suite, real provider,
native Electron/OS Trash/clipboard, packaged installation, cross-platform run,
or production crash/takeover drill was performed. No commit, merge, installation,
publication, security scan or external service action was performed.
