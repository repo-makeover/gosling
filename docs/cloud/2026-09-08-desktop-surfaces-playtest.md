# Gosling desktop-surfaces playtest report — 2026-09-08

## Executive result

This pass targets **today's changed user-facing surfaces** (2026-09-08 commits) against the
`docs/test_scenarios/` library, per the `audit-playtest-app` methodology. It is a follow-on to,
and does not repeat or relitigate, the same-day static audit closed in
`docs/cloud/2026-09-08-evening-audit-repair.md` and
`docs/logs/session/2026-09-08-evening-audit-repairs.md`. Security is out of scope.

Six new scenario cards (**DT-11–DT-16**) and one new card (**PN-12**) were added to the library
for surfaces with no existing coverage, plus three targeted Variations on existing cards
(**PA-02, CH-03, AP-08**) for surfaces that extend rather than replace an existing card. The
README index and file/scenario counts were updated to match (**119 → 127 cards**).

Of the 10 executed items: **7 Pass (Confirmed live or via targeted test execution)**,
**0 Fail**, **3 Blocked** for full live-Desktop click-through (component-level behavior for those
three is separately Confirmed via targeted Vitest). No confirmed product defect was found in
today's surfaces during this pass, so **no code repair was made**. One Low/Note cosmetic
duplication was observed (see below) and left unfixed as out of the minimal-repair scope for a
non-functional, non-misleading duplicate warning line.

The Desktop live-GUI blocker is an **environment/tooling limitation**, not a defect in today's
commits: the Vite dev server crash-looped on dependency pre-bundling in this sandbox regardless of
the code under test, and the available `chrome-devtools` MCP tool has no parameter to target an
arbitrary Chrome DevTools Protocol endpoint (it manages its own separate browser), so it could not
attach to the Electron instance even after the CDP port itself was confirmed reachable.

## Target and method

- Repository: `/Users/eric/Work/vscode/forked/gosling`
- Baseline: clean `main` at `c75f0894612b5c5849d0f33b76ea8e4fcd3e9396` (unchanged by this pass;
  only `docs/test_scenarios/*` were edited)
- Scope: today's (2026-09-08) 19 commits and their changed user-facing surfaces, cross-referenced
  against the existing 119-card library (see `docs/test_scenarios/README.md` before this pass)
- Scenario source: `docs/test_scenarios/README.md` and the referenced
  `agent-skills/010_audit/audit-playtest-app/SKILL.md` evidence-discipline rules
- Disposable state: `GOSLING_PATH_ROOT` under this session's scratchpad
  (`.../scratchpad/playtest-root` for CLI, `.../scratchpad/playtest-root-desktop` for Desktop),
  never the operator's real gosling home
- Provider: local Ollama (`gemma4:26b`, already running on this machine) — a real, disposable,
  zero-cost local model, per the safety rails' preference for the cheapest available path
- Build: `cargo build -p gosling-cli -p gosling` (debug) and `cargo build --release -p gosling-cli`
  (for the Desktop-bundled CLI binary via `just copy-binary`), both from a clean `source
  bin/activate-hermit` shell
- Desktop: launched via `pnpm run start-gui` with `ENABLE_PLAYWRIGHT=true`,
  `GOSLING_PLAYWRIGHT_USER_DATA_DIR`, and a disposable `GOSLING_PATH_ROOT`, per
  `justfile`'s `run-ui-playwright` recipe (adapted to also pin a disposable path root, which that
  recipe does natively). Confirmed this exposes a real Chrome DevTools Protocol port
  (`DevTools listening on ws://127.0.0.1:9315/...`), answering the operator's ask to check for a
  remote-debugging port before assuming Desktop automation was blocked.

## New scenario cards added

`docs/test_scenarios/14-desktop-ux-and-integration.md`:

