# Permission audit repairs and implementation review — 2026-09-07

Status: **all seven findings source-repaired and test-verified**. Two final
self-review passes are complete. The installed application has not been changed;
the operator's restriction on application builds and installation remains in force.

Source: [seven-finding audit](2026-09-07-permissions-audit.md). Baseline:
`main` at `2d03400a7`, clean. User authorizes all seven repairs, implementation
review, related defect repair, and tests. App/release builds, packaging,
installation and restarts remain excluded. Test compilation is authorized.

## Plan and baseline (Gates 0–3)

| Stage | Finding | Domain / priority / complexity | Touch set and regression boundary |
| --- | --- | --- | --- |
| 1 | SEC-GSL-902 | security / P0 / low | EgressInspector::inspect; request-local deduplication, downloads/literals followed by uploads |
| 2 | CON-GSL-901 | data integrity / P0 / high | PermissionManager read/mutate/persist; independent processes, revocations, failure rollback |
| 2 | WFG-GSL-901 | reliability / P1 / high | ToolInspectionManager, approval execution, ACP and Desktop completion; persistent-save failures must be visible and retryable without repeating execution |
| 2 | INV-GSL-901 | correctness / P1 / medium | ClaudeCodeProvider native approvals; provider/tool scoped persistence and recreation |
| 2 | WFG-GSL-902 | frontend/UX-bug / P2 / medium | Desktop pending request identity and ToolApprovalButtons; session changes, replacement and remount |
| 3 | SEC-GSL-901 | security / P0 / high | Shell segmentation, classification and path extraction in WorkingDirScopeInspector; heredocs, wrappers, background commands, benign diagnostics |
| 4 | CMP-GSL-901 | docs-defect / P3 / low | docs/TODO.md and source audit addendum; distinguish config lock history from newly verified permission locking |

Stage 1 is independent and has the smallest security fix surface. Stage 2 groups
all owners/consumers of durable approvals, with persistence preceding completion
and provider changes. Stage 3 shares no persistence contract. Stage 4 requires
verified repairs. Independent reads/checks may run concurrently; edits and
dependent validation are sequential. This document is the resumable checkpoint.

The audit's archived probes and current source inspection reproduce the same
defects at this baseline. Prior baseline checks: 1,864 core tests passed, three
manual benchmarks ignored; 32 scope tests, three inspection integration tests,
three CLI tests, 80 relevant Desktop tests and Desktop typecheck passed. Six Rust
invariant probes and one React probe failed. The passing hosted-save probe only
characterizes the defect and must become an error-propagation regression.
These old results are baseline evidence, not validation of this repair.

Governing sources: AGENTS.md is canonical; docs/architecture.md assigns permission
ownership to core and uses implementation formats as authoritative; accepted
ADR-0017 preserves pinned workspace policies and session-private additive grants;
docs/TODO.md AUT-GSL-001–003 declares ordinary autonomous approvals and persistent
provider/tool grants. Baseline drift: the seven source findings violate intended
inspection, durability and UI completion behavior. Existing serialized permission
categories and ACP options remain compatibility constraints. Compare these same
contracts after changes; do not amend policy to conceal defects.

Regression paths must pair the original repro with adjacent unchanged behavior.
Tests use temporary config/session directories, fake providers and command strings;
the host-changing screenshot command is never replayed. The final re-audit traces
changed owners, callers and consumers; a distinct completeness pass checks every
original finding and the combined regression surface. Both reviews are performed
by the implementing agent, not an independent reviewer.

## Progress

The following stage evidence was recorded during implementation. Final combined
validation and closure follow below.

- SEC-GSL-902: request-local destination deduplication. The original probe failed
  before repair and passes afterward (including two consecutive uploads); all
  25 egress inspector tests passed. Logs: `/tmp/gosling-permission-repair-egress*`.
