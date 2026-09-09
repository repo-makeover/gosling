# Cross-cutting lens audit: negative-space, invariant-sync, resource-lifecycle, recovery-idempotency

Date: 2026-09-08. Baseline: `main` at `c75f08946`, clean. Scope: today's commits,
`cb1aac7ed8df0e9661deb70957934c96550a0a1c..c75f08946` (19 commits), plus adjacent
consumers each lens's method required tracing (shared constants, DTO round-trips,
callers). Security audit explicitly out of scope.

This pass runs the four lenses **not** covered by the same-day
[evening audit and repair](2026-09-08-evening-audit-repair.md): negative-space,
invariant-sync, resource-lifecycle, recovery-idempotency. That prior pass's ten
findings (DAT-TODAY-001/002, REL-TODAY-001/002, WFG-TODAY-001-004, ARC-TODAY-001/002)
are treated as closed and are not relitigated here; where this pass touches the same
files, each finding below states explicitly why the mechanism is new and distinct
from that pass's disposition.

## Method

Four independent research agents applied `audit-negative-space`, `audit-invariant-sync`,
`audit-resource-lifecycle`, and `audit-recovery-idempotency` from
`/Users/eric/Work/vscode/agent-skills/010_audit/` as blind, read-only passes over the
diff, each producing an assumption ledger / invariant-sync inventory / resource
ownership matrix / recovery map plus findings in the standard finding format. A
parent reviewer then triaged every finding independently against source before
authorizing any repair — re-deriving reachability, re-reading cited call sites, and
in one case (INV-CROSS-002, FSR-CROSS-002) correcting or narrowing the sub-agent's
proposed mechanism after finding the literal suggestion did not hold up. Only findings
with concrete reproduction evidence were repaired; the two lenses reporting no
material findings say so plainly below rather than being padded with speculative
items.

## Findings and disposition

| ID | Lens | Severity | Confidence | Disposition |
|---|---|---|---|---|
| NEG-CROSS-001 | Negative-space | High | Confirmed | Repaired: invalid `GOSLING_AUTO_COMPACT_THRESHOLD` no longer hard-fails every `reply()` |
| NEG-CROSS-002 | Negative-space | Medium | Plausible | Not repaired — flagged as a security/operator-owned follow-up |
| NEG-CROSS-003 | Negative-space | Low | Plausible | Repaired: `copyArtifactContents`'s concurrent-modification guard strengthened |
| RES-CROSS | Resource-lifecycle | — | — | No confirmed findings; one Info-level test-coverage gap noted, not repaired |
| INV-CROSS-001 | Invariant-sync | Low | Confirmed | Repaired: `Select.tsx` now consumes the shared `Z_INDEX` constant |
| INV-CROSS-002 | Invariant-sync | Low | Confirmed | Repaired: corrected `Z_INDEX.TOOLTIP`'s now-false doc comment |
| FSR-CROSS-001 | Recovery-idempotency | Medium | Confirmed (mechanism); Likely (crash manifestation) | Repaired: compaction's conversation replace + usage update made atomic |
| FSR-CROSS-002 | Recovery-idempotency | Medium | Confirmed (mechanism); Likely (crash manifestation) | Repaired: `restore_output_revision` detects and refuses a phantom-latest (torn-write) state |

Totals: 8 findings raised across 4 lenses; 6 repaired, 1 documented-only
(operator/security-owned), 1 lens (resource-lifecycle) produced no repairable finding.

---

## 1. audit-negative-space

### Surface reviewed

Budget: ~28 files / tool calls, prioritized on `context_mgmt/mod.rs` (budget-cap
auto-compaction), the reply loop (`reply_entry.rs`, `reply_stream.rs`,
`tool_dispatch.rs`), output-revision storage, the permission/working-dir-scope and
egress inspectors, the new ACP custom-dispatch surface, and the new artifact IPC
handlers/UI (`fileIpc.ts`, `ArtifactPane.tsx`, `ArtifactFileList.tsx`).

### NEG-CROSS-001: Invalid auto-compact threshold turns every `reply()` into a hard failure

Severity: High
Confidence: Confirmed
Evidence basis: source-evidenced, test-reproduced
Domain: Negative-Space (NEG-001 impossible-state-possible / NEG-009 safety bypassed by alternate path)

Evidence:
- `crates/gosling/src/context_mgmt/mod.rs` (pre-fix, from commit `a48108750`):
  `check_if_compaction_needed` called `validate_compaction_settings(threshold, 0.0)?`
  unconditionally, propagating `Err` for any out-of-range value.
- `crates/gosling/src/agents/agent/reply_entry.rs:332` (pre-fix):
  `check_if_compaction_needed(...).await?` — the `?` propagates that `Err` out of
  `reply()`'s setup path, before the turn lease is even created.
- `crates/gosling/src/agents/agent/reply_stream.rs:184-190` (unchanged, verified):
  a second call site inside the turn loop has the identical `.await?` propagation,
  so the same defect also breaks the mid-turn proactive-compaction check.
- Today's own commit `a48108750` changed the test
  `test_check_if_compaction_needed_returns_false_when_disabled` to assert
  `.is_err()` for `[1.0, 1.5, -0.1, f64::NAN]` — i.e. the regression shipped with
  matching (self-confirming) test coverage, which is why it survived that commit's
  own review.

Observed behavior:
- Before `a48108750` (yesterday's/earlier behavior), an out-of-range threshold
  (`>= 1.0`, the realistic typo case — e.g. `5` instead of `0.5`) was treated as
  "auto-compaction disabled" with a one-time warning and `Ok(false)`.
  `a48108750` replaced that with unconditional validation that raises `Err`, which
  propagates through both `reply()` call sites via `?`. Since `Config::global()`
  reads the environment/settings file once and the value doesn't change between
  calls, this is not a one-off failure: it is a hard failure on *every* subsequent
  `reply()` call for the life of the process, for a value that has no other legal
  disabling representation available to the operator except `0`.

Expected boundary:
- A config-sourced value with a broken invariant should degrade the *specific*
  feature it configures (auto-compaction), not the entire reply path. Preferences
  written through the ACP API are a different boundary and should still be
  rejected at write time.

Failure mechanism:
- Today's commit introduced a single shared `validate_compaction_settings` (good —
  this is the correct fix for what would otherwise be an INV-style validation-logic
  duplication between `check_if_compaction_needed`, `auto_compact_reduction_budget`,
  and the CLI/ACP write-time checks) but wired it into the *read* path with `?`
  instead of a graceful degrade, losing the previous fail-safe behavior.

