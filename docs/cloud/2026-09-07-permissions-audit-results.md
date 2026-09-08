# Permission audit: test evidence — 2026-09-07

Target `0105cd449`. See [audit report](2026-09-07-permissions-audit.md).
These are observed test results, not passing claims about the defects.

## Command ledger

Every Cargo/PNPM invocation was preceded by `source bin/activate-hermit`.
No `cargo build`, release packaging, installation or app launch was run.

| Command | Exit/result |
| --- | --- |
| `cargo test -p gosling --lib working_dir_scope_inspector` | 0; 32 passed |
| `env -u MUNINN_MCP_BEARER_TOKEN cargo test -p gosling --lib` | 0; 1,864 passed, 3 ignored |
| `cargo test -p gosling --test tool_inspection_manager_tests` | 0; 3 passed |
| `cargo test -p gosling-cli --bin gosling non_interactive` | 0 but **zero tests**; not counted |
| `cargo test -p gosling-cli --lib non_interactive` | 0; corrected target, 3 passed |
| `pnpm --dir ui/desktop test:run src/components/ToolApprovalButtons.test.tsx src/acp/__tests__/permissionRequests.test.ts src/components/PermissionAudit20260907.test.tsx` | 1; 16 existing tests pass, new probe fails |
| `pnpm --dir ui/desktop test:run src/acp/__tests__/chatSessionLifecycle.test.ts src/acp/__tests__/chatSessionStore.test.ts src/acp/__tests__/chatSessionController.test.ts src/acp/__tests__/sessionNotificationAdapter.test.ts` | 0; 64 passed |
| `pnpm --dir ui/desktop typecheck` | 0 |
| `cargo test -p gosling --test permission_audit_20260907 -- --nocapture` (final source) | 101; 2 passed, 6 failed |
| `cargo fmt --all` followed by `cargo fmt --all -- --check` | Formatting executed; final check recorded below |

The core library ignored only three opt-in benchmarks: last_message_snippets,
session_listing and begin_tool_operation. The 32 scope tests are a subset of the
1,864 library passes. The 80 existing UI passes exclude the deliberately failing
new UI regression.

The first probe run had seven cases (two passed, five failed). The final probe
source adds a two-thread barrier-controlled writer test and collects both egress
ordering counterexamples before asserting. Final results supersede that first run.

## Rust probe output (final)

