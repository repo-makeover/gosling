# Gosling Playtest Scenario Library

End-to-end, user-facing test scenarios for exercising gosling like a real
operator would — through the CLI, Ink/text UI, and Electron Desktop, against
a configured local install. This library is the standing plan for exploratory
playtest passes; each pass executes these scenario cards and records results
in a separate report.

## Provenance and baseline

The methodology is derived from the `audit-playtest-app` skill
(`agent-skills` repo, `010_audit/audit-playtest-app/`): discover the app,
act like a curious/impatient/occasionally mistaken user, exercise happy
paths, invalid and boundary inputs, persistence, interruption, and recovery,
and report every issue with reproduction steps, severity, and user impact.
Organization and card density follow the Cuttlefish
`docs/test_scenarios/` pattern; **content is gosling-specific** (CLI,
Desktop, workspaces, ACP/serve, MCP extensions, skills, subagents).

Evidence discipline applies verbatim:

- **Confirmed** issues are observed by running the app. **Suspicions** are
  inferred from code or docs. Label every finding as one or the other.
- Never claim a scenario was executed if it was only inferred. If the
  binary/Desktop could not be launched, say so and report what blocked it.
- Capture exact inputs, screen/command, visible error text, CLI/log output,
  and observed state for every issue.

## Scope: what this library covers, and what it deliberately does not

This library targets behavior that is **only detectable by running the app
and using it as a user** — the gaps left by unit/integration tests and static
audits:

| Already covered elsewhere | Where | Excluded here |
|---|---|---|
| Unit/integration correctness | `cargo test`, crate `tests/` | Yes |
| Desktop unit/component tests | `ui/desktop` Vitest suites | Yes |
| Static security / dataflow / architecture | `docs/cloud/audit-*` | Yes |
| Goose catalog compatibility normalization | `documentation/GOOSE_COMPATIBILITY.md` | Yes (except live install/use paths) |

What remains is the **lived operator experience**: first-run configure,
provider honesty, chat and tool loops, workspaces and credential profiles,
MCP extensions, skills/plugins, session import/export, permission gates,
CLI robustness, headless `run` / `serve` / ACP, Desktop multi-window
navigation, and load/stress seams.

## Safety rails (binding for every pass)

- Use a **disposable gosling root**. Set `GOSLING_PATH_ROOT` to a newly created
  absolute temp directory so config, data, sessions, workspaces, and secrets
  cannot touch the operator's live installation. `XDG_CONFIG_HOME` and
  `XDG_DATA_HOME` alone are not a complete isolation guarantee on every
  platform. Never playtest against production credentials or customer
  workspaces.
- Use test/sandbox LLM credentials only. Prefer the cheapest path available
  (local Ollama, free-tier router, or a throwaway key with a spend cap).
- No real personal data. No malicious payloads or exploitation — invalid
  input means *safe-but-wrong* (empty required fields, oversized text,
  wrong file types, mistyped flags).
- Tool-permission scenarios must run in a sandbox directory you own; do not
  approve shell/file tools against system paths you care about.
- Everything written during a pass stays in the disposable home; clean up
  afterward.
- Stress cards: stop if the host is unsafe (thermal, disk &lt; 1 GiB free).

## Execution contract (binding for every card)

The condensed cards stay readable by sharing these requirements. A result is
not a Pass unless all applicable rules below are satisfied.

1. **Record the baseline.** Capture commit/build identifier, `gosling
   --version`, OS/architecture, surface and version (CLI, TUI, or Desktop),
   provider/model, permission mode, `GOSLING_PATH_ROOT`, and scenario ID.
2. **Start from declared state.** Unless a card explicitly names a dependency,
   give it a clean disposable root and fixture directory. Record any reused
   state. Never let an earlier card's leftovers create an accidental pass.
3. **Use deterministic fixtures.** Seed files, session markers, mock providers,
   MCP fixtures, ports, and expected hashes before the run. A model's claim
   about its own identity, cwd, tool call, or saved file is not evidence;
   corroborate it with UI metadata, protocol capture, or filesystem state.
4. **Test each surface separately.** When a card names CLI, TUI, and Desktop,
   record a separate result for each applicable surface. Success on one surface
   does not imply success on another.