- CON-GSL-901: stable `permission.yaml.lock` held across reload/mutate/atomic
  replacement; readers reload under a shared lock, with no cached authority.
  Damaged/unreadable policy fails closed and cannot be overwritten by an update.
  All three original storage probes failed before repair; seven storage checks
  passed after, including two child processes and revocation/removal. Log:
  `/tmp/gosling-permission-repair-storage.log`.
- WFG-GSL-901: saves return errors and precede dispatch. Failed decisions produce
  an actionable message and reopen approval, so retry does not repeat tool
  execution. UI labels distinguish a submitted persistent choice from an
  acknowledged bulk save. Original failure characterization is now an invariant.
- WFG-GSL-902: UI state uses session, tool ID and request generation. Replacements
  reset the component, and an old asynchronous submission cannot resolve a newer
  request. Original React probe failed before repair. Real request-store/component
  regressions cover cross-session reuse, in-place changes, remount and replacement.
- INV-GSL-901: legacy Claude Code saves/reuses provider/tool grants and denials;
  tests distinguish one-time approval from persistent approval across recreation.
  A failed save reports an error and sends a denial to the waiting CLI.
- The permission-filtered core run passed 122 tests, including three hosted save
  retries and native lifetime/error tests. Log:
  `/tmp/gosling-permission-repair-permission-suite.log`.
- Desktop stage-2 checks passed 37 tests (components, request store, adapter).
  Log: `/tmp/gosling-permission-repair-ui-stage2.log`. Typecheck initially hit
  the shell's old pnpm; rerun uses repository Hermit pnpm 10.30.3.

Related review repairs within the user's expanded authorization: explicit egress
denials and ACP provider denials must outrank ordinary Auto policy; unavailable
Desktop options must not report successful approval; the Desktop adapter must
refresh replacement prompts and retain domain metadata. These remain included in
the final regression/review scope. A compile check caught metadata attached to the
wrong ACP builder; it was moved to `ToolCallUpdate` before the passing core run.

Stage 3: SEC-GSL-901 uses the maintained tree-sitter Bash grammar instead of
extending the incompatible hand splitter. Dependencies were added with `cargo add`
(`tree-sitter` 0.26, `tree-sitter-bash` 0.25; lockfile resolves 0.26.13/0.25.1).
The temporary syntax-tree probe was removed after behavioral regressions replaced
it. An existing escaped-newline/literal-hash test exposed a parser edge case;
continuation splicing now preserves comments and literal data. All 32 scope tests
pass, including the original screenshot-shaped diagnostic fixture. Malformed
syntax cannot silently become an empty, permitted command. This remains a
heuristic inspection policy, not an OS sandbox or an interpreter verifier.

Stage 4 corrects the permission-lock history in `docs/TODO.md`, adds a dated
repair pointer to the historical audit/results, and appends the session record.
`git show --stat 37804170e` confirms that the old commit modified only
`crates/gosling/src/config/base.rs`. Its config repair remains valid; permission
locking is closed using this repair's new implementation and process tests.

## Final finding dispositions

These are repair dispositions for the existing detailed findings, not newly
invented findings or a claim of independent security certification.

