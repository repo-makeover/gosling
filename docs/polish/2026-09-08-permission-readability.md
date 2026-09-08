# Permission code readability — 2026-09-08

## Intake and scope

Target: `/Users/eric/Work/vscode/forked/gosling`, clean `main` at `da493ef4d`.
The operator requests human-readable code using semantic sugar and commenting
guidance, with every refactor confined to its own source file. This pass starts
with the recently repaired permission workflow. Scope clarification was offered;
the initial bounded scope is used while no different preference is supplied.

Execute mode, low involvement, authorized local readability changes. Use catalog
`governance-code-polish`, its naming/commenting/verification contracts, and the
semantic-preservation hazard checklist from `repair-language-syntactic-sugar`.
The latter implements language syntax; this task instead applies established
Rust/TypeScript forms and does not add a grammar, macro or language feature.
No separately named semantic-sugar or commenting skill was found in the catalog
or the checked local skill directories. The code-polish commenting policy fits
the requested source readability work.

Conventions: Rust snake_case/rustfmt; Desktop camelCase and PascalCase/Prettier;
concise comments explain intent, constraints and failure behavior. Existing
source-header policy requires no boilerplate headers. Preserve API/serialized
names, strings, error and logging behavior, imports, module boundaries, public
interfaces, dependencies and test assertions. No file/directory renames or
cross-file extractions. No application build or installation is part of this pass.

Environment: macOS arm64; local file/Git/shell tools and catalog MCP available;
Hermit Rust 1.92.0 and pnpm 10.30.3. Reviews are self-reviews, not independent
criticism. Reads and independent baseline checks run concurrently; source edits
and their per-file validation remain sequential. Git and this report retain
resumable checkpoints. Generated/vendor/build outputs are excluded.

## Baseline before edits

All commands start with `source bin/activate-hermit`.

| Command | Baseline |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `env -u MUNINN_MCP_BEARER_TOKEN cargo test -p gosling --lib` | 1,872 passed; 3 opt-in benchmarks ignored |
| `cargo test -p gosling --test permission_audit_regressions --test tool_inspection_manager_tests` | 17 + 3 passed |
| `cargo clippy -p gosling --all-targets -- -D warnings` | Passed |
| Vitest: PermissionAuditRegression, ToolApprovalButtons, permissionRequests, sessionNotificationAdapter, chatSessionStore, chatSessionLifecycle, chatSessionController | 86 passed in 7 files |
| `pnpm --dir ui/desktop typecheck` | Passed |

Logs: `/tmp/gosling-readability-baseline-{fmt,core,integration,clippy,ui,typecheck}.log`.
The environment variable is unset only in the core test subprocess for the
existing environment-merge test. No global configuration changes.

## Completed source units

| Unit | Source file | Readability changes | POL codes |
| --- | --- | --- | --- |
| P1 | `crates/gosling/src/config/permission.rs` | Name locked snapshots versus unlocked file reads; clarify policy categories, fresh-read/failure contracts and annotation locals; remove narration | 001, 004, 005 |
| P2 | `crates/gosling/src/tool_inspection.rs` | Explain verdict precedence and advisory versus mandatory prompts; name request lookup and security action; remove procedural narration | 001, 004, 005 |
| P3 | `crates/gosling/src/permission/working_dir_scope_inspector.rs` | Name recursion limit, executable argument unwrapping and byte-range scan variables; explain parser/redirect representation constraints | 001, 005, 010 |
| P4 | `crates/gosling/src/security/egress_inspector.rs` | Name request/destination locals and denied-domain predicate; explain precedence and domain metadata | 001, 005 |
| P5 | `ui/desktop/src/acp/permissionRequests.ts` | Name retained-generation limit and replaced requests; document generation identity and resolution contract | 001, 004, 005, 010 |
| P6 | `ui/desktop/src/components/ToolApprovalButtons.tsx` | Name display-state cache by request identity; explain remount/async liveness and approval presentation | 001, 005 |

Private symbol reference sweeps include source and string/documentation references.
All caller updates stay inside the same source file. Any descriptive historical
report references remain historical and do not require executable call-site edits.
Each unit must preserve evaluation count/order, short circuit, scope, failure and
return behavior. No assertion changes or new tests mirroring implementation.

## Local rename manifest

All entries are private helpers, constants, parameters or local bindings. Each
definition and every executable reference changed within its original source file.
There are no file/directory renames, exported-symbol renames, compatibility aliases,
case-only moves, module-identity changes or deletions.

