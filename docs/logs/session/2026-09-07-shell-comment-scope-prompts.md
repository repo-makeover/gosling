# Shell comment parsing and workspace approvals (2026-09-07)

## Intake and baseline

- Target: `/Users/eric/Work/vscode/forked/gosling`, branch `main`, clean at
  `cb72aabce` before this repair.
- Source finding **WDS-GSL-001**: the operator's screenshot shows an Autonomous
  workspace chat asking to approve a write to `/dev/null` followed by command text.
  Classification: correctness / workflow interruption, P2, localized repair.
- Session `20260906_50` has mode `auto`, cwd `/Users/eric`, additional directory
  `/Users/eric/Documents`, and `restrict_tools_to_working_dirs = false` (read-only
  inspection of `~/.local/share/gosling/sessions/sessions.db`).
- The 2026-09-07 CLI log records the `working_dir_scope` ALERT for
  `call_M3ym2OpL2YNTZWDuyiTULYoN` at `2026-09-08T00:34:25.606631Z`.
  Its stored command contains a Python heredoc comment beginning
  `# Dedicated test uses launchd's real activation API`, followed later by
  `curl ... >/dev/null` and another `launchctl print` pipeline.
- The source scanner enters single-quote state at the apostrophe in the comment.
  Subsequent lines merge into one segment; `embedded_redirect_target` sees the
  `/dev/null` suffix plus later command text as a path. The exact device-stream
  exemption consequently cannot match. This is a source trace of the real stored
  request, not an executed replay of its host-changing commands.
- The request already has a successful stored response. This repair does not
  approve, cancel, or replay it against live services.

## Plan and contract baseline

One stage: correct comment handling in `split_shell_command`; retain quote and
escape behavior for actual arguments. Add adjacent parser and workspace-inspector
regressions for literal hashes, escaped spaces/newlines, diagnostic heredocs, and
real writes after comments. No configuration, persisted format, or API changes.

Governing sources: `AGENTS.md` (canonical execution rules), `docs/architecture.md`
(core owns permissions), ADR-0017 (accepted workspace mutation boundary), and
`docs/TODO.md` AUT-GSL-001–003 (ordinary autonomous approvals). The policy remains
conformant; the parser has a pre-existing defect identifying a mutation target.
`WorkingDirScopeInspector::auto_downgrades_require_approval` deliberately returns
false, so workspace findings survive Auto mode. That policy is unchanged.

Workflow: private catalog `repair-defect-patchset`, scoped to the supplied
screenshot. The user's request authorizes this source repair; assume no change
to workspace grants. Independent reads were batched; source edits and validation
are sequential because checks depend on the patch. This log is the checkpoint.

Validation boundary: `AGENTS.md` reserves Cargo build/test/Clippy runs for an
explicit user request. Formatting, source review, and structural scans will run;
new regression tests and an installed-app replay remain unexecuted until then.
Do not treat earlier release-suite passes as validation of this patch.

## Progress

- Gates 0–3: source, session, policy baseline, and repair plan captured.
- Gate 4: `split_shell_command` now skips unquoted comments beginning at a word
  boundary through the newline, without interpreting their quotes or separators.
  Quoted/escaped hashes and hashes within words remain ordinary argument content.
- Added three regression tests: comment boundaries; literal hashes and line
  continuations; a workspace inspector fixture retaining the triggering heredoc
  comment, `/dev/null` redirection, and `launchctl` pipeline. The fixture also
  expects approval for genuine writes outside the project. These tests inspect
  commands without executing them.

## Review and validation

| Check | Result and scope |
| --- | --- |
| `source bin/activate-hermit` then `cargo fmt --all` | Passed; Rust source formatted |
| `git diff --check` | Passed |
| Targeted Python source scan comparing production text with `git show HEAD:<path>` | Passed: only `split_shell_command` changes; the inspector policy and every other production function remain byte-identical |
| Source-record/link/ignore/governance-marker scan | Passed; log is trackable, TODO link resolves, AGENTS marker remains present |
| Regression execution, crate build, Clippy | Not run: AGENTS requires an explicit build/test request |
| Original installed-app interaction | Not rerun; installed binary unchanged |

Patch review and a separate completeness pass were performed by the implementing
agent, not an independent reviewer. Quote state, escaped spaces, escaped newlines,
literal hashes, comment-only lines, and commands following comments were traced.
Review caught an overly broad Unicode whitespace check in the initial patch; it
now uses only space/tab word boundaries to match `shell_words`. A nonbreaking-space
case was added. No new finding remains from this source review, but that does not
establish runtime equivalence or close the test/reviewer gates.