```text
Compiling gosling v1.2.1 (/Users/eric/Work/vscode/forked/gosling/crates/gosling)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.82s
     Running tests/permission_audit_20260907.rs (target/debug/deps/permission_audit_20260907-3ac2f3d83508a004)

running 8 tests

thread 'concurrent_permission_writers_preserve_both_decisions' (409655) panicked at crates/gosling/tests/permission_audit_20260907.rs:127:5:
assertion `left == right` failed: both independent concurrent writes returned success and must survive
  left: [None, Some(AlwaysAllow)]
 right: [Some(NeverAllow), Some(AlwaysAllow)]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

thread 'egress_checks_later_upload_to_the_same_destination' (409656) panicked at crates/gosling/tests/permission_audit_20260907.rs:272:5:
earlier requests suppressed upload checks: [("curl https://audit.invalid/endpoint", [InspectionResult { tool_request_id: "first", action: Allow, reason: "Egress destinations detected: https://audit.invalid/endpoint", confidence: 0.6, inspector_name: "egress", finding_id: None, metadata: None }]), ("printf '%s' 'https://audit.invalid/endpoint'", [])]

thread 'independent_permission_writer_preserves_existing_denial' (409658) panicked at crates/gosling/tests/permission_audit_20260907.rs:83:5:
assertion `left == right` failed: a successful unrelated write must preserve another writer's denial
  left: None
 right: Some(NeverAllow)
test concurrent_permission_writers_preserve_both_decisions ... FAILED

thread 'existing_permission_reader_observes_revocation' (409657) panicked at crates/gosling/tests/permission_audit_20260907.rs:101:5:
assertion `left == right` failed: a long-lived manager must not retain revoked authority
  left: Some(AlwaysAllow)
 right: Some(NeverAllow)
test egress_checks_later_upload_to_the_same_destination ... FAILED
test independent_permission_writer_preserves_existing_denial ... FAILED
test existing_permission_reader_observes_revocation ... FAILED
test observed_hosted_save_failure_returns_without_a_grant ... ok

thread 'literal_heredoc_does_not_hide_a_later_outside_write' (409659) panicked at crates/gosling/tests/permission_audit_20260907.rs:213:5:
the real write after literal heredoc data must require approval: []

thread 'readonly_workspace_rejects_env_and_background_mutations' (409662) panicked at crates/gosling/tests/permission_audit_20260907.rs:187:5:
mutations missing read-only denial: ["env", "background"]
test readonly_workspace_accepts_a_comment_with_an_apostrophe ... ok
test readonly_workspace_rejects_env_and_background_mutations ... FAILED
test literal_heredoc_does_not_hide_a_later_outside_write ... FAILED

failures:

failures:
    concurrent_permission_writers_preserve_both_decisions
    egress_checks_later_upload_to_the_same_destination
    existing_permission_reader_observes_revocation
    independent_permission_writer_preserves_existing_denial
    literal_heredoc_does_not_hide_a_later_outside_write
    readonly_workspace_rejects_env_and_background_mutations

test result: FAILED. 2 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

error: test failed, to rerun pass `-p gosling --test permission_audit_20260907`
```

## Desktop approval probe output

The expected error logs for mocked save failure belong to existing negative tests.
The single failed test is the new request-identity regression.

```text
> gosling-app@1.2.1 test:run /Users/eric/Work/vscode/forked/gosling/ui/desktop
> vitest run src/components/ToolApprovalButtons.test.tsx src/acp/__tests__/permissionRequests.test.ts src/components/PermissionAudit20260907.test.tsx


 RUN  v4.1.0 /Users/eric/Work/vscode/forked/gosling/ui/desktop

 ❯ src/components/PermissionAudit20260907.test.tsx (1 test | 1 failed) 100ms
     × keeps a second session approval actionable when its tool id was used before 99ms

⎯⎯⎯⎯⎯⎯⎯ Failed Tests 1 ⎯⎯⎯⎯⎯⎯⎯

 FAIL  src/components/PermissionAudit20260907.test.tsx > 2026-09-07 permission audit > keeps a second session approval actionable when its tool id was used before
TestingLibraryElementError: Unable to find an accessible element with the role "button" and name "Allow Once"

Here are the accessible roles:

  paragraph:

  Name "":
  <p
    class="text-sm text-muted-foreground mt-2"
  />

  --------------------------------------------------

Ignored nodes: comments, script, style
<body>
  <div />
  <div>
    <p
      class="text-sm text-muted-foreground mt-2"
    >
      developer__shell
       -
      Allowed once
    </p>
  </div>
</body>
 ❯ Object.getElementError ../node_modules/@testing-library/dom/dist/config.js:37:19
 ❯ ../node_modules/@testing-library/dom/dist/query-helpers.js:76:38
 ❯ ../node_modules/@testing-library/dom/dist/query-helpers.js:52:17
 ❯ ../node_modules/@testing-library/dom/dist/query-helpers.js:95:19
 ❯ src/components/PermissionAudit20260907.test.tsx:34:19
     32|       { wrapper: IntlTestWrapper }
     33|     );
     34|     expect(screen.getByRole('button', { name: 'Allow Once' })).toBeInT…
       |                   ^
     35|   });
     36| });

⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯[1/1]⎯


 Test Files  1 failed | 2 passed (3)
      Tests  1 failed | 16 passed (17)
   Start at  19:59:11
   Duration  957ms (transform 142ms, setup 510ms, import 253ms, tests 285ms, environment 1.25s)

 ELIFECYCLE  Command failed with exit code 1.
```