| Unit | Before → after |
| --- | --- |
| P1 | `read_file` → `read_permissions_file_unlocked`; `read_map` → `read_permissions_snapshot`; mutation/lookup `map` → `permissions`; private category parameter `name` → `category`; `write_annotated` → `mutating_tool_names`; `anns` → `annotations` |
| P2 | Inspector lookup `i` → `inspector`; `all_requests` → `requests_by_id`; `action_str` → `security_action` |
| P3 | `command_words` → `unwrap_command_words` (helper) / `executable_words` (local); shell flag `index` → `command_flag_index`; traversal `pending` → `pending_nodes`; `protected` → `literal_ranges`; `bytes` → `source_bytes`; `output` → `spliced_bytes`; scan `index` → `byte_index`; `ranges` → `remaining_literal_ranges`; depth literal `16` → `MAX_SHELL_ANALYSIS_DEPTH` |
| P4 | `tc` → `tool_call`; `name` → `tool_name`; `is_web` → `is_web_request`; `text` → `inspection_text`; `t` → `text`; destination `d`/`dest` → `destination`; metadata local `domains` → `flagged_domains`; existing denial predicate named `has_denied_domain` |
| P5 | `previous` → `replacedRequest`; `oldKey` → `candidateKey`; history literal `500` → `REQUEST_GENERATION_HISTORY_LIMIT` |
| P6 | `globalApprovalState` → `resolvedApprovalStates`; `recordApprovalState` → `rememberResolvedApproval`; cache parameter `id` → `requestIdentity`; `oldest` → `oldestRequestIdentity`; tool mapper `t` → `tool`; existing tool-name expression named `extensionToolNames` |

Reference sweeps found no remaining old private helper/cache references in
executable source. Two old cache-name mentions remain intentionally in historical
audits: `docs/cloud/2026-09-07-permissions-audit.md` and
`docs/cloud/audit-workflow-gui.md`. Their described snapshots were not rewritten.
Rust compilation, integration tests and Desktop tests/typechecking validate local
call-site updates. Public methods, JSON/YAML keys, permission values, ACP option
IDs, UI strings and logging fields/messages retain their existing identities.

## Comments and behavior-preservation review

The comments explain fresh reads and lock ownership, mandatory versus advisory
inspection, denial precedence, shell traversal/redirect representation, and the
difference between a request's identity, a delivered decision and a saved grant.
Constructor/read/write docs were checked against their implementation and the
atomic-file helper. Routine getter/setter narration and historical before/after
UI prose were removed. No boilerplate headers, TODOs, dead-code deletions or
logging/error-text changes were introduced.

The syntactic-sugar review applies only to existing Rust/TypeScript expressions:

- Naming `16` and `500` preserves the same numeric values, comparisons and types.
- `if domains.values().any(predicate)` becomes a local binding followed by the
  same `if`; evaluation occurs once at the same point, with the same short circuit.
- Naming the extension tool-name list preserves the conditional map, fallback
  list and second map in their original order. No operation moves across `await`.
- Other executable changes are local renames. Lock lifetimes, request/cache
  mutation order, parser byte offsets, loop order, error propagation, return
  values, async checks and dispatch boundaries remain unchanged on diff review.

No grammar, macro expansion, syntax feature, new lifetime or public abstraction
was introduced. There is no machine-generated AST-equivalence proof; confidence
comes from full hunk review plus unchanged before/after test outcomes.

## POL coverage

Coverage is limited to these six source files and their changed references.

| Code | Disposition | Evidence / action |
| --- | --- | --- |
| POL-001 | Fixed | P1–P6 private/local naming manifest above; all call-site edits stay in the owning file |
| POL-002 | Clean in scope | Existing Rust snake_case and Desktop camelCase/PascalCase filenames fit adjacent conventions; no paths renamed |
| POL-003 | N/A — no required header addition | Existing `source-header-policy.md` requires no boilerplate; no license, shebang or generated banner changed |
| POL-004 | Improved | P1/P2/P4/P5 document nontrivial policy, failure and request contracts; trivial accessor narration removed under AGENTS rules |
| POL-005 | Fixed | Replaced stale/generalized permission and historical UI comments with current constraints; reviewed each added claim against code |
| POL-006 | Clean in scope | Source/comment review found no commented-out executable block requiring removal |
| POL-007 | Clean in scope | No `dbg!`, `debugger;`, `console.log`, `todo!` or `unimplemented!` markers in the six-file scan; no debug instrumentation added |
| POL-008 | No cleanup warranted | No unused import/variable or unreachable-code deletion proposed; imports and initializers retained; Clippy/typecheck pass |
| POL-009 | Clean in scope | `TODO`, `FIXME`, `HACK`, `XXX`, `TEMP`, `temporary`, `workaround`, `cleanup` scan found no markers in these files; no ledger items removed |
| POL-010 | Fixed | P3 analysis depth and P5 retained-generation limits named with identical values |
| POL-011 | Preserved | Logging levels, events, fields and strings unchanged; no new logging/security finding identified in changed hunks; not a repository secrets audit |
| POL-012 | Preserved | Error paths/catches/strings unchanged; P1/P2/P5/P6 comments clarify existing failure and delivery behavior; no behavior repair included |
| POL-013 | Preserved | Existing tests already describe relevant scenarios; no fixture/assertion/name changes; only two same-file P1 private helper references updated |
| POL-014 | Verified | Source and documentation/string reference sweeps; old cache name retained only in the two historical audits and this manifest |
| POL-015 | Verified | Rustfmt, Prettier and `git diff --check` pass |
| POL-016 | Deferred | Scope inspector is 2,067 lines and egress inspector 1,118, including tests; route to `repair-source-modularization` only under future authorization permitting splits |
| POL-017 | Preserved; broader architecture not verified | Imports, module boundaries and public interfaces unchanged; no new boundary crossing; no repository-wide architecture verdict |
| POL-018 | No consolidation in scope | Reviewed local helpers serve distinct tasks; no duplicate extraction warranted in this patch; cross-file consolidation excluded by the operator |