Break-it angle:
- Set `GOSLING_AUTO_COMPACT_THRESHOLD=5` (a plausible typo for `0.5`) and call
  `reply()`: every subsequent turn now errors out of `reply()`'s setup, for the
  life of the process, with no user-facing recovery path other than editing the
  config and restarting.

Impact:
- A single malformed environment/config value makes the agent completely unusable
  (not just "auto-compaction unusable") until the operator finds and fixes the
  config and restarts the process. Silent, high blast radius, easily reachable by
  an ordinary typo.

Operational impact:
- Blast radius: Service (every reply on the process). Side-effect class: none
  (read-only misconfiguration). Reversibility: reversible (fix config + restart).
  Operator visibility: silent until the first `reply()` call fails. Rerun safety: unsafe (every retry fails identically until config is fixed).

Adjacent failure modes:
- The same unconditional-validate-then-`?` shape exists in
  `auto_compact_reduction_budget`, but that function is only reached *after*
  `check_if_compaction_needed` returns `true` at both real call sites — with this
  fix, an invalid threshold now short-circuits before reaching it, so it was left
  unchanged (see Non-goals).

Recommended mitigation (applied):
- In `check_if_compaction_needed`, catch `validate_compaction_settings`'s `Err`,
  log a one-time warning, and return `Ok(false)` — the same fail-safe shape the
  pre-`a48108750` code used, but built on the new shared validator instead of a
  bespoke re-implementation.

Implementation assessment:
- Complexity: local_guardrail. Cost: S. Cost drivers: tests. Nominal agent: claude
  (already applied). Rationale: single-function control-flow change plus a
  reply()-level regression test; no schema/DTO/cross-crate surface.

### Repair

`crates/gosling/src/context_mgmt/mod.rs:518-529` (`check_if_compaction_needed`):
replaced the unconditional `validate_compaction_settings(threshold, 0.0)?;` with an
`if let Err(error) = ...` branch that logs a one-time `tracing::warn!` and returns
`Ok(false)`, matching the pre-`a48108750` fail-safe shape. The ACP-preferences
write-time validation in `crates/gosling/src/acp/server/config.rs` was left
untouched — it should keep rejecting bad values at write time; this fix is only
about the read path not turning a stale bad value into a permanent outage.

Unit test updated: `test_check_if_compaction_needed_returns_false_when_disabled`
(renamed `..._returns_false_when_disabled_or_invalid`) now asserts `Ok(false)` for
`[0.0, 1.0, 1.5, -0.1, f64::NAN]` instead of asserting `.is_err()` for the invalid
subset.

New integration-level regression test:
`crates/gosling/tests/compaction.rs::reply_succeeds_when_auto_compact_threshold_env_is_invalid`
— sets `GOSLING_AUTO_COMPACT_THRESHOLD=5` via `env_lock::lock_env` (not the ACP
preferences API), drives a full `agent.reply()` call, and asserts a real assistant
text response still arrives instead of `reply()` erroring.

