# 02 — Chat Sessions (CLI and Desktop)

The primary value proposition: send a message, watch the agent work, get a
result, keep the conversation. Covers happy path, input seams, interrupt,
persistence, isolation, and slash commands.

---

### CH-01 — First message to first response (primary happy path)
- Goal: one message in, one useful agent response out, on CLI and/or Desktop.
- Category: happy path
- Preconditions: provider configured (LC-01); disposable project cwd containing known marker files `alpha.txt` and `beta.txt` and no other `.txt` files.
- Steps:
  1. CLI: `gosling session` → send `List every .txt filename in the current working directory, one per line.`
  2. Desktop: open a separate new chat in the same directory and send the same prompt; record this as a separate surface result.
- Expected: message appears once; any tool status reaches a terminal state; the response includes `alpha.txt` and `beta.txt` and does not invent a third `.txt` file; session reaches completed/idle within the deadline.
- Observe: is it clear which provider/model answered? Does the session get a sensible title in the list?
- Variations: follow-up that depends on the first answer; confirm context is retained.

### CH-02 — Composer / REPL input seams
- Goal: the input surface tolerates realistic messy input.
- Category: invalid input / boundary
- Preconditions: an open session (CLI or Desktop).
- Steps / Variations (each is a send attempt):
  1. Empty message; whitespace-only message.
  2. A generated 16 KiB ASCII message with unique markers at byte 1 and the end.
  3. Emoji, accented text, non-Latin scripts (`日本語`, `العربية`), RTL text.
  4. Markdown/HTML-ish text (`<script>alert(1)</script>`, backticks, `# heading`) — rendering check, not an exploit.
  5. Multi-line input with newlines; paste with trailing whitespace.
  6. Rapid double-send of the same text.
- Expected: empty/whitespace sends create no turn; both long-input markers persist in reopened history or an explicit size limit rejects the send before billing; unicode round-trips byte-for-byte; markup renders inert; double-send creates at most one accepted user turn or clearly reports two intentionally accepted turns.
- Observe: Desktop markdown rendering of user vs assistant content; CLI wrap/scroll behavior.

### CH-03 — Interrupt mid-run then continue
- Goal: stop/cancel mid-run behaves like a user expects.
- Category: interruption
- Preconditions: session able to start a long task (tooling or long generation).
- Steps:
  1. Start a task proven during setup to remain active for at least 30 seconds, preferably a controllable delayed MCP fixture rather than relying on prompt wording.
  2. Interrupt (Ctrl-C in CLI per product norms; Desktop stop control).
  3. Observe session state; send `Say READY` as a new turn.
- Expected: provider/tool cancellation is observable; work stops within 10 seconds; session lands in a clear stopped/idle state; no output from the cancelled operation appears after terminal state; follow-up produces `READY` without a new session unless documented.
- Variations: force the run to lose its lease/turn ownership (e.g. a competing client or server-side revocation) instead of a user-initiated cancel, and confirm the terminal state and any `terminal_error` metadata reported to CLI/ACP consumers distinguishes lease loss from a deliberate user cancellation rather than reporting both identically or as success.
- Variations: interrupt twice quickly; navigate away mid-stream on Desktop and return; close the Desktop window mid-stream and reopen the session.

### CH-04 — Session persistence across relaunch
- Goal: sessions are truly persisted, not only in memory.
- Category: persistence / relaunch
- Preconditions: several sessions with multi-turn history; at least one with an attachment if Desktop supports it.
- Steps:
  1. Note session ids/names and a unique marker string in history.
  2. Fully quit CLI sessions and/or quit Desktop.
  3. Relaunch; list sessions (`gosling session list` / Desktop history); reopen each.
- Expected: list, titles, full history (or documented window), and attachments survive; no duplicate or ghost sessions; interrupted mid-run sessions show an honest terminal state.
- Observe: timestamps not re-stamped to "now"; working directory still correct on resume.

### CH-05 — Parallel sessions isolation
- Goal: two concurrent sessions do not bleed context or tool side effects into each other.
- Category: concurrency
- Preconditions: ability to open two CLI sessions or two Desktop chats; two distinct disposable directories A and B.
- Steps:
  1. Session A in dir A: ask `What is your cwd? Create a file named only-in-A.txt with content A.`
  2. Session B in dir B: ask `What is your cwd? Create a file named only-in-B.txt with content B.`
  3. Confirm filesystem results; ask each session to read the other's file.
- Expected: each session reports its own cwd; files land only in the intended directory; neither session silently uses the other's context as if it were local unless tools explicitly reach across (and then paths must be clear).
- Observe: Desktop header/workspace pin matches each session.

### CH-06 — Slash-command discoverability and typos
- Goal: in-session slash commands help the user; typos fail safely.
- Category: invalid input / navigation
- Preconditions: interactive CLI session (Desktop slash/command palette if present).
- Steps:
  1. Run `/help` (and any documented help surface).
  2. Exercise a few real commands: `/model`, `/mode`, `/skills`, `/clear` or `/compact` as documented — prefer non-destructive first.
  3. Type a typo that looks like a command: `/halp`, `/eixt`, `/moel`.
- Expected: help lists real commands; real commands do what current help says; unknown `/…` tokens either give an "unknown command" hint or are explicitly shown as a model-bound message before submission; no silent billable reroute.
- Observe: does plain `exit`/`quit` without slash leave the session? Can the user send the word "exit" to the model if needed?
- Variations: `/prompt` with missing args; empty `/`.