## CLI headless behavior

```text
Compiling gosling-cli v1.2.1 (/Users/eric/Work/vscode/forked/gosling/crates/gosling-cli)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.98s
     Running unittests src/lib.rs (target/debug/deps/gosling_cli-88b191298f67ba9e)

running 3 tests
test session::non_interactive_confirmations_are_denied ... ok
test commands::doctor::tests::doctor_report_is_bounded_and_non_interactive ... ok
test cli::tests::session_remove_accepts_non_interactive_confirmation ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 251 filtered out; finished in 0.00s
```

## Inspection aggregation

```text
Compiling gosling v1.2.1 (/Users/eric/Work/vscode/forked/gosling/crates/gosling)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.61s
     Running tests/tool_inspection_manager_tests.rs (target/debug/deps/tool_inspection_manager_tests-1347ac6b42da8408)

running 3 tests
test flagged_domain_comes_from_any_result_for_the_request_and_only_when_unambiguous ... ok
test security_prompt_is_not_shadowed_by_an_earlier_allow_for_the_same_request ... ok
test test_inspect_tools_aggregates_and_handles_errors ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## Desktop lifecycle

```text
> gosling-app@1.2.1 test:run /Users/eric/Work/vscode/forked/gosling/ui/desktop
> vitest run src/acp/__tests__/chatSessionLifecycle.test.ts src/acp/__tests__/chatSessionStore.test.ts src/acp/__tests__/chatSessionController.test.ts src/acp/__tests__/sessionNotificationAdapter.test.ts


 RUN  v4.1.0 /Users/eric/Work/vscode/forked/gosling/ui/desktop


 Test Files  4 passed (4)
      Tests  64 passed (64)
   Start at  20:02:17
   Duration  958ms (transform 273ms, setup 332ms, import 312ms, tests 494ms, environment 993ms)
```

## Reproduction sources and isolation

- [Rust probes](2026-09-07-permissions-audit-probes.rs) were run as
  `crates/gosling/tests/permission_audit_20260907.rs`.
- [UI probe](2026-09-07-permissions-audit-ui-probe.tsx) was run as
  `ui/desktop/src/components/PermissionAudit20260907.test.tsx`.
- Temporary runner files were removed after byte-comparing them with the archived
  source. No failing regression was silently changed to accept the broken result.
- Rust managers/databases use per-test TempDirs. The writer concurrency case uses
  two actual threads with independent managers and a barrier before either writes.
  It is not a separate-process crash or scheduler-stress test.
- No fixture shell or network command is executed: these tests inspect text and
  assert decisions. The UI fixture mocks transport availability, not component
  state; it tests a real remount against the actual module-global approval cache.
- `observed_hosted_save_failure_returns_without_a_grant` intentionally asserts
  the observed defect (normal return with absent grant). Its pass is evidence of
  the swallowed-save behavior, not successful persistence.

## Final artifact checks

`cargo fmt --all -- --check` exited 0 after the temporary probe runner files were
removed. The artifact validation checked taxonomy completeness, link/path and line
bounds, archived probe count, temporary-file removal, whitespace, the AGENTS
governance marker, `git diff --check`, and absence of code changes:

```text
PASS: 205 unique taxonomy dispositions and 28 standalone dispositions.
PASS: four audit artifacts plus session log; links/source paths/line bounds valid.
PASS: eight archived Rust probes; temporary Rust/UI runner files removed.
PASS: whitespace/newline/governance checks and git diff --check.
PASS: no tracked code changes; untracked files limited to four audit artifacts.
```

Final working tree: four new audit artifacts and an appended existing session log.
No production-source or permanent-test changes were made in this audit. Saved
output omits ANSI color escapes and whitespace-only formatting; assertion content
is preserved.
