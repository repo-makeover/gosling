# 14 — Desktop UX and Native Integration

Desktop must remain operable across onboarding, windows, keyboard input,
artifacts, archived sessions, external backends, and macOS integration. Record
screenshots and both renderer/backend logs for every failure.

---

### DT-01 — Onboarding interruption and resume

- Goal: an unconfigured user can leave and resume onboarding without a dead end.
- Category: first launch / interruption
- Preconditions: fresh disposable root and Desktop installation.
- Steps: launch; advance one screen; go back; quit on each major step in separate fresh runs; relaunch; finally configure a test provider and create a chat.
- Expected: every relaunch shows either the first incomplete step or an explicit restart choice; Back never loses already validated non-secret input unexpectedly; completion transitions once to a usable chat; no duplicate provider entries.
- Observe: keyboard focus, default button, and secret-field clearing after quit.

### DT-02 — Window close versus application quit

- Goal: macOS close, reopen, quit, and relaunch have distinct, honest lifecycle behavior.
- Category: lifecycle / persistence
- Preconditions: Desktop with two chats, one actively streaming from a controllable delayed fixture.
- Steps: close the active window with the red control; reopen via Dock/menu; close all windows; use `Cmd+Q`; relaunch and reopen both sessions.
- Expected: close behavior matches app convention and does not orphan an invisible unbounded run; quit terminates backend children within 10 seconds; relaunch shows honest terminal state and persisted completed history.
- Observe: Dock indicator, menu enablement, and approval dialogs owned by a closed window.

### DT-03 — Keyboard-only navigation and focus

- Goal: all primary Desktop workflows are reachable without a pointer.
- Category: accessibility / navigation
- Preconditions: Desktop configured with one session and one workspace.
- Steps: from launch, use Tab/Shift-Tab, arrows, Enter, Space, Escape, and documented shortcuts to create/open a chat, send text, stop a run, visit each navigation item, open/close a dialog, and return to composer.
- Expected: focus is always visible; order follows visual/logical order; no keyboard trap; Escape closes only the top modal; sending and stopping work; focus returns to the invoking control after close.
- Observe: screen-reader names for icon-only controls using macOS Accessibility Inspector if available.

### DT-04 — Shortcut rebinding, conflicts, and persistence

- Goal: user-defined shortcuts validate conflicts and survive relaunch.
- Category: settings / persistence
- Preconditions: Desktop Keyboard settings; record defaults and real system-level shortcuts to avoid.
- Steps: assign an unused combination; invoke it; try a duplicate app shortcut, a reserved macOS shortcut, and an incomplete chord; reset one binding; quit and relaunch.
- Expected: valid binding works exactly once; conflicts are rejected or require explicit resolution; incomplete input cannot erase a binding; reset restores documented default; final values persist.
- Observe: global versus app-local scope and behavior when focus is in the composer.

### DT-05 — Narrow window, resize, and long-content layout

- Goal: Desktop remains usable over its supported size range.
- Category: boundary / navigation
- Preconditions: session containing a long unbroken URL, wide code block, long tool name, deep list, and long workspace/model names.
- Steps: resize gradually from large to minimum width/height; toggle sidebar and artifact pane; scroll history; open settings and approval dialog at minimum size; return to large size.
- Expected: no overlapping controls, unreachable buttons, horizontal page escape, blank panel, or lost content; intended panes scroll independently; layout recovers after expansion.
- Observe: text truncation has accessible full-name affordance where selection depends on it.

### DT-06 — Artifact inventory and preview type matrix

- Goal: completed previewable outputs populate automatically, supported artifacts preview safely, and unsupported file types stay out of the presented inventory.
- Category: files / boundary
- Preconditions: small known fixtures for Markdown, JSON, HTML, image, PDF, SVG, empty file, unknown binary, and a missing path.
- Steps: complete one turn that writes/references four supported fixtures plus an unknown binary and an email-like compatibility inference; verify `Outputs 4` before clicking a chip; confirm the unsupported entries are absent; open each listed fixture; switch tabs rapidly; use Save a copy where offered; compare source hash before and after preview.
- Expected: the inventory populates without opening the pane or reading a file; `.rs`, `.ts`, `.py`, and `.sh` are code; file types with no in-app renderer are excluded from the list and count; supported formats render the intended content; active content cannot execute privileged app actions; malformed or missing supported files show a bounded error; preview never mutates source; saved copy hash matches source where no conversion is promised.
- Observe: large-file warning and renderer console errors.

### DT-07 — Session artifact state across navigation and relaunch