| Finding | Final behavior / source owner | Regression evidence | Disposition |
| --- | --- | --- | --- |
| SEC-GSL-901 | `working_dir_scope_inspector.rs:analyze_shell_at_depth` visits executable syntax, redirects and shell-received heredocs; wrappers and background commands retain mutations; unsupported syntax requires approval | `readonly_workspace_rejects_env_and_background_mutations`, `literal_heredoc_does_not_hide_a_later_outside_write`, shell syntax matrix, malformed-syntax control, 32 existing scope tests | Repaired; observed inspector tests |
| SEC-GSL-902 | `egress_inspector.rs:inspect` deduplicates separately for each request | `egress_checks_later_upload_to_the_same_destination`, existing inbound/literal/loopback controls | Repaired; observed inspector tests |
| CON-GSL-901 | `config/permission.rs:mutate_permissions` locks the stable sidecar across fresh read/mutation/atomic replacement; `read_map` reloads under a shared lock | Independent managers, concurrent threads, two child processes writing 30 decisions each, stale-reader revocation, removal, corruption and failed updates | Repaired; observed storage/process tests |
| WFG-GSL-901 | `tool_inspection.rs` returns save errors; `agents/tool_execution.rs:handle_approval_tool_requests` saves before dispatch and reissues failed decisions; Desktop labels distinguish submission from acknowledged bulk persistence | `hosted_save_failure_is_reported_without_a_grant`; three `hosted_save_failure_keeps_dispatch_pending_and_retries_only_the_decision` cases; component assertions | Repaired; observed hosted stream/component tests |
| WFG-GSL-902 | `permissionRequests.ts` owns session/tool/generation identity; `ToolApprovalButtons.tsx` keys state and pending checks by that identity | Real request-store/component tests for session reuse, rerender, remount and replacement; stale-generation and separator-collision request-store tests | Repaired; observed component/store tests |
| INV-GSL-901 | `providers/claude_code.rs` saves native persistent decisions before replying and reloads provider/tool grants for later requests | `native_permission_reuse_matches_the_selected_lifetime` distinguishes Always Allow and Allow Once across requests and provider recreation; save failure returns an error and denial to the waiting CLI | Repaired; observed fake-provider protocol tests |
| CMP-GSL-901 | `docs/TODO.md` separates config lock history from the newly repaired permission file; historical audit preserved with addendum | Git commit file inventory, corrected source/record links, final document checks | Reconciled; source/history verified |

## Changed-workflow review (Gate 8)

The implementing agent traced the final producer, owner and consumer paths:
shell syntax → workspace verdict → inspector aggregation → hosted confirmation;
destination extraction → per-request egress decision → domain metadata → ACP →
Desktop; and durable permission mutation → fresh hosted/native-provider reads.
Failure and retry traces cover disk errors before dispatch, cancellation,
unsupported options, replaced request identities and provider recreation.

Additional confirmed issues found during this review were repaired in the same
authorized scope:

- Explicit egress and ACP provider denials now dominate ordinary Auto behavior;
  saved broad provider grants cannot suppress security-scoped approval or Chat.
- Multiple URLs on the same hostname produce one persistent domain option.
  ACP sends typed domain/tool metadata, the Desktop adapter retains it, and both
  inline approval rendering and replacement prompts consume the updated data.
- Unsupported Desktop choices remain pending instead of reporting success;
  domain-only choices cannot be mistaken for a tool-wide grant. One-time ACP
  requests do not advertise a persistent tool choice that cannot be stored.
- Request identity uses an encoded tuple rather than an ambiguous separator;
  an older asynchronous request cannot resolve its replacement. Remounts retain
  acknowledged bulk-grant context.
- A missing session propagates an inspection error instead of skipping its
  workspace policy. Domain approvals are recorded as allow decisions in the
  existing security event.

The final contract comparison against the baseline is **no new drift**.
Permission categories, serialized levels and provider/tool key format remain
unchanged. Persistence errors now reach the existing approval owner; ACP metadata
is additive. Workspace policy, read-only roots, session-private grants and the
separate security-approval boundary remain enforced. This repair does not change
Autonomous mode into an unrestricted permission bypass.

## Invariant inventory

Final invariant review uses catalog `audit-invariant-sync`, including its shared
contracts and detection/test guidance. Copies that intentionally differ are
identified explicitly; equality is not assumed across different grant scopes.