## Validation and baseline comparison

Focused checks ran after each source unit, before moving to the next:

| Unit | Focused checks | Result |
| --- | --- | --- |
| P1 | `cargo test -p gosling --lib config::permission::tests`; `cargo test -p gosling --test permission_audit_regressions permission` | 19 + 7 passed |
| P2 | `cargo test -p gosling --lib tool_inspection::tests`; `cargo test -p gosling --test tool_inspection_manager_tests` | 5 + 3 passed |
| P3 | `cargo test -p gosling --lib working_dir_scope_inspector`; `cargo test -p gosling --test permission_audit_regressions shell` | 32 + 2 passed |
| P4 | `cargo test -p gosling --lib egress_inspector`; `cargo test -p gosling --test permission_audit_regressions egress` | 25 + 2 passed |
| P5 | Vitest permissionRequests + PermissionAuditRegression; Desktop typecheck | 12 passed; typecheck passed |
| P6 | Vitest ToolApprovalButtons + PermissionAuditRegression; Desktop typecheck | 12 passed; typecheck passed |

Then the baseline command set was rerun with identical flags:

| Check | Before | After | Delta |
| --- | --- | --- | --- |
| Core library | 1,872 passed; 3 ignored | 1,872 passed; 3 ignored | None |
| Permission audit regressions | 17 passed | 17 passed | None |
| Inspection integration | 3 passed | 3 passed | None |
| Seven Desktop test files | 86 passed | 86 passed | None |
| Core all-target Clippy, warnings denied | Passed | Passed | None |
| Desktop typecheck | Passed | Passed | None |
| Rustfmt check | Passed | Passed | None |

Final commands, after `source bin/activate-hermit`:

```sh
cargo fmt --all -- --check
env -u MUNINN_MCP_BEARER_TOKEN cargo test -p gosling --lib
cargo test -p gosling --test permission_audit_regressions --test tool_inspection_manager_tests
cargo clippy -p gosling --all-targets -- -D warnings
pnpm --dir ui/desktop exec vitest run src/components/PermissionAuditRegression.test.tsx src/components/ToolApprovalButtons.test.tsx src/acp/__tests__/permissionRequests.test.ts src/acp/__tests__/sessionNotificationAdapter.test.ts src/acp/__tests__/chatSessionStore.test.ts src/acp/__tests__/chatSessionLifecycle.test.ts src/acp/__tests__/chatSessionController.test.ts
pnpm --dir ui/desktop typecheck
pnpm --dir ui/desktop exec prettier --check src/acp/permissionRequests.ts src/components/ToolApprovalButtons.tsx
git diff --check
```

Logs: `/tmp/gosling-readability-p*.log` and
`/tmp/gosling-readability-final-{fmt,core,integration,clippy,ui,typecheck,prettier}.log`.
These are local temporary logs; durable counts and commands are recorded here.
Final comment review clarified cancellation and delivery-failure wording; the
Desktop command set was rerun afterward and passed again. No failed or newly
passing baseline test required an unexplained-delta disposition.

## Diff audit and handoff

Every source hunk was read against `da493ef4d` and maps to P1–P6 above: local
naming (POL-001), contract/comment clarity (POL-004/005), named existing limits
(POL-010), or configured formatter consequences (POL-015). Test hunks are only
P1's two local private-helper call-site renames. Documentation consists of this
report and a dated checkpoint in the existing permission session log. No
unsupported source edits or pre-existing user changes were overwritten.

Completed: the bounded permission-workflow readability pass, with 1,978 tests
passing and the same 3 opt-in benchmarks ignored as the baseline. This verifies
the exercised source paths; no claim is made for the entire repository, ignored
benchmarks, an independent review or an installed-GUI runtime replay.

The six source files retain their original responsibilities and public
interfaces. Large-file splitting, architecture changes and cross-file
deduplication are deferred under the operator's constraint. No release build,
packaging, reinstall, application backup, app restart or live service mutation
occurred during this pass. Tests compiled the changed code as part of validation.