5. **Apply default deadlines.** Unless the card overrides them, require local
   UI/CLI feedback within 10 seconds, extension/server startup within 30
   seconds, cancellation within 10 seconds, and a provider-backed turn within
   120 seconds. Record elapsed time. Environment-caused slowness may be Blocked;
   an unexplained spinner or missed cancellation is Fail.
6. **Assert observable outcomes.** Record exit code, stdout and stderr
   separately for commands; before/after file hashes for persistence or
   mutation; process/port state for lifecycle cards; and screenshots plus
   renderer/backend logs for Desktop failures. Redact credentials before
   attaching evidence.
7. **Pass atomically.** Every statement under Expected is an assertion. If one
   fails, the card fails. Run Variations as separately labeled subcases; an
   unexecuted variation does not fail the base card unless the pass scope made
   it required.
8. **Use statuses consistently.** Blocked means a prerequisite outside the app
   prevented execution and includes the blocker. Not applicable means the
   feature is absent by design on that build/surface and includes evidence.
   Not executed means no attempt was made. Only runtime evidence can be marked
   Confirmed.
9. **Preserve failure artifacts.** On failure, stop mutating the fixture until
   logs, config, session IDs, exact input, screenshots, and relevant files are
   captured. Retry from a cloned fixture, not by repairing the evidence in
   place.

## How to run a pass

1. **Environment** (source checkout):
   ```bash
   source bin/activate-hermit
   export GOSLING_PATH_ROOT="$(mktemp -d)"
   cargo build -p gosling-cli
   # optional Desktop:
   # just run-ui   # or ui/desktop docs
   ```
   Ensure at least one provider is configured (`gosling configure`) under
   the disposable home. CLI binary: `./target/debug/gosling` or install path.
2. **Order**: execute files in numeric order. Lifecycle and a primary chat
   happy path first; stress last (after a green smoke). Within a file, run
   cards top to bottom; later cards often depend on state from earlier ones.
3. **Record**: for each card, fill in *Actual result*, *Status*
   (Pass / Fail / Blocked / Not applicable / Not executed), *Confirmation*
   (Confirmed / Suspicion), and issues with reproduction steps. Keep the
   library itself clean — record results in the pass report, not by editing
   cards.
4. **Report**: produce a playtest report per the baseline skill's
   `templates/playtest-report.md` shape. Durable summaries can live under
   `docs/cloud/` when the pass is part of a formal audit.

## Scenario card format