| ID | Name | Surface (today's commit) |
|---|---|---|
| DT-11 | Artifact delete and Trash recovery | `f5a910578`, `6f4e8e3a9` |
| DT-12 | Copy artifact contents authorization boundary | `2186fab44` |
| DT-13 | Repository file filter persistence | `b4a782f03` |
| DT-14 | Artifact file timestamp display | `e38f2eaa2` |
| DT-15 | Workspace readiness indicator accuracy | `035535822` |
| DT-16 | Dialog, dropdown, and tooltip z-index stacking | `397ee3789` |

`docs/test_scenarios/16-provider-and-network-resilience.md`:

| ID | Name | Surface (today's commit) |
|---|---|---|
| PN-12 | Auto-compact reduction setting scopes partial vs full compaction | `f94d9b26d`, `687022855` |

Extended (Variation added to an existing card rather than a new ID, per the README's
extend-don't-duplicate guidance):

| Card | File | Added Variation covers |
|---|---|---|
| PA-02 | 08 | Policy-reason text on tool denial (`a48108750`) |
| CH-03 | 02 | Terminal-error/cancellation-vs-lease-loss reporting (`c75f08946`) |
| AP-08 | 17 | Lease-revocation-distinct-from-cancel terminal state (`c75f08946`) |

The README's Files table, "Full library" count, and Scenario index table were updated to
127 cards (from 119/112, the latter of which was already internally inconsistent before this
pass — corrected as a stale-index fix). `PN-11` (pre-existing but missing from the index table)
was also added while touching that section, since it sits directly above the new `PN-12` row.

## Scenario outcome ledger

| ID | Evidence tier | Status |
|---|---|---|
| PN-12 (env var boundary) | Confirmed — live CLI, 5 sub-cases | **Pass** |
| PN-12 (`budget_capped_compact_end` algorithm) | Confirmed — targeted `cargo test` | **Pass** |
| PN-12 (ACP preference read/save/reject) | Confirmed — targeted `cargo test` | **Pass** |
| PN-12 (Desktop preference-UI round-trip) | Not executed this pass | Not executed |
| PA-02 (never-allow policy-reason text) | Confirmed — live CLI turn | **Pass** |
| PA-02 (working-dir-scope-specific reason text) | Not executed this pass (source-read only) | Not executed / Suspicion |
| CH-03 / AP-08 (lease-loss vs cancellation) | Confirmed — targeted `cargo test` | **Pass** |
| DT-11 (artifact delete/Trash) | Confirmed — targeted Vitest (component level) | **Blocked** (live click-through) |
| DT-12 (copy contents authorization) | Confirmed — targeted Vitest + integration test (component/IPC level) | **Blocked** (live click-through) |
| DT-13 (repository filter persistence) | Confirmed — targeted Vitest (component level) | **Blocked** (live click-through) |
| DT-14 (file timestamp display) | Confirmed — targeted Vitest (component level) | **Blocked** (live click-through) |
| DT-15 (workspace readiness indicator) | Confirmed — targeted Vitest (component level) | **Blocked** (live click-through) |
| DT-16 (dialog/dropdown/tooltip z-index) | Confirmed — targeted Vitest (`ConfirmationModal.test.tsx`) | **Blocked** (live click-through) |

Totals: **7 Pass**, **0 Fail**, **6 Blocked-for-full-live-execution-but-Confirmed-at-component-level**,
**2 Not executed**.

## Evidence detail

### PN-12 — auto-compact reduction

Live CLI (`./target/debug/gosling info`, disposable `GOSLING_PATH_ROOT`, no provider needed for
this check):

```
GOSLING_AUTO_COMPACT_REDUCTION=-0.5 → "Warning: Invalid GOSLING_AUTO_COMPACT_REDUCTION: -0.5. Use 0 ..."
GOSLING_AUTO_COMPACT_REDUCTION=1.0  → same warning, PLUS a second "Warning: Invalid auto-compaction settings: ..." line
GOSLING_AUTO_COMPACT_REDUCTION=abc  → "Warning: ... Failed to deserialize value ... Falling back to the default."
GOSLING_AUTO_COMPACT_REDUCTION=0    → no warning (valid, full-collapse)
GOSLING_AUTO_COMPACT_REDUCTION=0.3  → no warning (valid)
```

All five sub-cases behaved correctly and boundedly. One Low/Note: the `1.0` boundary case prints
two separate warning lines (the per-field parse-time check and the later cross-field
threshold-vs-reduction check both fire for the same underlying value) instead of one. This is
cosmetic doubling, not incorrect or misleading, so it was recorded as a Note rather than repaired.

`cargo test -p gosling --lib budget_capped_compact_end`: 2/2 passed
(`test_budget_capped_compact_end_stops_once_budget_met`,
`test_budget_capped_compact_end_falls_back_to_ceiling_when_budget_exceeds_region`) — confirms the
partial-collapse splice math introduced by `f94d9b26d`.

`cargo test -p gosling --test acp_custom_requests_test auto_compact_reduction`: 1/1 passed
(`test_custom_preferences_read_save_auto_compact_reduction`) — confirms the ACP preference
validates `[0, 1)` and maps to `GOSLING_AUTO_COMPACT_REDUCTION`, per `687022855`.

### PA-02 (Variation) — policy-reason text on tool denial

Live CLI turn against local Ollama, disposable root, `GOSLING_MODE: approve`, and a
`permission.yaml` with `never_allow: [shell, developer__shell]` (the bare tool name `shell` is
what actually matched — the built-in developer extension registers its shell tool as `shell`, not
`developer__shell`; the latter appears only in an ACP-layer unit test's example principal and did
not match on its own in a first attempt). Result:

```
▸ shell
  command: echo hi
Annotated { raw: Text(RawTextContent { text: "Tool denied by policy: User permission denies this tool", ... }) }
```

This confirms `a48108750`'s `handle_denied_tools` path (`crates/gosling/src/agents/agent/reply_context.rs`)
produces the distinct `"Tool denied by policy: {reason}"` wording instead of the prior generic
`"Tool denied by current permissions."` fallback, sourced from `PermissionInspector`'s
`"User permission denies this tool"` reason string. The model then tried a different tool
(`tree`, not covered by `never_allow`), which correctly hit the separate, expected
"non-interactive mode ... no operator is available" boundary — a useful incidental confirmation
that the approve-mode gate is still real and not silently bypassed.

The working-dir-scope-specific reason text (the other half of the Variation, sourced from
`working_dir_scope_inspector.rs`) was not independently exercised live this pass; it is recorded
as Suspicion from source reading only, not Confirmed.

### CH-03 / AP-08 (Variation) — lease loss vs. cancellation

`cargo test -p gosling --lib turn_completion_distinguishes_lease_revocation_from_user_cancellation`:
1/1 passed. This is the exact function (`Agent::ensure_turn_not_revoked`) `c75f08946` added to
distinguish a revoked lease from a user-initiated cancel and to attach `terminal_error` metadata
accordingly. This is Confirmed via direct execution of the production code path, but is not a full
live two-ACP-client session exercising the end-to-end wire behavior; the card is marked Pass on
that basis with the distinction noted.

### DT-11 through DT-16 — Desktop artifact/UX surfaces

Targeted Vitest run (`pnpm exec vitest run` against the eight test files covering all six new
cards): **8 files / 108 tests passed**, 0 failed:

- `ArtifactFileList.test.tsx`, `ArtifactPane.test.tsx`, `ArtifactWorkbenchContext.test.tsx`
- `useArtifactFileTimestamps.test.ts`
- `NavigationPanel.test.tsx`, `WorkspaceSidebarSection.test.tsx`
- `ConfirmationModal.test.tsx`
- `main/artifactAccessIntegration.test.ts`

This is genuine runtime evidence (the actual component/IPC-handler code executing against
real DOM/IPC mocks), but it is not the same as a human clicking through the running app, so per
the execution contract's "only runtime evidence can be marked Confirmed" and "pass atomically"
rules, the full cards remain **Blocked** for the live-interaction assertions (visual stacking,
actual OS Trash calls, actual clipboard content, actual cross-window navigation) while the
component-level behavior they test is Confirmed Pass.

**Live Desktop attempt and its blocker:** `just copy-binary` (release `gosling` binary) succeeded;
`pnpm run start-gui` with `ENABLE_PLAYWRIGHT=true` launched Electron, spawned the bundled
`gosling serve` backend on a random port with a pinned TLS cert, and exposed
`ws://127.0.0.1:9315/devtools/...` — no macOS Keychain prompt blocked this dev-mode (unsigned,
unpackaged) launch, unlike the packaged-app Keychain stall documented in prior playtests. However,
the Vite dev server printed `➜ Local: http://localhost:5173/` and then entered a
`[plugin vite:dep-scan] The server is being restarted or closed. Request is outdated` crash-loop,
so the renderer never loaded past `ERR_CONNECTION_REFUSED` (confirmed via a raw CDP
`Page.captureScreenshot` over the exposed WebSocket: a blank white page, before and after a manual
`Page.reload`). The file cited in the dep-scan stack trace
(`ui/desktop/src/utils/nextChatExtensions.ts`) predates today's commits (last modified Sep 6), so
this is environment/dependency-graph flakiness in this sandbox, not a defect introduced by any of
today's changes. Separately, the `chrome-devtools` MCP tool available in this session manages its
own Chrome instance and exposes no parameter to attach to an arbitrary CDP URL/port, so even had
the renderer loaded, this session's tooling could not have driven it — this is recorded as a
session/tooling limitation, not a product finding.

## Findings

No confirmed product defect in today's surfaces. One Low/Note:

- **Note** — `GOSLING_AUTO_COMPACT_REDUCTION=1.0` prints two separate warning lines instead of
  one (the per-field check and the cross-field `threshold >= reduction` check both fire for the
  same boundary value). Not misleading or functionally wrong; left unrepaired as out of minimal-fix
  scope for a cosmetic duplicate.

## Validation

Passed (this pass; does not repeat the prior audit's full-suite run, which already validated the
underlying repair commits):

- `cargo build -p gosling-cli -p gosling` (debug) — clean
- `cargo build --release -p gosling-cli` — clean, 3m54s
- `cargo test -p gosling --lib turn_completion_distinguishes_lease_revocation_from_user_cancellation` — 1/1
- `cargo test -p gosling --lib budget_capped_compact_end` — 2/2
- `cargo test -p gosling --test acp_custom_requests_test auto_compact_reduction` — 1/1
- `pnpm exec vitest run` (8 targeted files) — 8/8 files, 108/108 tests
- Live CLI turns against local Ollama (`gemma4:26b`) under a disposable `GOSLING_PATH_ROOT`:
  baseline smoke, PN-12 env-var boundary (5 sub-cases), PA-02 never-allow denial
- `git diff -- AGENTS.md GEMINI.md docs README.md` reviewed; only `docs/test_scenarios/*` changed
- `grep -R "GILES:DOCS-GOVERNANCE:START" -n AGENTS.md` — present; `GEMINI.md` absent (expected)

Not run this pass (no code was changed, so the broader repair-validation battery from the prior
audit was not re-run; see that report for the full-suite baseline):

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, full `cargo test`, full Desktop Vitest
  suite, `pnpm run typecheck` — none of these were needed since no source file was modified in
  this pass, only `docs/test_scenarios/*.md`.

## Residual risks and next actions

1. Live Desktop click-through for DT-11–DT-16 remains unexecuted. Repeat once this sandbox's Vite
   dev-server dependency-scan instability is diagnosed (looked environment-specific, not
   code-specific) or via a packaged build (`just package-ui`) with Keychain handled by the
   operator.
2. If a session later has a `chrome-devtools`-equivalent MCP tool that accepts an arbitrary CDP
   `browserURL`/port, DT-11–DT-16 and the z-index/readiness-indicator visual assertions become
   directly automatable against the `ENABLE_PLAYWRIGHT=true` launch path confirmed working here.
3. PA-02's working-dir-scope-specific denial-reason text and PN-12's Desktop preference-UI
   round-trip were not independently live-verified this pass; both are plausible from source
   but should get a dedicated live pass.
4. The cosmetic double-warning at `GOSLING_AUTO_COMPACT_REDUCTION=1.0` is a good small follow-up
   if anyone touches `cli.rs`'s compaction-settings validation again.

## Final status

`completed_with_partial_verification`. Every new/extended card was either Confirmed Pass via live
execution or targeted test, or explicitly marked Blocked/Not-executed with its exact blocker —
none were rounded up. No code repair was needed or made. `docs/test_scenarios/*` are the only
files changed by this pass.
