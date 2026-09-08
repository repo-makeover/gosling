# Permission readability follow-up — 2026-09-08

## Intake and previous-pass review

Operator request: double-check the previous pass, then polish five more files
under the same file-local naming/commenting and behavior-preservation constraints.
Intake: clean `main` at `9dc07049e`; the previous six-file pass is committed there.

Catalog search again selected `governance-code-polish`. Reuse its already-loaded
naming, commenting, verification and engagement contracts, plus the applicable
evaluation-preservation checklist from `repair-language-syntactic-sugar`. This is
ordinary Rust/TypeScript readability work, not new language syntax. Execute mode,
low involvement, implementing-agent review; no delegated or independent review.

AGENTS was reread first. Previously read README, index, architecture, advisory
Giles metadata and package conventions are unchanged from the prior baseline;
the relevant architecture/index entries and latest session checkpoint were
refreshed. Rustfmt, Prettier, existing error conventions and source-header policy
apply. Public APIs, protocol/schema names, prompts, runtime strings, imports,
module boundaries, test assertions and dependencies must remain unchanged.

All previous source diffs were reviewed, including permission locking/failure
contracts, verdict aggregation, parser offsets/limits, egress precedence,
generation identity and display history. No regression requiring a patch was
found. The previous six files remain unchanged in this pass. Fresh baseline
checks reproduced 1,872 core passes (3 opt-in benchmarks ignored), 17 audit and
3 inspection integration passes, and 86 Desktop passes in seven files. Clippy,
typecheck, Rustfmt and Prettier also passed. Logs use
`/tmp/gosling-polish2-before-{core,integration,ui,clippy,typecheck,fmt,prettier}.log`.

## Five source units

| Unit | Source | Planned improvement |
| --- | --- | --- |
| N1 | `crates/gosling/src/permission/permission_inspector.rs` | Name baseline/other-inspector results and judge candidates; explain precedence and argument-specific classification |
| N2 | `crates/gosling/src/permission/permission_judge.rs` | Name the judge response tool and locals; explain fallback/response extraction without changing prompts |
| N3 | `crates/gosling/src/permission/tool_class.rs` | Explain name-based classification limits and suffix matching; replace historical narration with current policy |
| N4 | `ui/desktop/src/acp/adapter/permissions.ts` | Name metadata and replacement message; explain offered domain scope and prompt refresh |
| N5 | `ui/desktop/src/acp/adapter/elicitations.ts` | Name content updates and emitted changes; explain duplicate suppression and preserved message positions |

These files have existing relevant core/adapter/store tests and fit the current
permission/input workflow. The unwired legacy `ToolPermissionStore` was inspected
but not selected; its existing limitations and historical findings are outside
this readability pass. No source moves, cross-file extraction or application
build/install is planned. Every private rename updates only its owning file.

## Applied changes and rename manifest

All five units are patched and individually checked. Each executable rename is
private or local, and all of its references remain in its original file. There
are no file/directory renames, deletions, public-symbol changes, module moves,
case-only renames or compatibility aliases.

| Unit | Local changes | Codes |
| --- | --- | --- |
| N1 | `permission_check_result` → `decisions`; `permission_results` → `baseline_results`; `non_permission_results` → `other_inspector_results`; `llm_detect_candidates` → `read_only_candidates`; `detected` → `judge_read_only_tool_names`; `is_readonly` → `can_auto_approve`; `tc` → `tool_call`. Comments distinguish name-based policy from a model verdict and explain precedence/cache behavior. | POL-001/004/005 |
| N2 | `create_read_only_tool` → `create_read_only_judge_tool`; `extract_read_only_tools` → `extract_read_only_tool_names`; `req` → `request`; `args` → `arguments`; completion locals `tool`, `check_messages`, `res` → `judge_tool`, `judge_messages`, `judge_response`. Existing response-tool string named `READ_ONLY_JUDGE_TOOL_NAME`; docs explain session/global fallback and unusable verdicts. | POL-001/004/005/010 |
| N3 | Private helper parameters `name`, `names`, `suffixes` → `tool_name`, `bare_names`, `extension_suffixes`. Module/API comments explain recognized-name matching and its limits without changing the tables. | POL-001/004/005 |
| N4 | `existingIndex` → `existingMessageIndex`; `permissionPrompt` → `firstPermissionPromptText`; `meta`, `gosling`, `permission` → `toolMetadata`, `goslingMetadata`, `permissionMetadata`; message local → `permissionMessage`; existing choice predicate named `offersDomainApproval`. Comments explain explicit domain choices and prompt replacement. | POL-001/005 |
| N5 | `hasExistingElicitation` → `hasElicitationMessage`; `statusData` → `statusFlags`; `changes` → `messageChanges`; `index` → `messageIndex`; `messageChanged` → `hasMatchingElicitation`; mapped content → `updatedContent` / `contentBlock`. Comments explain preserving an existing form and reporting the replacement's original position. | POL-001/005 |