- Goal: durable inventory and session-scoped preview state restore without cross-session or stale-file confusion.
- Category: persistence / navigation
- Preconditions: three artifact tabs from two sessions/workspaces; record tab order and pane width.
- Steps: resize and select the middle tab; navigate away and back; close one tab; quit/relaunch; move one source file before another relaunch.
- Expected: switching sessions immediately replaces the inventory and restores only that session's supported tabs/selection; persisted unsupported tabs are discarded; closed tab stays closed; relaunch reloads inventory from ACP and restores only documented preview state; moved source becomes a named missing-file state and is not replaced with another file of the same basename. Inventory presence alone never grants access to an outside-root path.
- Observe: state isolation across multiple Desktop windows.

### DT-08 — Archive and restore session lifecycle

- Goal: archiving changes visibility, not history integrity.
- Category: delete-undo / persistence
- Preconditions: three completed sessions including one pinned workspace session and one open in another window.
- Steps: archive one from history; inspect Active and Archived tabs; attempt to open the archived session from stale UI; restore it; archive the cross-window session; relaunch.
- Expected: membership changes exactly once; archive preserves ID/history/exportability; restore returns the same ID; stale views refresh or explain state; cross-window action cannot create a duplicate or zombie.
- Observe: counts, selection after removal, and CLI list agreement.

### DT-09 — External backend authentication and reconnect

- Goal: Desktop can switch to a remote gosling backend and recover from bad settings.
- Category: settings / recovery
- Preconditions: local `gosling serve` on a test port with known secret and certificate mode; embedded backend healthy.
- Steps: configure correct URL/secret and connect; send a marker turn; change to wrong secret, unreachable port, malformed URL, and wrong TLS expectation one at a time; restore correct settings; switch back to embedded backend.
- Expected: correct remote works; each fault is distinguished and bounded; secret is never displayed after save; reconnect succeeds without resetting unrelated settings; sessions are attributed to the backend that owns them.
- Observe: retry cadence and whether an old authenticated socket survives credential replacement.

### DT-10 — Native notifications and denied permission

- Goal: task notifications respect app setting and macOS permission state.
- Category: settings / interruption
- Preconditions: controllable delayed completion; Desktop notification toggle; ability to reset permission for the test app if safe.
- Steps: enable notifications and complete a backgrounded task; repeat in foreground; deny macOS permission and retry; disable in-app notifications and retry; click a delivered notification.
- Expected: eligible background completion sends at most one notification; foreground/disabled behavior matches the setting; OS denial produces no retry storm and offers an actionable settings path; clicking focuses the correct session.
- Observe: cancelled/failed runs use accurate wording and never claim success.

### DT-11 — Artifact delete and Trash recovery

- Goal: user can remove artifact files from the workbench with an undo-safe path, without losing saved revision history.
- Category: delete-undo / files
- Preconditions: session with several outputs listed in the ArtifactPane; OS Trash reachable; disposable workspace root.
- Steps: select one file's row delete button; confirm the "Move to Trash" dialog; select multiple files via checkboxes and use "Move selected to Trash"; delete a file that was concurrently removed on disk before confirming; attempt delete while the backend is unreachable.
- Expected: the confirmation dialog lists every selected path before commit; confirming moves files to OS Trash (not a permanent unlink) and removes only the confirmed batch from the list; a file already missing on disk reports "missing" and is dropped from the list without being treated as a failure; a genuine per-item failure keeps that item and shows a bounded error beside it without blocking the rest of the batch; canceling the dialog leaves the list untouched; saved revision history for a trashed file remains reachable afterward (cross-check with DT-12/Saved history).
- Observe: batch-limit behavior (`ARTIFACT_TRASH_BATCH_LIMIT`) when selecting more files than one batch, and toast wording for trashed/missing/failed counts.
- Variations: delete the currently active preview tab; delete then immediately reopen the same session.

### DT-12 — Copy artifact contents authorization boundary

- Goal: "Copy contents" only ever copies bytes the renderer window is authorized to read, and fails safely on non-text or oversized files.
- Category: files / boundary
- Preconditions: one text artifact under an authorized workspace root, one binary/image/PDF artifact, one text file just over the 20 MiB copy limit, and (if reachable in a disposable setup) a path outside the workspace's authorized roots.
- Steps: use Copy contents on the text file; on an image/PDF/unknown-kind tab; on the oversized text file; on a file that changes on disk between stat and read; on a path outside authorized roots if reachable (e.g. a manipulated tab source).
- Expected: the text file copies exact UTF-8 content to the clipboard and confirms success; image/PDF/unknown kinds either use the tab's already-loaded content or do not offer the control (per `supportsCopyContents`); the oversized file is rejected with the stated 20 MiB text-file limit message and nothing is copied; a file that changes mid-read is rejected with a "changed while copying" error rather than copying stale or partial bytes; a non-UTF-8/binary file is rejected rather than silently copying garbage; a path outside the authorized roots is rejected by the main-process access check, not merely hidden in the UI.
- Observe: the clipboard actually contains the expected text (verify via a paste target, not just the toast), and whether a failed copy leaves a stale success toast.