`crates/gosling-cli/src/cli.rs:39-98` (`warn_about_invalid_config_values`) was
reviewed but **not** changed: on closer reading its two branches are correctly
differentiated — the "parseable but out of range" branch (the actual scenario this
finding is about) already prints correct, non-misleading guidance ("Use 0 to
disable auto-compaction or a value greater than 0 and less than 1"); only the
separate "genuinely unparseable string" branch says "Falling back to the default",
and that phrasing is accurate there since every consumer of
`config.get_param::<f64>(...)` does apply `.unwrap_or(DEFAULT)` per call. The
sub-agent's characterization of this as part of the same defect did not hold up
under closer reading; no change was needed.

### Non-goals
- `auto_compact_reduction_budget`'s own `validate_compaction_settings(...)?` was
  left unconditional/strict. Both of its real call sites (`reply_entry.rs:334`,
  `reply_stream.rs:214`) are gated behind `needs_auto_compact`, which after this
  fix is only ever `true` when the threshold is valid — so the invalid-threshold
  scenario can no longer reach it via either production caller. Making it lenient
  too would be unbudgeted scope for no reachable benefit.

### Validation
- `cargo test -p gosling --lib context_mgmt::` — 92 passed, including the renamed
  unit test.
- `cargo test -p gosling --test compaction` — 11 passed, including the new
  `reply_succeeds_when_auto_compact_threshold_env_is_invalid`.
- `cargo build -p gosling`, `cargo clippy -p gosling -p gosling-cli --lib --tests
  --locked -- -D warnings`, `cargo fmt --check -p gosling -p gosling-cli` — all
  clean (see consolidated validation section for exact runs and full-suite results).

---

### NEG-CROSS-002: Temp-scratch exemption is host-wide, not session-scoped

Severity: Medium
Confidence: Plausible
Evidence basis: source-evidenced
Domain: Negative-Space (NEG-004 cross-boundary composition / NEG-002 hidden actor)

Evidence:
- `crates/gosling/src/permission/working_dir_scope_inspector.rs:50-57`: when a
  session has `restrict_tools_to_working_dirs = false`, `temporary_scratch_dirs()`
  (host `/tmp`, `/var/tmp`, `$TMPDIR`) is added to the allowed-directory set with no
  session-private subdirectory scoping.

Observed behavior:
- Two sessions with disjoint `working_dir`s but both `restrict_tools_to_working_dirs
  = false` share the *same* exempted temp roots, so a tool call in session A could
  read/write a path under `/tmp` that session B also considers "in scope" without
  either session's working-dir-scope inspector raising an approval prompt.

Expected boundary / Failure mechanism:
- The scratch exemption is host-wide rather than session-scoped, so it doesn't
  compose safely with "two independent sessions on the same host" the way the
  primary `working_dir`/`additional_working_dirs` scoping does.

Why this is not repaired here:
- This borders the security/trust-boundary domain that the operator explicitly
  scoped *out* of this pass. It also changes an intentional, recently-shipped design
  decision (today's commit `9a782c195`, "Expand working dir scope inspector...",
  and the memory note on shell-scope false positives) rather than a defect in
  today's changes proper — narrowing it needs a human-owner call on whether
  cross-session temp collisions are an accepted risk or need session-private scratch
  subdirectories, not a unilateral agent patch. Per the task's explicit scope
  reminder, this is recorded as a flagged follow-up for the operator/security lens,
  not fixed.

Recommended follow-up (not applied):
- If accepted as real risk: scope the exemption to a session-private subdirectory
  under the host temp root (created per-session) rather than the whole shared temp
  tree, and add a cross-session collision regression test.

---

### NEG-CROSS-003: `copyArtifactContents`'s concurrent-modification guard is coarse-mtime-only

Severity: Low
Confidence: Plausible → repaired as a defense-in-depth strengthening
Evidence basis: source-evidenced
Domain: Negative-Space (NEG-006 rare timing window)

Evidence:
- `ui/desktop/src/main/fileIpc.ts` (`copyArtifactContents` handler, pre-fix): the
  post-read integrity check compared only `size` and `mtimeMs` between the initial
  and final `handle.stat()` calls, which can miss a same-size, different-content
  overwrite that lands within one mtime tick on a coarse-clock filesystem.

Investigation note (corrects the original research finding): the sub-agent's
proposed fix — reusing the sibling `trashArtifactFiles` handler's `dev`/`ino`
inode comparison (`fileIpc.ts:570-575`) — does not actually strengthen this check.
`trashArtifactFiles` compares `dev`/`ino` across **two independent path-based
`lstat()` calls** (pre- and post-authorization) to catch a symlink-swap TOCTOU;
`copyArtifactContents`'s two stats are both **fd-based** (`handle.stat()`) on the
*same* open file descriptor, whose device/inode identity is fixed for the life of
the descriptor by POSIX semantics regardless of what happens to the file — so a
literal port of that check would be a no-op that could never fail, which would
have been a "fake success" repair. Applying it was declined in favor of a change
that is actually non-vacuous.

Repair applied: added `current.ctimeMs !== stats.ctimeMs` to the mismatch
condition (`ui/desktop/src/main/fileIpc.ts:373-380`). `ctimeMs` is already present
on the same `Stats` objects (no extra syscall) and updates on in-place content
writes independently of `mtimeMs`'s resolution on filesystems where the two differ
in granularity, giving a real (if not airtight) additional signal.

Honest residual-risk note: this remains a heuristic, not a cryptographic guarantee
— on a filesystem where `mtime` and `ctime` share identical coarse resolution, a
same-tick same-size overwrite is still theoretically undetectable by metadata
alone. Closing that fully would require a full second read-and-hash pass, which
was judged disproportionate cost for a Low-severity, narrow-window finding with no
existing reproduction; this is recorded as a residual risk, not silently declared
solved.

### Validation
- `pnpm --dir ui/desktop run typecheck` — clean.
- `pnpm --dir ui/desktop exec vitest run src/main/fileIpc.test.ts` — 2 passed
  (routing-level smoke test; no dedicated test exists for the concurrent-write
  race itself, before or after this change — noted as a validation limit).

---

## 2. audit-resource-lifecycle

### Surface reviewed

Budget: ~26 files / tool calls, prioritized on commits `c75f08946` (cancellation vs.
lease-loss distinction) and `a48108750` (structured errors), tracing lease/
cancellation-token ownership through `reply_entry.rs`, `reply_stream.rs`,
`tool_dispatch.rs`, `execute_commands.rs`, `context_mgmt/mod.rs`,
`output_revisions_storage.rs`, `session_manager.rs`, `acp/server/prompt_execution.rs`
/`manage_sessions.rs`, `gosling-cli/src/session/mod.rs`, and the summon delegation
call sites.

### Findings

No RES-001..019 item produced a Confirmed or Likely finding with concrete
reproduction evidence. The lens independently re-derived (rather than trusted) the
prior evening pass's own REL-TODAY-001/002 closure by walking the same lease/
cancellation-token ownership matrix from source: turn leases are acquired once per
`reply()` call via a guard bound to the async-stream's lifetime
(`let _turn_lease = turn_lease;` in `reply_entry.rs`), released on every exit path
(normal completion, cancellation, and error) because the guard's `Drop` runs
regardless of which branch the `try_stream!` macro exits through; `c75f08946`'s
child-lease-cancellation distinction and the CLI cancellation/EOF race fix were
traced end-to-end and found to close cleanly with no orphaned lease path or
un-reaped subprocess left open.

### Non-finding (Info-level, not repaired)

No regression test directly asserts the `session_turn_leases` DB row is actually
deleted after a cancellation or error — the existing tests
(`canceled_manual_compaction_does_not_replace_history`,
`canceled_auto_compaction_does_not_replace_history`) assert on stream/message
content, not on lease-table state. This is a coverage gap, not a defect: tracing
the lease-release code path from source shows the release fires on the
guard's `Drop` for every exit, matching the intended design.

Recommended (not applied — Info-level, no reachable defect to fix): add an
assertion in `crates/gosling/tests/compaction.rs`'s
`canceled_manual_compaction_does_not_replace_history` (and its auto-compact
sibling) querying `SELECT COUNT(*) FROM session_turn_leases WHERE session_id = ?`
after `stream.next().await` returns, possibly after a short yield since release is
a detached spawn. Left as a follow-up per the "only repair with concrete
reproduction evidence" rule — there is no reproduced defect here to fix, only a
gap in what's asserted.

### Validation
No repair; no test changes for this lens.

---

## 3. audit-invariant-sync

### Surface reviewed

Budget: ~18 files / tool calls: the shared `Z_INDEX` constant
(`ui/desktop/src/components/Layout/constants.ts`) and every consumer found by grep
across `ui/desktop/src`; the new artifact-copy/delete IPC constants
(`ui/desktop/src/ipc/channels.ts`, `main/fileIpc.ts`, `preload.ts`); the
`workspaceFolderRoots`/`autoCompactReduction` DTO fields traced from
`crates/gosling-sdk-types/src/custom_requests.rs` through `acp-schema.json`/
`acp-meta.json` to `ui/sdk/src/generated/{types,zod,client}.gen.ts` and their UI
consumers.

### INV-CROSS-001: `Select.tsx` hardcodes a z-index literal instead of the shared constant

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Invariant-Sync (INV-010 silent add-site gap)

Evidence:
- `ui/desktop/src/components/Layout/constants.ts:12` — `Z_INDEX.POPOVER = 9999`.
- `ui/desktop/src/components/ui/dialog.tsx:9,45,67`,
  `ui/desktop/src/components/ui/dropdown-menu.tsx:8,37,207`,
  `ui/desktop/src/components/ui/Tooltip.tsx:5,45` — each imports and uses
  `Z_INDEX.{OVERLAY,DROPDOWN_ABOVE_OVERLAY}`.
- `ui/desktop/src/components/ui/Select.tsx` (pre-fix): inline `styles.menu` object
  hardcoded `zIndex: 9999` with no import of `Z_INDEX`, coincidentally matching
  `Z_INDEX.POPOVER`'s value with nothing enforcing that they stay equal.

Ground-truth source: `Z_INDEX` in `Layout/constants.ts` is the declared shared
registry for dialog/dropdown/tooltip/popover stacking.

Divergence class: required-identical (a literal that must track the registry's
`POPOVER` value, not an intentionally-narrower copy) — this is drift, not a
legitimate divergence.

Scope note: `Layout/constants.ts` and `Select.tsx` were **not** touched by today's
commits (`git diff cb1aac7ed..HEAD -- ...constants.ts` and `...Select.tsx` are both
empty; `constants.ts` was last touched in `f150672e2`, unrelated to today). What
*is* in scope: today's commit `397ee3789` migrated `dialog.tsx`/`dropdown-menu.tsx`/
`Tooltip.tsx` onto the shared constant, which is exactly the "adjacent consumer of
a shared constant touched today" surface the task brief authorizes tracing. The
migration was partial: it widened the constant's consumer set without auditing for
other call sites still hardcoding an equivalent literal — a silent add-site gap
that the migration itself created evidence for.

Recommended mitigation (applied): import `Z_INDEX` and use `Z_INDEX.POPOVER` in the
one place (`styles.menu`, an inline style object, not a Tailwind class string) where
it can be referenced programmatically.

### Repair

`ui/desktop/src/components/ui/Select.tsx:4,50` — added
`import { Z_INDEX } from '../Layout/constants';` and changed the `styles.menu`
inline style's `zIndex: 9999` to `zIndex: Z_INDEX.POPOVER`.

Non-goal (documented, not changed): `Select.tsx:22`'s Tailwind class string
(`z-[9999]`) was left as a literal. Tailwind's JIT scanner requires a static class
string at build time; `` z-[${Z_INDEX.POPOVER}] `` would not be picked up by the
production build's content scan, so this specific occurrence cannot be
de-duplicated the same way without a different mechanism (e.g. a safelist entry or
a CSS custom property) — out of scope for a minimal fix. The inline style now wins
at runtime regardless (styles are applied after classes), so this is a residual
static-duplication note, not a functional gap.

### Validation
- `pnpm --dir ui/desktop run typecheck` — clean.
- No dedicated `Select.tsx` test exists; change is a same-value literal→constant
  substitution (9999 → `Z_INDEX.POPOVER`, which equals 9999), so it is
  behavior-preserving by construction. Verified no other test suite broke.

---

### INV-CROSS-002: `Z_INDEX.TOOLTIP`'s doc comment is false and the member has zero programmatic consumers

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Invariant-Sync (INV-004 enum/constant/UI value drift)

Evidence:
- `ui/desktop/src/components/Layout/constants.ts:9-10` (pre-fix):
  `/** Tooltips - should appear above most UI elements */ TOOLTIP: 200,` — but
  `Tooltip.tsx:45` uses `Z_INDEX.DROPDOWN_ABOVE_OVERLAY` (10001) for the tooltip
  content's actual stacking, not `TOOLTIP`. `grep -rn "Z_INDEX\.TOOLTIP"
  ui/desktop/src` returns zero matches anywhere in the tree.
- `Tooltip.tsx:55` and `bottom_menu/DirSwitcher.tsx:262` both hardcode a literal
  Tailwind `z-[200]` class (coincidentally the same number as `TOOLTIP`, for an
  *inner* element's local stacking context, not page-level tooltip stacking) — so
  `TOOLTIP`'s value isn't even the source these literals track; they're
  independent, Tailwind-arbitrary-value-constrained numbers that happen to match.

Divergence class: the comment's claim ("should appear above most UI elements") is
false relative to the actual registry (200 is below `POPOVER`/`OVERLAY`/
`DROPDOWN_ABOVE_OVERLAY`), and the member is unused. Not fixing this is exactly the
"registry member with unwritten/wrong intent" case INV-015 flags.

Recommended mitigation (applied): correct the comment to state the member's actual
(unused) status rather than delete it — deletion of a `pub`-equivalent exported
constant with no exhaustive proof of zero *external* (npm-package) consumers was
judged higher-risk than a documentation correction for a Low-severity finding;
repo-wide grep (`grep -rln "Z_INDEX" --include=*.ts --include=*.tsx` outside
`ui/desktop/src`) confirmed zero cross-package references, but keeping the
constant preserves anything relying on its mere existence while removing the false
claim.

### Repair

`ui/desktop/src/components/Layout/constants.ts:9-10` — comment changed to:
`/** Unused: tooltip content actually stacks at DROPDOWN_ABOVE_OVERLAY. Kept only
for any future consumer that needs a value between HEADER and POPOVER. */`

### Validation
- `pnpm --dir ui/desktop run typecheck` — clean (comment-only change).

---

### Non-findings (checked and held)

- `workspaceFolderRoots` and `autoCompactReduction` DTO fields: traced from Rust
  source (`crates/gosling-sdk-types/src/custom_requests.rs`) through
  `acp-schema.json`/`acp-meta.json` to `ui/sdk/src/generated/types.gen.ts`/
  `zod.gen.ts`/`client.gen.ts` and their UI consumers
  (`NavigationPanel.tsx`/`.test.tsx`, `acp/server/config.rs`). Both fields are
  consistently `Option`al end-to-end, serialize/deserialize symmetrically (no field
  present on one side and absent on the other), and an absent value means the same
  thing (no override / use default) on both the Rust producer and the TS consumer.
  No drift found; today's ARC-TODAY-002 disposition (already closed by the prior
  pass) covers the same round-trip and remains accurate.
- New artifact-copy/delete IPC constants (`desktopCommandChannels.copyArtifactContents`,
  `.trashArtifactFiles`, `.getArtifactFileTimestamps`, `.classifyArtifactRepositories`
  in `ui/desktop/src/ipc/channels.ts`): confirmed the *same* constant is imported
  and used on all three of the main-process handler registration
  (`fileIpc.ts:59-83`), the preload exposure (`preload.ts`), and the renderer call
  site (`ArtifactPane.tsx`) — not just two of three. This matches ARC-TODAY-001's
  already-closed disposition; no new drift found in this pass.
- `crates/gosling/src/config/permission.rs` / `permission/tool_class.rs` /
  `agents/agent/tool_dispatch.rs`: no hand-maintained permission-table-vs-guard
  divergence found; the tool-class enumeration used for permission decisions is the
  same one `tool_dispatch.rs` consumes, not a re-derived parallel list.

---

## 4. audit-recovery-idempotency

### Surface reviewed

Budget: ~24 files / tool calls, primary targets `context_mgmt/mod.rs` (budget-cap
auto-compaction, commits `f94d9b26d`/`687022855`), `output_revisions_storage.rs`
(revision writes), and the cancellation/lease-loss/terminal-error work in
`c75f08946`/`a48108750`.

### Recovery & Idempotency Map (material operations)

| Operation | Side-effect class | Interruption point | On-rerun behavior (pre-fix) | Idempotency class | Safe state |
|---|---|---|---|---|---|
| Compaction: replace conversation + update usage metrics | DB (2 statements, 2 transactions) | crash between the two commits | stale-high `sessions.total_tokens` after conversation already replaced | non-idempotent-unprotected (pre-fix) → naturally-idempotent (post-fix, single tx) | fail_resumable |
| Restore output revision: insert "Restored" row + rename file into place | DB + file | crash/failure between `tx.commit()` and `replacement.persist()` | DB claims a restore the file never received; a same-hash retry could insert a duplicate "Restored" row | non-idempotent-unprotected (pre-fix) → refuses to duplicate-apply (post-fix) | fail_visible |
| `finish_output_capture`: insert revision + write annotated bytes | DB + file | same commit-then-write ordering as restore | self-correcting on the *next* tool call touching the same path (always re-derives bytes from live disk content); the phantom mid-sequence DB row itself is not retroactively fixed | naturally-idempotent for *future* captures; the interrupted capture's own row stays unreconciled | fail_resumable (partial) |

### Write-path atomicity framework (compaction's `sessions` usage update)

1. **Is it atomic?** Pre-fix: two separate idioms — `replace_conversation`
   (temp+rename-free, single `BEGIN IMMEDIATE` transaction over `messages`) then a
   *separate* `record_usage` (single autocommit `UPDATE` on `sessions`). Two
   independently-committed transactions, not one.
2. **What does half look like?** `messages`/`session_summaries` reflect the
   post-compaction conversation; `sessions.total_tokens`/`accumulated_*` still
   reflect the pre-compaction usage.
3. **Who detects half?** Nothing, pre-fix. `resolve_context_usage`'s
   `max(stored, estimated)` (`context_mgmt/mod.rs:561-564`) — deliberately built to
   protect against *undercounting* (stored usage recorded before large tool outputs
   are tokenized) — picks the stale-high `stored` value in this scenario, which is
   the opposite of its intended protective direction.
4. **What repairs half?** Nothing, pre-fix, until the *next* successful compaction
   overwrites the stale value.

### FSR-CROSS-001: Compaction's conversation-replace and usage-update commit separately

Severity: Medium
Confidence: Confirmed for the missing atomicity guard; the crash-timing
manifestation itself is Likely (not reproduced via an actual kill drill — see
Evidence basis)
Evidence basis: source-evidenced (mechanism); simulation-reasoned (crash
manifestation) — no `requires-authorized-drill` claim was made Confirmed
Domain: Failsafe (REC-003 transaction boundary / REC-012 provenance consistency)

Evidence:
- `crates/gosling/src/agents/agent/reply_entry.rs:484-501` (pre-fix,
  `perform_compact_with_provider`): `session_manager.replace_conversation(...)`
  (its own `BEGIN IMMEDIATE` transaction, `message_storage.rs:521-529`) followed by
  a separate `self.update_session_metrics(...)` call
  (`reply_parts.rs:587-613` → `session_manager.record_usage`, a standalone
  autocommit `UPDATE`, `session_crud.rs:329-380`).
- Same shape reused, unguarded, in the manual `/compact` command handler:
  `crates/gosling/src/agents/execute_commands.rs:237-242` (pre-fix).
- `reply_stream.rs:184-253` (in-loop threshold compaction) and `:800-860`
  (context-limit-recovery compaction) both route through the *same*
  `perform_compact_with_provider`, so fixing that one method also fixes both of
  those call sites; they are not independent bugs.
- `context_mgmt/mod.rs:561-564`, `resolve_context_usage`'s `max(stored, estimated)`
  undercounting guard: confirmed by direct source read that a stale-high `stored`
  value (left behind by an interrupted metrics update) wins over a correctly-low
  fresh estimate of the now-compacted conversation.

Observed behavior:
- If the process crashes (or, more narrowly, if the metrics-update call fails for
  a reason unrelated to the conversation replace — though today's `a48108750`
  already surfaces *that* case gracefully via `CompactionMetricsError`) between the
  two separate commits, the conversation is durably replaced but
  `sessions.total_tokens` is not, and the stale-high value spuriously wins
  `resolve_context_usage`'s max() on the very next turn, re-triggering
  auto-compaction against a conversation that's already small.

Expected boundary:
- The two writes represent one logical operation ("compaction completed") and
  should commit or roll back together (`fail_resumable`: on failure, neither write
  lands, and the existing `compaction_failure_message` — "your original session is
  intact" — becomes accurate for 100% of failures on this path instead of only
  some of them).

Failure mechanism:
- Two independently-committed transactions with no shared atomicity boundary.

Break-it angle: kill the process between `replace_conversation`'s commit and
`update_session_metrics`'s commit (not reproduced via an actual kill — see
Evidence basis); the spurious-recompaction consequence itself was verified by
direct source read of `resolve_context_usage`, which is deterministic given a
stale stored value.

Impact: one extra, wasted, silent compaction pass on the next turn after an
already-rare crash window. Self-correcting (the next successful compaction fixes
the stale value); not data-corrupting.

Operational impact: Blast radius: Workflow (one session, one extra turn).
Side-effect class: DB. Reversibility: reversible (self-corrects next compaction).
Operator visibility: silent. Rerun safety: unsafe until self-corrected.

Resilience mapping: Phase: recover. Objective(s): reconstitute. Safe state:
fail_resumable.

Failure analysis (FMECA row): Failure mode: two-transaction split write.
Likely cause: `replace_conversation` and `record_usage` are independent public
APIs owned by different storage submodules, called sequentially. Operational
phase: recovery (post-compaction). Local effect: `sessions.total_tokens` stale.
Workflow effect: spurious re-compaction next turn. System-or-operator effect: none
visible beyond an extra "Compacting..." notification. Detection method: none
pre-fix. Detection latency: n/a (self-corrects silently). Operator visible: false.
Compensating provision: none pre-fix.

Criticality: Likelihood: unlikely (requires a crash in a narrow window).
Detectability: silent.

Recommended mitigation (applied): fold both writes into one `BEGIN IMMEDIATE`
transaction.

Implementation assessment: Complexity: persistence_recovery. Cost: M. Cost
drivers: modules (3 storage files + 2 call sites), tests. Nominal agent: claude
(already applied). Rationale: cross-module transactional refactor with a clear,
bounded blast radius (compaction call sites only) and full regression coverage
available in the existing suite.

### Repair

New atomic storage primitive:
- `crates/gosling/src/session/session_manager/session_crud.rs:392-449`
  (`record_usage_in_tx`): same UPDATE statement as `record_usage`, executed against
  an existing `&mut Transaction` instead of `pool` directly. `record_usage` itself
  was deliberately left unchanged (still a standalone autocommit UPDATE with no
  write-guard) to avoid an unrelated locking-behavior change to an already-tested,
  unguarded path; the SQL is duplicated with a cross-reference comment rather than
  refactored to share one statement, an explicit, documented trade-off favoring
  zero risk to the existing `record_usage` over eliminating a small SQL
  duplication.
- `crates/gosling/src/session/session_manager/message_storage.rs:538-558`
  (`replace_conversation_and_record_usage`): opens one write-guard + one
  `BEGIN IMMEDIATE` transaction, calls `replace_conversation_in_tx` then
  `record_usage_in_tx`, commits once.
- `crates/gosling/src/session/session_manager.rs:872-890`: public delegator on
  `SessionManager`.
- `crates/gosling/src/agents/reply_parts.rs:619-648`
  (`Agent::replace_conversation_and_update_metrics`): mirrors
  `update_session_metrics`'s `is_compaction_usage: true` branch (compaction usage
  always maps output tokens to the new input context) and calls the new atomic
  storage method instead of two separate calls.

Call sites updated:
- `crates/gosling/src/agents/agent/reply_entry.rs:485-503`
  (`perform_compact_with_provider`): the `compacted_context: true` branch (a
  compacted-resume session, where durable history is deliberately *not* replaced)
  still calls the original `update_session_metrics` — there is nothing to pair it
  with atomically in that branch. The `compacted_context: false` branch now calls
  `replace_conversation_and_update_metrics`.
- `crates/gosling/src/agents/execute_commands.rs:237-243` (manual `/compact`
  handler): same fix applied for internal consistency — this is the sibling
  implementation of the identical mechanism (same-shaped "replace conversation,
  then separately record usage" pattern), not a new/different surface, so leaving
  it unfixed would have reintroduced the exact defect asymmetrically between the
  auto-compact and manual-compact paths.

**Documented side effect** (transparency, not a hidden regression): today's
`a48108750` added `CompactionMetricsError` specifically to distinguish "compaction
was saved, but usage metrics failed separately" from "nothing was saved" in the
`!compacted_context` branch's error message. With this fix, that branch is now
atomic — if it errors, *neither* write landed, so that distinction can no longer
occur for this call site, and the plain `compaction_failure_message` ("your
original session is intact") is correct for 100% of its failures. The
`CompactionMetricsError` type, its `auto_compaction_failure_message` special-case,
and its own unit test (`compaction_failure_distinguishes_a_committed_replacement`)
were left in place unmodified — this repair's scope is FSR-CROSS-001, not a
relitigation of the prior pass's REL-TODAY-001 disposition — but its one
production construction site is gone, so in practice it is presently unreachable
in production (still compiles cleanly; `cargo clippy --lib` and `--lib --tests`
both pass with zero warnings, confirmed explicitly rather than assumed). This is
noted here so it isn't mistaken for silently-working functionality; a future pass
may want to simplify it away, but that's out of this repair's scope.

New regression test: `crates/gosling/src/session/session_manager.rs` (test module)
`replace_conversation_and_record_usage_applies_both_writes_together` — asserts (a)
a single successful call updates both the stored conversation and the usage/cost
fields together, and (b) a failing call (nonexistent session) leaves *zero* stray
messages behind (rollback, not a torn insert).

### Validation
- `cargo test -p gosling --lib session_manager::tests::replace_conversation_and_record_usage_applies_both_writes_together` — passed.
- `cargo test -p gosling --test compaction` — 11 passed (all pre-existing tests
  unchanged in assertion; `failed_compaction_publishes_terminal_error_for_manual_and_automatic_paths`
  specifically re-verified the terminal-error signaling from REL-TODAY-001's
  disposition still holds after this change).
- `cargo test -p gosling --lib` (full) — 1885 passed, 3 ignored (baseline
  unchanged).
- `cargo build -p gosling`, `cargo clippy -p gosling --lib` and `--lib --tests
  --locked -- -D warnings`, `cargo fmt --check` — all clean.

---

### FSR-CROSS-002: `restore_output_revision` commits its DB row before the file replacement lands

Severity: Medium
Confidence: Confirmed for the missing guard; the crash-timing manifestation itself
is Likely (the design already minimizes the window to a single `rename()` syscall
after a pre-fsynced temp file — see below)
Evidence basis: source-evidenced (mechanism); test-reproduced (the detection/
refusal behavior itself, via a constructed fixture, not a real crash)
Domain: Failsafe (REC-001 write-path atomicity / REC-012 provenance consistency)

Evidence:
- `crates/gosling/src/session/session_manager/output_revisions_storage.rs:573-582`
  (pre-fix): `insert_revision(&mut tx, &path, &next, &bytes).await?;` →
  `tx.commit().await?;` → *then* `replacement.persist()` (the atomic rename onto
  the live path).
- `crates/gosling/src/session/output_revisions.rs:318-342` (`prepare_replacement`,
  unchanged): the design deliberately stages and `fsync`s the new bytes into a
  temp file *before* the DB commits ("Stage and sync bytes without touching the
  live file, so SQLite can commit first" — an explicit, already-reasoned-about
  trade-off, not an oversight), so the residual crash window is narrowed to a
  single `rename()` syscall after everything else durable is already in place —
  this repair narrows further what was already a deliberately-minimized risk, it
  does not fix an unconsidered one.
- A **graceful** `persist()` failure (as opposed to a crash) is already correctly
  handled: `output_revisions_storage.rs:580-581` wraps the error with "Restore
  snapshots saved, but file replacement failed; refresh history before retrying" —
  this repair does not touch that path.
- Investigation correction: the sibling `finish_output_capture` path
  (`output_revisions_storage.rs:267-390`) has the *same* commit-then-write
  ordering, not a genuine "self-heal" as originally characterized by the research
  pass. Its resilience comes from a different property: every call always
  re-derives what to write from the *currently observed* disk content
  (`after.body`), so a phantom DB-only revision left by an interrupted prior write
  doesn't stop the *next* capture from correctly re-syncing going forward — but if
  nothing subsequently touches that path, the phantom row itself is never
  reconciled, same as restore's.

Observed behavior (constructed, not from a real crash): if the DB commit lands but
the file replacement never reaches disk, `output_revisions` records a "Restored"
row whose content the live file never received. A caller unaware of the crash
(their original request having also died with the process) could retry with the
*same* `expected_current_hash` — since the file genuinely never changed, that
retry's `current.hash == request.expected_current_hash` check would pass — and
succeed, but insert a **second**, duplicate "Restored" row on top of the phantom
one; and any `list_output_revisions` call in between would show a revision the
file never actually held.

Distinguishing this from a legitimate case (must not break): a mismatch between
`digest(current.body)` and the latest revision's `content_hash` is *also* the
normal, expected signature of a genuine external edit (someone hand-edited the
file after the last capture) — which `restore_output_revision`'s existing
baseline-insertion logic (`:533-546`) already handles correctly and which two
existing tests (`failed_restore_does_not_commit_the_external_edit_baseline`,
`restore_commit_failure_leaves_live_bytes_unchanged`) protect. A blanket
"mismatch → refuse" guard would have broken that legitimate flow.

Failure mechanism / precise signature used: a *torn write* is distinguishable from
an *external edit* because in the torn-write case, the live file's content exactly
matches an **earlier** (non-latest) revision already in history — nothing new was
written at all — whereas a genuine external edit produces content matching *no*
prior revision. The repair checks specifically for "current file matches an
earlier revision, while a newer one is already recorded as latest", not a bare
hash mismatch.

Break-it angle: constructed a fixture where a second write's DB row (version 2) is
committed but the file is reverted to version 1's exact bytes (simulating the
crash window) and confirmed the new check fires; confirmed the *external-edit*
case (file content matching no revision) still falls through unchanged to the
existing baseline-insertion path.

Impact: without the fix, a duplicate "Restored" audit-trail row and a transient
window where `list_output_revisions` shows a revision the file never received.
Not data-corrupting (the underlying restore, when retried, lands correctly); a
audit-trail/display consistency issue.

Operational impact: Blast radius: Workflow (one output path). Side-effect class:
DB (extra row) / user-visible (history display). Reversibility: reversible (no
data loss). Operator visibility: silent until a user reads history. Rerun safety:
unsafe pre-fix (duplicate row on retry); refused pre-repair, and now surfaced
explicitly instead of silently duplicated.

Resilience mapping: Phase: recover. Objective(s): reconstitute, understand. Safe
state: fail_visible.

Failure analysis (FMECA row): Failure mode: DB-commit-before-file-write ordering
with no reconciliation. Likely cause: deliberate "stage+fsync before commit"
design left the final rename as the sole post-commit step, with no read-time
check. Operational phase: recovery (restore retry). Local effect: phantom DB row.
Workflow effect: possible duplicate row on retry. System-or-operator effect:
misleading revision history display in the interim. Detection method: none
pre-fix. Detection latency: until next restore attempt or manual history review.
Operator visible: false pre-fix. Compensating provision: none pre-fix.

Criticality: Likelihood: unlikely (single-syscall crash window, already
minimized by design). Detectability: silent pre-fix.

Recommended mitigation (applied): detect the torn-write signature at the top of
`restore_output_revision`'s history read and refuse with an explicit, actionable
error rather than silently proceeding.

Implementation assessment: Complexity: local_guardrail. Cost: S. Cost drivers:
tests. Nominal agent: claude (already applied). Rationale: single-function,
read-then-compare addition with no schema change and full regression coverage
available.

### Repair

`crates/gosling/src/session/session_manager/output_revisions_storage.rs:524-556`:
after fetching `history` inside the existing write transaction, added a check —
if the latest revision's `content_hash` doesn't match the live file's current
digest, *and* some **earlier** (non-latest) revision's `content_hash` *does* match
it — `anyhow::bail!` with `OutputRevisionError::Conflict("A previous write to this
output was recorded but never reached the file (interrupted mid-save); refresh its
history before restoring again")`. Placed before the existing baseline-insertion
`if` so a genuine external edit (which matches no revision) still falls through to
that unchanged existing path.

New regression test: `crates/gosling/tests/output_revisions_test.rs::restore_refuses_when_latest_revision_was_never_applied_to_the_file`
— writes two revisions, then reverts the live file to the *first* revision's exact
bytes (simulating a crashed second write), and asserts: the next restore attempt
is refused with the "interrupted mid-save" message, history length stays at 2 (no
duplicate/extra row inserted), and the file is left byte-for-byte untouched.

Both existing tests required to keep passing unmodified
(`failed_restore_does_not_commit_the_external_edit_baseline`,
`restore_commit_failure_leaves_live_bytes_unchanged`) were re-run and pass with no
source changes.

### Non-goals
- No self-heal/auto-repair of the phantom row or an automatic retry of the file
  write was implemented — only detection-and-refusal. An auto-heal was considered
  and rejected as disproportionate new complexity/risk for a Low-likelihood,
  non-corrupting finding; a human/UI-driven retry (which now works correctly, since
  a fresh `expected_current_hash` read via `get_output_revision` reflects the true
  live-file state) is the intended recovery path.
- `get_output_revision`/`list_output_revisions` were not changed to add
  reconciliation of their own — the write-path check (which runs on every restore
  attempt, the only mutating entry point) was judged sufficient without a
  DTO/schema change that would have rippled into `gosling-sdk-types`,
  `acp-schema.json`, and the generated TS/Zod types for a read-only surface.

### Validation
- `cargo test -p gosling --test output_revisions_test` — 29 passed (28 pre-existing
  + 1 new), including the two tests this repair was required not to disturb.
- `cargo test -p gosling --lib` (full) — 1885 passed, 3 ignored.
- `cargo build -p gosling`, `cargo clippy -p gosling --lib --tests --locked -- -D
  warnings`, `cargo fmt --check` — all clean.

---

## Consolidated validation (all repairs, run together)

- `cargo build -p gosling` — clean.
- `cargo test -p gosling --lib --test compaction --test output_revisions_test` —
  1885 lib tests passed (3 ignored, unchanged baseline), 11 compaction tests
  passed, 29 output-revision tests passed. Zero failures.
- `cargo clippy -p gosling -p gosling-cli --lib --tests --locked -- -D warnings` —
  clean, zero warnings (explicitly re-checked both with and without `--tests` to
  confirm the `CompactionMetricsError` side effect noted under FSR-CROSS-001
  doesn't trigger a dead-code warning in either the lib-only or lib+tests target).
- `cargo fmt --check -p gosling -p gosling-cli` — clean (after running `cargo fmt`
  once to apply formatting to the new code).
- `pnpm --dir ui/desktop run typecheck` — clean. (Required activating a newer
  `pnpm` via the bundled `corepack` binary — the ambient `pnpm 10.6.4` did not
  satisfy this repo's `engines.pnpm >= 10.30.0`; `corepack prepare pnpm@10.30.0
  --activate` resolved it, then `pnpm --dir ui/desktop install --frozen-lockfile`
  populated `node_modules`, which was not present at the start of this session.)
- `pnpm --dir ui/desktop exec vitest run src/components/ui/ConfirmationModal.test.tsx
  src/main/fileIpc.test.ts` — 6 passed, 0 failed.

Not run in this pass (validation limits, stated per the audit method rather than
silently skipped):
- Full `pnpm --dir ui/desktop exec vitest run` (whole suite) was not re-run; only
  the suites directly touching changed files were run, since the changes are
  narrow, typecheck is clean, and the concurrent playtest session was also
  actively using this checkout (see Constraints below).
- No native Electron, packaged-install, or live-provider verification was
  performed.
- The crash/kill-timing manifestations underlying NEG-CROSS-001 (pre-fix),
  FSR-CROSS-001, and FSR-CROSS-002 were not reproduced via an actual process kill;
  each was verified via direct source reasoning plus a constructed
  fixture/regression test that reproduces the *resulting state* a crash would
  leave, not the crash itself (`requires-authorized-drill` for the kill itself,
  per `evidence_discipline.md`).

## Constraints observed

Per the operator's instructions, this session did not touch `docs/test_scenarios/`
or write playtest reports — a concurrent session's changes to those files and a
new `docs/cloud/2026-09-08-desktop-surfaces-playtest.md` were observed mid-session
(`git status`) and left untouched as contention, not treated as a bug or reverted.

No commit, merge, or push was performed. Final `git status` is recorded in the
companion session log.