Each scenario uses this condensed card (aligned with Cuttlefish / the
skill's `templates/scenario-card.md`):

```
### <ID> — <Name>
- Goal: what the user is trying to accomplish
- Category: happy path / invalid input / boundary / empty state /
  interruption / persistence / delete-undo / settings / navigation /
  recovery / files / concurrency
- Preconditions: required state, data, config
- Steps: numbered user actions
- Expected: what a correct app does
- Observe: seams and secondary effects to watch for
- Variations: additional input/order permutations under the same card
```

Severity scale: **Critical** (crash, data corruption, lost work, blocked
primary workflow, irreversible action without warning) · **High** (major
workflow fails, wrong saved data, broken relaunch, unrecoverable without
technical help) · **Medium** (secondary workflow fails, unclear errors,
stuck UI, non-persisting settings) · **Low** (confusing label, glitch,
awkward navigation) · **Note** (observation or product question).

## Files (127 scenarios)

| File | Surface | Cards | Core question |
|---|---|---|---|
| [`01-first-run-and-lifecycle.md`](01-first-run-and-lifecycle.md) | install, configure, info, doctor | LC-01–04 | Can a new operator get from zero to a working agent? |
| [`02-chat-sessions.md`](02-chat-sessions.md) | CLI session, Desktop chat, slash cmds | CH-01–06 | Does the primary chat loop work, interrupt, and persist? |
| [`03-workspaces.md`](03-workspaces.md) | Desktop workspaces, credentials, artifacts | WS-01–04 | Do workspace pins, profiles, and outputs stay honest? |
| [`04-providers-and-models.md`](04-providers-and-models.md) | providers, model switch, planner | PM-01–04 | Is model selection truthful across surfaces? |
| [`05-extensions-and-mcp.md`](05-extensions-and-mcp.md) | extensions, MCP add/list/remove | EX-01–04 | Do MCP extensions install, load, fail, and unload cleanly? |
| [`06-skills-plugins-subagents.md`](06-skills-plugins-subagents.md) | skills, plugins, subagents | SK-01–03 | Do skill/plugin/subagent paths work end to end? |
| [`07-sessions-import-export.md`](07-sessions-import-export.md) | list/remove/export/import | SE-01–03 | Can sessions be listed, removed, and moved safely? |
| [`08-permissions-and-approvals.md`](08-permissions-and-approvals.md) | modes, tool perms, approvals UI | PA-01–03 | Do gates hold and resume as the operator expects? |
| [`09-cli-surface.md`](09-cli-surface.md) | help, flags, run, completion | CL-01–04 | Is the CLI robust to real terminal usage and misuse? |
| [`10-settings-config-navigation.md`](10-settings-config-navigation.md) | Settings, config.yaml, sidebar | ST-01–03 | Do settings persist and screens agree? |
| [`11-headless-serve-acp.md`](11-headless-serve-acp.md) | `run`, `serve`, `acp` | HS-01–03 | Do non-interactive and server surfaces stay coherent? |
| [`12-stress-and-adversarial.md`](12-stress-and-adversarial.md) | load, races, recovery under pain | SX-01–09 | Does gosling stay coherent when the operator is impatient? |
| [`13-advanced-cli-and-sessions.md`](13-advanced-cli-and-sessions.md) | resume, fork, edit, diagnostics, term, TUI, review | AC-01–10 | Are advanced CLI workflows deterministic and scriptable? |
| [`14-desktop-ux-and-integration.md`](14-desktop-ux-and-integration.md) | onboarding, windows, keyboard, artifacts, backend | DT-01–16 | Does Desktop behave like a durable native application? |
| [`15-context-and-filesystem.md`](15-context-and-filesystem.md) | hints, instructions, roots, stdin, runtime gates | CX-01–10 | Is context scoped, explainable, and isolated? |
| [`16-provider-and-network-resilience.md`](16-provider-and-network-resilience.md) | limits, disconnects, OAuth, metadata, cost | PN-01–12 | Do provider failures recover without lying or losing state? |
| [`17-acp-server-and-protocol.md`](17-acp-server-and-protocol.md) | auth, origins, TLS, framing, cancellation | AP-01–10 | Are ACP transports secure, bounded, and interoperable? |
| [`18-state-extension-and-permission-depth.md`](18-state-extension-and-permission-depth.md) | state integrity, migrations, MCP, plugins, approvals | SI-01–10 | Do cross-cutting state and safety boundaries hold under change? |
| [`19-deep-research.md`](19-deep-research.md) | zero-input research, reports, delegate fail-fast | DR-01–02 | Does Deep Research respect an empty Initial Inputs boundary and stop invalid delegation churn? |
| [`20-deep-research-regressions.md`](20-deep-research-regressions.md) | ACP persistence, mode compatibility, delegation, long inputs, extension failures, file compatibility | DR-03–09 | Do the known Deep Research failure modes remain repaired under exact replay? |

### Suggested pass shapes

| Pass | Files | Intent |
|---|---|---|
| Smoke / first day | 01 → 02 → 09 | Configure, one chat works, CLI is sane |
| Core product | 01 → 05, 08, 10 | Chat, workspaces, providers, MCP, perms, settings |
| Resilience | 04, 07, 11 | Model honesty, session portability, headless/serve |
| Deep Research | 19 → 20 | Prompt-only scope, report routing, and known-failure regression replay |
| Stress | 12 (after green smoke) | Concurrency, bloat, restart-under-load, env seams |
| Full library | 01 → 20 numeric order | Release or major-regression playtest (**127 cards**) |

## Required coverage checklist

A pass is complete only when every category below has at least one executed
scenario (or an explicit not-applicable/blocked note):

- [ ] First launch / initial empty state
- [ ] Primary happy-path workflow (prompt → response)
- [ ] Primary workflow with invalid input
- [ ] Save / persistence behavior
- [ ] Delete, cancel, or undo behavior
- [ ] Settings or preferences persistence
- [ ] Navigation across major Desktop surfaces (or CLI equivalent)
- [ ] Close and relaunch behavior
- [ ] Interrupted or stopped workflow
- [ ] File attach / export / workspace outputs
- [ ] Error recovery (provider unavailable, extension fail, crash)
- [ ] Edge or boundary input
- [ ] Model / provider selection and mid-session switching
- [ ] Permission / approval boundary
- [ ] Concurrency or load stress
- [ ] Headless or server surface (`run` / `serve` / ACP)
- [ ] Session resume, fork, external edit, and diagnostics
- [ ] Terminal integration, TUI launch, and review dry-run
- [ ] Keyboard-only Desktop operation and narrow-window layout
- [ ] Artifact preview and workbench persistence
- [ ] Project instruction/context hierarchy and ignored-file boundary
- [ ] Provider rate-limit, timeout, disconnect, and context exhaustion
- [ ] Server authentication, Origin validation, TLS, and protocol framing
- [ ] Config/session migration from a previous supported release

## Scenario index (127)

| ID | File | Name |
|---|---|---|
| LC-01 | 01 | Fresh install to first successful reply |
| LC-02 | 01 | First-launch empty / unconfigured states |
| LC-03 | 01 | `info` / `doctor` honesty |
| LC-04 | 01 | Config hand-edit then relaunch |
| CH-01 | 02 | First message to first response |
| CH-02 | 02 | Composer / REPL input seams |
| CH-03 | 02 | Interrupt mid-run then continue |
| CH-04 | 02 | Session persistence across relaunch |
| CH-05 | 02 | Parallel sessions isolation |
| CH-06 | 02 | Slash-command discoverability and typos |
| WS-01 | 03 | Create workspace and pin new chat |
| WS-02 | 03 | Credential profile bind and secret non-echo |
| WS-03 | 03 | Missing primary folder / relink |
| WS-04 | 03 | Artifact save routes to product outputs |
| PM-01 | 04 | Configure provider and model |
| PM-02 | 04 | Mid-session model or provider switch |
| PM-03 | 04 | Bad / expired API key failure clarity |
| PM-04 | 04 | Planner vs main model split |
| EX-01 | 05 | Enable bundled extension and use a tool |
| EX-02 | 05 | Add streamable HTTP / stdio MCP extension |
| EX-03 | 05 | Broken MCP extension fails closed |
| EX-04 | 05 | Remove extension; session no longer offers tools |
| SK-01 | 06 | Skills list and invoke |
| SK-02 | 06 | Plugin install/update from git |
| SK-03 | 06 | Subagent parallel fan-out |
| SE-01 | 07 | Session list, rename, remove |
| SE-02 | 07 | Export session |
| SE-03 | 07 | Import session (JSON / foreign jsonl) |
| PA-01 | 08 | Manual approval mode gates a tool |
| PA-02 | 08 | Never-allow tool is refused |
| PA-03 | 08 | Mode switch mid-session |
| CL-01 | 09 | Help and discoverability |
| CL-02 | 09 | Unknown commands and bad flags |
| CL-03 | 09 | `gosling run` one-shot formats |
| CL-04 | 09 | Completion generation |
| ST-01 | 10 | Desktop settings persist across relaunch |
| ST-02 | 10 | Sidebar navigation stress |
| ST-03 | 10 | Invalid config.yaml values |
| HS-01 | 11 | Headless `run` with budgets |
| HS-02 | 11 | `gosling serve` lifecycle |
| HS-03 | 11 | `gosling acp` stdio smoke |
| SX-01 | 12 | Session stampede (many short chats) |
| SX-02 | 12 | Multi-window / multi-tab race on one session |
| SX-03 | 12 | Rapid model thrash under active stream |
| SX-04 | 12 | Hundred-turn history bloat |
| SX-05 | 12 | Extension/tool storm and max-turns budget |
| SX-06 | 12 | Hard kill mid-run then recover |
| SX-07 | 12 | Workspace switch race during artifact save |
| SX-08 | 12 | Config thrash + concurrent CLI invocations |
| SX-09 | 12 | Cross-surface consistency after chaos |
| AC-01 | 13 | Resume selection by recency, name, and ID |
| AC-02 | 13 | Fork creates an independent history |
| AC-03 | 13 | External-editor resume and failure handling |
| AC-04 | 13 | Session diagnostics artifact |
| AC-05 | 13 | Session list filters, ordering, and JSON |
| AC-06 | 13 | Recent project discovery and launch |
| AC-07 | 13 | Terminal shell initialization is non-destructive |
| AC-08 | 13 | Terminal session identity and isolation |
| AC-09 | 13 | TUI resolution, launch, and dependency failure |
| AC-10 | 13 | Review dry-run discovery and scoping |
| DT-01 | 14 | Onboarding interruption and resume |
| DT-02 | 14 | Window close versus application quit |
| DT-03 | 14 | Keyboard-only navigation and focus |
| DT-04 | 14 | Shortcut rebinding, conflicts, and persistence |
| DT-05 | 14 | Narrow window, resize, and long-content layout |
| DT-06 | 14 | Artifact preview type matrix |
| DT-07 | 14 | Artifact workbench state across navigation and relaunch |
| DT-08 | 14 | Archive and restore session lifecycle |
| DT-09 | 14 | External backend authentication and reconnect |
| DT-10 | 14 | Native notifications and denied permission |
| DT-11 | 14 | Artifact delete and Trash recovery |
| DT-12 | 14 | Copy artifact contents authorization boundary |
| DT-13 | 14 | Repository file filter persistence |
| DT-14 | 14 | Artifact file timestamp display |
| DT-15 | 14 | Workspace readiness indicator accuracy |
| DT-16 | 14 | Dialog, dropdown, and tooltip z-index stacking |
| CX-01 | 15 | Root `AGENTS.md` instruction loading |
| CX-02 | 15 | Nested context loads only when scoped |
| CX-03 | 15 | Custom context filenames and ordering |
| CX-04 | 15 | Ignored and sensitive files stay out of context |
| CX-05 | 15 | Persistent instructions refresh between turns |
| CX-06 | 15 | `GOSLING_PATH_ROOT` provides complete isolation |
| CX-07 | 15 | `--no-session` leaves no resumable history |
| CX-08 | 15 | Instruction file and stdin boundaries |
| CX-09 | 15 | Code-execution runtime disable gate |
| CX-10 | 15 | One-shot system prompt remains scoped |
| PN-01 | 16 | Rate-limit response and retry recovery |
| PN-02 | 16 | Network disconnect during streaming |
| PN-03 | 16 | Slow provider timeout and cancellation |
| PN-04 | 16 | Context-window exhaustion and compaction |
| PN-05 | 16 | Empty and malformed provider responses |
| PN-06 | 16 | Provider/model override precedence |
| PN-07 | 16 | Model-list failure and custom model ID |
| PN-08 | 16 | Local provider stops and restarts |
| PN-09 | 16 | OAuth expiry, refresh, and user cancellation |
| PN-10 | 16 | Usage, cost, and statistics consistency |
| PN-11 | 16 | Turn-local provider failover |
| PN-12 | 16 | Auto-compact reduction setting scopes partial vs full compaction |
| AP-01 | 17 | Authenticated serve startup requirement |
| AP-02 | 17 | Missing, wrong, and correct shared secret |
| AP-03 | 17 | Origin allowlist replacement semantics |
| AP-04 | 17 | TLS certificate/key validation |
| AP-05 | 17 | Stdio framing and stdout cleanliness |
| AP-06 | 17 | Invalid ACP messages preserve the connection |
| AP-07 | 17 | Concurrent ACP session isolation |
| AP-08 | 17 | ACP cancellation reaches a terminal state |
| AP-09 | 17 | Server termination during an active request |
| AP-10 | 17 | Protocol version and capability negotiation |
| SI-01 | 18 | Duplicate workspace identity and rename |
| SI-02 | 18 | Delete workspace with pinned sessions |
| SI-03 | 18 | Symlinked workspace and reference-folder boundaries |
| SI-04 | 18 | Export format matrix, overwrite, and permissions |
| SI-05 | 18 | Imported-session working-directory trust boundary |
| SI-06 | 18 | Upgrade migration from a prior supported release |
| SI-07 | 18 | Duplicate MCP install and command environment |
| SI-08 | 18 | MCP secret storage and redaction |
| SI-09 | 18 | Malformed skill/plugin and interrupted update |
| SI-10 | 18 | Approval scope, persistence, and competing clients |
| DR-01 | 19 | Prompt-only research does not discover workspace files |
| DR-02 | 19 | Invalid delegate source fails once without retry churn |
| DR-03 | 20 | ACP prompt persistence survives queued writers |
| DR-04 | 20 | Solo external ACP provider retains Autonomous mode |
| DR-05 | 20 | Legacy `source: "dummy"` becomes an ad-hoc delegate only |
| DR-06 | 20 | External-provider delegate runs bounded and tool-disabled |
| DR-07 | 20 | Initial Inputs contain long and multiline content |
| DR-08 | 20 | Invalid extension parameters fail once with diagnosable feedback |
| DR-09 | 20 | Initial Inputs accept traditional document and image formats |