### DT-13 — Repository file filter persistence

- Goal: the "Hide repository files" toggle reliably classifies source files and persists across sessions and relaunch.
- Category: settings / persistence
- Preconditions: a workspace whose outputs mix source-code files (`.rs`, `.ts`, `.py`, `.sh`, etc.) with non-code outputs (Markdown, JSON, images).
- Steps: enable "Hide repository files" in the ArtifactPane; confirm the list and `Outputs N` count update immediately; switch to a different session/workspace; relaunch Desktop; disable the toggle; keep it enabled until every remaining item is filtered.
- Expected: the toggle only removes classification-eligible source files, never files the preview type matrix already excludes for other reasons; the setting persists across relaunch via the workbench's stored state and is not silently reset per session; when every visible item is filtered out, the pane shows the documented empty-filter state rather than an unexplained blank list; disabling restores the full list without requiring a refresh.
- Observe: whether the filter is workspace-scoped or applies uniformly to every workspace, since its persisted value is not keyed by session.

### DT-14 — Artifact file timestamp display

- Goal: created/modified timestamps for artifact files load asynchronously without blocking the list and degrade honestly when unavailable.
- Category: files / boundary
- Preconditions: session with a normal file, a file whose timestamps the OS can't report (e.g. removed just before the request), and a many-file case to exercise the async fetch.
- Steps: open the ArtifactPane and observe the list before and after timestamps resolve; view a file whose stat call fails; add/remove files while timestamps are loading.
- Expected: the list and its delete/select controls are usable immediately, showing a "Reading file timestamps…" state per row rather than blocking on the fetch; once resolved, created/modified times render in local time with a full-precision tooltip; a file whose timestamp lookup fails shows the "unavailable" state rather than a stale or zero date; timestamps do not misattach to the wrong row when the underlying file list changes mid-fetch.
- Observe: IPC round-trip behavior for large lists (one batched call vs. one per file).

### DT-15 — Workspace readiness indicator accuracy

- Goal: the sidebar's per-workspace "chat ready" indicator reflects an unseen new reply, not any background activity.
- Category: navigation / notification
- Preconditions: two workspaces, each with an active session; ability to background one session's chat while a turn streams.
- Steps: start a turn in workspace A while its chat is the visible/focused view; start a turn in workspace B while a different chat is focused; let both finish; switch focus to workspace B and back; remove/close the session carrying an unread indicator; trigger a run that ends without a new assistant reply (e.g. a tool-only run that errors before any reply) while backgrounded.
- Expected: workspace A never lights up (it was being viewed when its reply landed); workspace B lights up once its reply lands while unviewed; viewing workspace B clears its indicator; removing a session with a pending indicator does not leave a dangling icon or throw; a completed run with no genuinely new reply ID does not mark the workspace ready.
- Observe: the indicator's accessible label/title text, and whether it reappears if a second unseen reply lands in the same session after being cleared.

### DT-16 — Dialog, dropdown, and tooltip z-index stacking

- Goal: overlays layer correctly relative to each other so a dropdown or tooltip opened from inside a modal is never hidden behind it.
- Category: navigation / boundary
- Preconditions: a flow that opens a confirmation modal containing a dropdown or a hover-tooltip trigger (e.g. the artifact delete confirmation, or a settings dialog with a select control), plus a long-content confirmation modal (many selected paths).
- Steps: open a confirmation dialog; open a dropdown/select and a tooltip while the dialog is open; scroll a long confirmation modal's body; open two stacked dialogs if reachable; close the top overlay and confirm the one beneath regains interaction.
- Expected: a dropdown or tooltip triggered from within a dialog renders above the dialog's own overlay and content, never clipped or hidden behind it; scrolling long modal content scrolls only the content area, not the page behind it; closing the top overlay restores full interactivity to the one below without a stuck backdrop or unreachable close control.
- Observe: any leftover backdrop element after rapid open/close, and keyboard focus trapping when a dropdown is nested inside a dialog.