Source/string sweeps across `crates`, `ui`, `docs` and `.github` found no old
private helper references before this manifest was added. The old names above
are intentionally retained only as rename evidence. No external caller changes
were necessary. Existing imports and API signatures remain unchanged.

## Preservation and diff review

Every source hunk was reviewed against `9dc07049e` and maps to N1–N5 above;
format-only wrapping maps to POL-015. No test assertion, fixture or test name was
edited. Additional direct comparisons establish:

- The prior six source files are byte-identical to the committed baseline.
- All three inline Rust test modules are byte-identical to the baseline.
- Both judge prompt bodies and all nine classification name/suffix tables are
  byte-identical to the baseline.
- The judge tool string keeps its exact value and still has the same allocation
  at tool construction and comparison at response extraction.
- The offered-domain predicate is evaluated once in the same place; the metadata
  check retains its original short circuit. No operation moves across an await.
- Elicitation mapping preserves order, cloning/spread order, status property
  names, unmatched object identity and explicit message indices. Naming the
  match flag does not introduce a comparison of old/new status values.

The current comments describe recognized-name heuristics rather than claiming a
complete classification of every tool. No model prompts, user-facing strings,
error messages, logging values, serialized keys, cache behavior or permission
precedence were changed. This is source/diff review plus test evidence, not a
formal AST-equivalence proof or an independent audit.

## POL coverage

| Code | Disposition | Evidence / action |
| --- | --- | --- |
| POL-001 | Fixed | N1–N5 local rename manifest and reference sweeps |
| POL-002 | Clean in scope | Existing Rust/TypeScript path naming conventions retained; no file moves |
| POL-003 | N/A — no header requirement | Existing source-header policy requires no boilerplate; N3's module docs are behavior documentation, not a license/provenance change |
| POL-004 | Improved | N1/N2/N3 nontrivial contracts clarified; trivial helper narration removed; no accessor boilerplate |
| POL-005 | Fixed | Historical/overbroad policy comments replaced with current precedence, classification and state invariants |
| POL-006 | Clean in scope | Five-file source/comment review found no commented-out executable blocks requiring removal |
| POL-007 | Clean in scope | Five-file scan found no `dbg!`, `todo!`, `unimplemented!`, `debugger;` or `console.log` remnants; no debug instrumentation added |
| POL-008 | No cleanup warranted | Imports, initializers and executable branches retained; the unwired legacy store was excluded rather than deleted |
| POL-009 | Clean in scope | No TODO/FIXME/HACK/XXX/TEMP/temporary/workaround/cleanup markers in the five-file scan; no ledger entries removed |
| POL-010 | Fixed | N2 gives the existing structured-response tool ID one private constant; its wire value is unchanged |
| POL-011 | Preserved | Logging strings, levels and values unchanged; no repository-wide secrets/logging audit claimed |
| POL-012 | Preserved | Existing error/empty-result paths and cache-save handling unchanged; their comments now describe actual behavior |
| POL-013 | Preserved | Existing scenario tests retained; three Rust test modules are byte-identical; no new implementation-mirroring tests |
| POL-014 | Verified | Private helper references updated only in their source file; source/docs/string sweeps found no stale references outside this manifest |
| POL-015 | Verified for source | Rustfmt and Prettier checks pass; final diff/document structural checks recorded at handoff |
| POL-016 | No file-size finding | Files are 514/266/173/82/97 lines, below the skill's approximately 800-line default; no monolith restructuring; any future splitting routes to `repair-source-modularization` under separate scope |
| POL-017 | Preserved; wider architecture not verified | No module, import or authority-boundary changes; no architecture-clean claim beyond this patch |
| POL-018 | No consolidation warranted | Local helpers serve distinct purposes; no byte-identical helper consolidation identified; cross-module deduplication excluded |

