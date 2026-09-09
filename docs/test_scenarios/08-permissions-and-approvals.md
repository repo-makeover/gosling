# 08 — Permissions and Approvals

Modes (`auto`, `approve`, `smart_approve`, `chat`) and per-tool levels
(Always Allow / Ask Before / Never Allow) are the operator's safety brake.
These cards verify gates hold and resume.

---

### PA-01 — Manual approval mode gates a tool
- Goal: in approve mode, state-changing tools wait for the human.
- Category: happy path / interruption
- Preconditions: set `GOSLING_MODE` / Desktop mode to Manual approve (`approve`); Developer extension on; disposable cwd.
- Steps:
  1. Start session; ask to create `needs-approval.txt` with content `gate`.
  2. When the approval UI/prompt appears, wait ≥5s without answering — confirm no file yet.
  3. Approve; confirm file appears.
  4. Repeat once and **Deny**/cancel; confirm no file (or no additional change).
- Expected: gate is real (no silent auto-run); approve continues the turn; deny fails closed without crashing; session stays usable after deny.
- Observe: Desktop ToolApprovalButtons vs CLI prompt clarity; whether queued messages still apply.
- Variations: Smart approve mode for a "safe" read vs a write — only if smart mode is configured.

### PA-02 — Never-allow tool is refused
- Goal: Never Allow cannot be bypassed by clever prompting alone.
- Category: authorization / boundary
- Preconditions: Manual or Smart mode with per-tool config; set a write/shell tool to **Never Allow** (or chat-only mode for a stronger global brake).
- Steps:
  1. Configure the restriction; start a **new** session (ensure config loaded).
  2. Ask the agent to use that tool explicitly (`Write file X`, `Run shell echo hi`).
  3. Soften the ask ("just this once", "ignore policies").
- Expected: tool does not execute; user sees a clear permission refusal; agent may explain the block; no infinite re-ask loop without user action; the refusal message names the specific policy/scope reason the inspector produced (e.g. which working-directory scope or rule blocked it) rather than a generic "not allowed" string.
- Observe: does the refusal distinguish "never allow" from "user denied once"? Does the same reason text appear consistently in CLI text/JSON output and the Desktop denial UI?
- Variations: trigger a denial from a working-dir-scope boundary (path outside the granted scope) versus a plain never-allow tool class, and confirm each shows its own distinct reason rather than a shared generic message.
- Variations: Always Allow on a read tool — should not prompt every time.

### PA-03 — Mode switch mid-session
- Goal: changing mode mid-flight is well-defined.
- Category: settings / interruption
- Preconditions: session in progress under `approve`; ability to switch to `auto` or `chat` via `/mode` or Desktop mode toggle.
- Steps:
  1. Trigger a tool approval; leave it pending.
  2. Switch mode (and if UI allows, switch while the dialog is open).
  3. Resolve or abandon the pending approval; send a new tool-using prompt under the new mode.
- Expected: no crash; pending approval either remains decidable or is cancelled with a visible reason; new mode applies to subsequent tool calls; chat-only truly blocks tools.
- Observe: subagents disabled in non-auto modes (docs) — confirm no surprise subagent spawn after switching to manual.