| Invariant | Ground truth | Acting copies / handling | Match class | Guard / final delta |
| --- | --- | --- | --- | --- |
| Persistent permission levels and categories | `config/permission.rs:PermissionLevel`, category constants | Mutation, YAML serde and fresh getters persist/read the same values | Required identical | Exhaustive Rust matches and config round-trip/precedence tests; no schema change or remaining delta |
| Decision meaning and lifetime | `crates/gosling-providers/src/permission.rs:Permission`, re-exported by core | Hosted save/dispatch; ACP mapping; legacy Claude native response; Desktop permission union/options | Same action meaning; tool/domain/provider scopes intentionally differ | Hosted save tests, ACP mapping/lifetime tests, native recreation tests and Desktop options tests; original native lifetime gap repaired |
| Durable state is the authority | `PermissionManager` transaction/read methods | Inspector, settings, egress and provider callers consume fresh state; writers merge under one sidecar lock | Required identical state source | Existing-reader, independent-writer, child-process and corruption tests; no cached grant remains |
| Request identity | `permissionRequests.ts:acpPermissionRequestIdentity` | Pending resolver validates identity; approval cache/component consume it; adapter refreshes payload | Required identical within a generation | Session, replacement, remount and separator tests; no id-only cache remains |
| Domain grant membership | Egress `domains` metadata plus offered domain option | Hosted single-domain selection; ACP metadata/options; adapter; inline buttons | Exactly one distinct domain; intentionally narrower than a tool grant | Multi-URL domain regression, adapter metadata/replacement test, domain button and option tests |
| Scope verdict ownership | Workspace inspector and inspector aggregation | Shell analysis feeds path/read-only classification; Auto preserves hard approvals; CLI rejects unattended hard prompts | Required propagation; ordinary Auto approvals intentionally differ | 32 scope tests, syntax matrix, malformed/missing-session regressions, aggregation and CLI tests |
| Lock closure history | `37804170e` file inventory and current permission implementation | TODO, historical audit addendum, this report, session log | Records must describe their actual checkpoint | Corrected attribution and separate dated closure; no retroactive rewrite of audit results |

| Required INV category | Final disposition in this repair scope |
| --- | --- |
| INV-001 Replicated Membership Set | No remaining confirmed mismatch: categories are owned in core; wire actions are covered by mapping tests |
| INV-002 Canonical Source Bypassed | Original stale permission authority repaired; consumers use the fresh-read manager |
| INV-003 Schema/Code Inventory Drift | Not applicable to changed code: no database relationship inventory changed |
| INV-004 Enum/Constraint/UI Value Drift | Existing Rust decisions and Desktop union retain all six actions; unsupported offered-option combinations reject without consuming the request |
| INV-005 Serialize/Deserialize Asymmetry | No remaining confirmed mismatch: permission serde shape unchanged; ACP domain metadata now reaches its consumer |
| INV-006 Export/Import Schema Mismatch | No export/import schema changed; existing permission round-trip tests pass |
| INV-007 Drift Guard Missing | Original behavior gaps now have permanent regressions; no new unguarded changed membership set identified. Cross-language mappings remain hand-maintained and tested, not generated |
| INV-008 Handling Class Omitted | Save, one-time dispatch, deny and cancel behavior explicitly matched; domain grants remain domain-scoped |
| INV-009 Unenforced Must-Match Invariant | Stale-state and UI identity invariants now have owner methods plus failure/replacement tests |
| INV-010 Silent Add-Site Gap | No new member added; Rust matches and Desktop `Record<Permission, string>` constrain action handling, while mapping tests cover offered subsets |
| INV-011 Authoritative Copy Not Consumed | Legacy native provider now consumes durable provider/tool policy; Desktop consumes request-store identity |
| INV-012 Permission/Guard Table Drift | Reviewed hosted, ACP and legacy native consumers; explicit denials and security-scoped constraints survive ordinary Auto defaults |
| INV-013 Migration/Model Drift | Not applicable: no migrations or ORM models changed |
| INV-014 Narrowed Subset Copy | Intentional: provider grants do not become global tool/domain grants; security and one-time requests offer narrower choices |
| INV-015 Divergence Class Unclassified | Grant-scope and offered-option differences classified above; no remaining confirmed unclassified divergence in changed paths |

Cross-lens routing: concurrency/integrity evidence owns the locking and revocation
repair; security owns shell and egress verdicts; workflow/frontend evidence owns
save completion and identity; compliance/documentation owns the historical
attribution. These are rechecks of the original suite's findings and changed
consumers, not an assertion that additional standalone specialist scans ran.