## Validation

Per-file checks, each after `source bin/activate-hermit`:

| Unit | Focused command | Result |
| --- | --- | --- |
| N1 | `cargo test -p gosling --lib permission::permission_inspector`; `cargo test -p gosling --test tool_inspection_manager_tests` | 19 + 3 passed |
| N2 | `cargo test -p gosling --lib permission::permission_judge` | 2 passed |
| N3 | `cargo test -p gosling --lib permission::tool_class` | 6 passed |
| N4 | Vitest sessionNotificationAdapter + PermissionAuditRegression; Desktop typecheck | 20 passed; typecheck passed |
| N5 | Vitest chatSessionStore; Desktop typecheck | 26 passed; typecheck passed |

Before/after commands use identical flags:

```sh
cargo fmt --all -- --check
env -u MUNINN_MCP_BEARER_TOKEN cargo test -p gosling --lib
cargo test -p gosling --test permission_audit_regressions --test tool_inspection_manager_tests
cargo clippy -p gosling --all-targets -- -D warnings
pnpm --dir ui/desktop exec vitest run src/components/PermissionAuditRegression.test.tsx src/components/ToolApprovalButtons.test.tsx src/acp/__tests__/permissionRequests.test.ts src/acp/__tests__/sessionNotificationAdapter.test.ts src/acp/__tests__/chatSessionStore.test.ts src/acp/__tests__/chatSessionLifecycle.test.ts src/acp/__tests__/chatSessionController.test.ts
pnpm --dir ui/desktop typecheck
pnpm --dir ui/desktop exec prettier --check src/acp/adapter/permissions.ts src/acp/adapter/elicitations.ts src/acp/permissionRequests.ts src/components/ToolApprovalButtons.tsx
```

The environment variable is removed only from the core test subprocess, as in
the previous session, to isolate the existing environment-merge test. No global
configuration changes. Baseline core/integration per-test outcomes also match
the prior session's retained logs, including the same ignored benchmarks.

| Baseline check | Before | After | Delta |
| --- | --- | --- | --- |
| Core library | 1,872 passed; 3 ignored | 1,872 passed; 3 ignored | None |
| Permission audit regressions | 17 passed | 17 passed | None |
| Inspection integration | 3 passed | 3 passed | None |
| Seven Desktop test files | 86 passed | 86 passed | None |
| Core all-target Clippy, warnings denied | Passed | Passed | None |
| Desktop typecheck | Passed | Passed | None |
| Rustfmt / Prettier | Passed | Passed | None |

Core and integration named test outcomes match exactly before/after; Desktop's
reported file/test totals also match. No new failure or unexpectedly passing
baseline test needs investigation. Logs use `/tmp/gosling-polish2-after-*.log`
and `/tmp/gosling-polish2-n*.log`; commands/counts are retained here because
temporary logs are not durable repository artifacts.

## Handoff

Completed: previous-pass recheck and exactly five additional source files
polished. The final scope consists of those files, this report and a dated
checkpoint in the existing permission session log. Diff and record checks
verify the five-file source inventory, all 18 POL rows and the session-report
link. No existing user work was overwritten.

Validation covers 1,978 passing tests and the same 3 ignored opt-in benchmarks.
It does not establish whole-repository behavior or replace an independent
review. No new behavior defect was identified in this patch review. Larger
refactors and cross-file consolidation remain outside scope. No application
build/install, backup, restart or live GUI/service mutation occurred; test
compilation was part of source validation.