The changed function feeds `shell_segments`, then `mutation_paths` / `referenced_paths`
and read-only classification. The manager's Auto exception, read-only-root denials,
canonicalization, and structured-tool handling are unchanged. The post-patch declared
contract comparison is **no new drift**, with behavioral verification pending.
This remains a heuristic shell scanner, not a complete parser for arbitrary heredoc
languages or nested shell programs; no broader parser-correctness claim is made.

Files changed: `working_dir_scope_inspector.rs`, `.gitignore`, `docs/TODO.md`, and
this log. WDS-GSL-001 is newly recorded in TODO as source-patched but open for
verification; no prior historical finding or release evidence was rewritten.

Status: **completed_with_partial_verification** for the source patch; installed-app
repair remains pending. A question is pending for explicit test/build/install
authorization. Next: run the targeted `working_dir_scope_inspector` tests and
`tool_inspection_manager_tests`, then the relevant Rust lint/build checks and a
backed-up server installation if authorized. Do not execute the stored launchctl
command as a replay against live services.

## Authorized audit and test follow-up (2026-09-07)

This checkpoint supersedes the earlier pending test-authorization status above.
The operator requested another full audit suite followed by tests and explicitly
said not to build the app yet. The shell-comment repair is now committed as
`0105cd449`. No further production code was changed in this follow-up.

- All 13 catalog base audit lenses were applied to the permission workflow, with
  205 taxonomy dispositions and explicit runtime/specialist coverage limits.
- Seven additional findings remain open: shell grammar gaps, batch egress
  deduplication, independent-writer/stale-reader permission state, swallowed
  persistent-save errors, cross-session UI approval state, legacy Claude Code
  ignoring Always Allow persistence, and a stale permission-lock completion claim.
- The 32 scope regression tests pass, including all three tests from the source
  repair. The full Gosling library run passed 1,864 tests with 3 opt-in benchmarks
  ignored. Inspection integration and CLI noninteractive suites each passed 3.
- Existing Desktop approval/request and lifecycle tests passed 80 tests total;
  Desktop typechecking passed.
- Isolated audit probes produced 6 Rust invariant failures and 1 React failure,
  confirming open defects; two Rust probes passed (a benign-comment control and
  characterization of a swallowed permission-save failure). The report separates
  component/inspector reproduction from untested installed-app consequences.
- Test-only compilation occurred. No application/release build, packaging,
  installation, restart or live host-command replay occurred.
- The temporary runner files were removed after preserving byte-identical probe
  sources under `docs/cloud/`; the application source and permanent tests retain
  no changes from this follow-up. Formatting and final artifact checks passed.

Evidence: [full audit](../../cloud/2026-09-07-permissions-audit.md),
[test results](../../cloud/2026-09-07-permissions-audit-results.md),
[Rust probes](../../cloud/2026-09-07-permissions-audit-probes.rs), and
[React probe](../../cloud/2026-09-07-permissions-audit-ui-probe.tsx).

Current WDS-GSL-001 status: **source repair test-verified; installed-app verification
pending**. The seven broader audit findings are reported, not repaired. The stale
TODO claims are explicitly preserved as reconciliation follow-ups rather than
rewriting historical evidence during an audit.

## Authorized seven-finding repair and implementation review (2026-09-07)

This checkpoint supersedes the earlier seven-open-findings status. The operator
authorized repairing all seven, checking the implementation and repairing related
issues found during that check, while retaining the restriction on app builds.

All seven findings are now source-repaired and test-verified. Changes cover shell
grammar inspection, request-local egress checking, locked/fresh permission state,
save-before-dispatch and failure retry, legacy native provider persistence,
Desktop session/request identity and metadata, and corrected permission-lock
history. Related revocation, offered-option, replacement and domain-scope defects
found during review were also repaired. Source ownership and the two self-review
passes are recorded in the
[repair report](../../cloud/2026-09-07-permissions-repair.md).

Final validation passed 1,872 core tests (3 opt-in benchmarks ignored), 17 audit
regressions, 3 inspection integration tests, 3 targeted CLI tests and 86 Desktop
tests. Core all-target Clippy, Desktop typecheck and translation checks passed.
Formatting and record checks are recorded with the final repair report. The
original audit/probe artifacts remain historical evidence; dated addenda point
to the current dispositions. TODO now distinguishes the old config lock commit
from the newly tested permission lock.

Status: **source repair and implementation review complete**. The installed app
is unchanged; no application build, packaging, installation, restart or replay
of host-changing commands occurred. Tests compiled as authorized. This is scoped
permission-workflow validation and two reviews by the implementing agent, not
an independent or repository-wide clean-security verdict.