## Distinct completeness review (Gate 9)

After implementation review and the final combined tests, a separate pass mapped
each original finding to its permanent regression or source/history check,
checked changed helper callers, compared serialized contracts, checked that
temporary probes were removed, and reconciled all current status records.
The review also checked that a passing characterization of a swallowed save
error had become an assertion of propagated failure, rather than preserving a
test that passes only while the defect exists. No unresolved confirmed finding
remains within this seven-finding repair and its changed workflows.

## Final validation ledger

Commands used the repository Hermit environment. All entries below passed on the
final source; earlier failed development attempts are not counted as validation.
Validated snapshot: 21 changed source/dependency/test files, SHA-256
`4ad7d86aef3f6b3e79400c9613bf4862facb69bd3355291e9e669a8bbef03658`.
The digest covers sorted paths, each followed by NUL, file bytes and NUL; docs
are excluded so the evidence record can be reconciled after validation.

| Command / scope | Final result |
| --- | --- |
| `env -u MUNINN_MCP_BEARER_TOKEN cargo test -p gosling --lib` | 1,872 passed, 3 opt-in benchmarks ignored; includes all 32 scope tests |
| `cargo test -p gosling --test permission_audit_regressions --test tool_inspection_manager_tests` | 17 permanent audit regression tests and 3 inspection integration tests passed |
| `cargo test -p gosling-cli --lib non_interactive` | 3 passed; 251 unrelated tests filtered |
| `cargo clippy -p gosling --all-targets -- -D warnings` | Passed |
| Desktop Vitest: `PermissionAuditRegression`, `ToolApprovalButtons`, `permissionRequests`, `sessionNotificationAdapter`, `chatSessionStore`, `chatSessionLifecycle`, `chatSessionController` | 86 tests in 7 files passed |
| `pnpm --dir ui/desktop typecheck` | Passed |
| `pnpm --dir ui/desktop exec node scripts/i18n-check.js` | Passed |
| `cargo fmt --all -- --check`, Prettier check over the nine changed Desktop source/test/English-message files, `git diff --check` | Passed |
| Source-record/link/governance checks | Reconciled all seven IDs, checked local Markdown links and trackability, preserved the AGENTS governance marker |

Full local command logs: `/tmp/gosling-permission-repair-full-core.log`,
`/tmp/gosling-permission-repair-integrations.log`,
`/tmp/gosling-permission-repair-cli.log`,
`/tmp/gosling-permission-repair-clippy.log`,
`/tmp/gosling-permission-repair-ui-final.log`,
`/tmp/gosling-permission-repair-typecheck-final.log`, and
`/tmp/gosling-permission-repair-i18n.log`. The command/count ledger above is the
durable evidence record; temporary logs may disappear later.

Development checks caught and resolved the initial metadata builder mismatch,
a syntax-tree cursor lifetime error, the escaped-newline regression and one
Clippy `collapsible_else_if` warning. The first typecheck used an older shell
pnpm; final validation used Hermit pnpm 10.30.3. The Muninn token was unset only
for the test subprocess because an existing environment-merge test requires an
isolated environment; no user credential or global configuration was changed.

## Limits and handoff

This closes source repair and implementation validation. It does not establish
installed-app behavior: no application/release build, packaging, installation,
restart, live provider call or host-management command replay occurred. Cargo
compiled tests and Clippy targets as part of authorized validation. Native
provider protocol tests use fake streams; Desktop tests use component/store
fixtures. The entire workspace's unrelated crates and every Desktop test were
not run. Reviews were performed by the implementing agent, not an independent
reviewer.

Shell classification remains heuristic for arbitrary interpreter programs and
computed paths. The added Bash grammar covers the reproduced shell-structure
defects and tested adjacent syntax; this report does not claim universal shell
or OS-level isolation. Genuine workspace/security requests can still require
approval under the existing policy. Installing these repairs is a later task
under the operator's build restriction.

## Authorized GUI build and reinstall follow-up

The operator subsequently authorized building and reinstalling the Gosling GUI,
explicitly requesting **no application backup**. This supersedes the earlier
build/install restriction for this follow-up. Source baseline is clean `main`
at `cb1aac7ed`, containing the seven repairs. Target: macOS arm64,
`/Applications/Gosling.app`; existing bundle ID `com.electron.gosling`.

Execution uses catalog `plan-rust-app` for the existing Rust build boundary and
the repository's Electron packaging commands. Hermit supplies Rust 1.92.0,
Node 24.10.0 and pnpm 10.30.3. Plan: compile the release CLI/backend with default
features and the locked dependency graph, stage it using `just copy-binary`,
package the GUI, apply the documented local ad-hoc signing entitlements, then
quit, replace and launch the installed app. These stages are sequential because
the package consumes the compiled backend and installation consumes the verified
package. Cargo's existing target cache allows build resumption. No previous
application bundle will be retained or renamed as a backup.

Checkpoint: backend build started with
`./scripts/with-rusty-v8-cache.sh cargo build --locked --release -p gosling-cli --bin gosling`.
Build log: `/tmp/gosling-permission-install-backend.log`. Final acceptance will
verify signing, installed/backend payload equality, backend version and the
installed GUI's visible startup. Installation has not occurred at this checkpoint.

Packaging checkpoint: release backend succeeded in 2m 38s; `just copy-binary`
and `pnpm --dir ui/desktop run package` succeeded. The documented
`codesign --force --deep --sign - --entitlements ui/desktop/entitlements.plist`
signing step and deep/strict verification passed. Bundle version is 1.2.1 with
the existing bundle identifier. Release, staging and packaged backend SHA-256
all equal `b419c384f54625113108d9a2611bf123f12f85c3177a6be718c4ca59423f8f2c`.
Only this record changed in the tracked source tree during packaging.

Installation checkpoint: the application was quit through its native Quit menu;
all processes under the installed bundle exited before replacement. The new
bundle was staged and signature-verified, the old `/Applications/Gosling.app`
was removed, and the new bundle moved into that path. **No application backup
was created.** Installed deep/strict signature verification passed. Installed
backend SHA-256 matches the release/package value above; installed `app.asar`
matches the package at
`46cf2957eac0ddbd5f6cde8e277907b03056fabe2d9e67b798a1b79d9322b4f1`.
The installed backend reports 1.2.1.

Acceptance limit: the new installed GUI process launches, but window inspection
times out. A one-second process sample shows its main thread waiting in macOS
`SecItemCopyMatching` / SecurityServer while `SecurityAgent` is running.
Computer Use explicitly refuses access to `com.apple.SecurityAgent` for safety.
The system Keychain interaction must be completed by the operator; no credential
was retrieved, changed or entered, and no Keychain access control was bypassed.
**Build and reinstall complete; visible GUI startup verification pending the
macOS Keychain interaction.** The prior installed-app-pending repair status is
superseded only to this extent; a live permission-command replay was not run.

Build/package/signature logs are retained under
`/tmp/gosling-permission-install-{backend,sdk,package,sign}.log`; the diagnostic
sample is `/tmp/gosling-permission-install-startup-sample.txt`. No runtime source,
dependency lockfile, application settings or session data was changed during
the installation follow-up.

Startup retry (2026-09-08): the operator requested another attempt. Opening
`/Applications/Gosling.app` now succeeds. The installed GUI visibly loads its
workspace list, model selector and new-chat controls; its embedded backend and
renderer processes are running from the installed bundle. The backend reports
1.2.1. **Build, reinstall and visible startup verification are complete.** The
earlier Keychain startup blocker is no longer present on this attempt. No
SecurityAgent interaction, credential change or application backup occurred.
This verifies startup, not a live host-changing permission-command replay.
