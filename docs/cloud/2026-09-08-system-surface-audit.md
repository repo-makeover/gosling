# Independent Audit Report: 2026-09-08 System & Surface Scan

**Audit Date**: 2026-09-08  
**Scope**: Work conducted over the course of today (commits `a6ee677a6..HEAD`), focusing on **Dataflow Integrity**, **Reliability**, and **Workflow / GUI Truth** (Security scans excluded per directive).  
**Methodology**: `agent-skills` (`audit-dataflow-integrity` v3.3, `audit-reliability` v3.0, `audit-workflow-gui` v3.1, `audit-dataflow-state-transition` v3.1).  
**Repository**: `gosling` (v1.2.3)

---

## 1. Executive Summary & Scope

Today's changes (16 commits across 122 files, +11,085 / -785 LOC) delivered five major product and runtime capabilities:
1. **Context Management & Auto-Compaction Budgeting**: `autoCompactReduction` preference and budget-capped compaction cutoff calculations (`crates/gosling/src/context_mgmt/mod.rs`, ACP config & server custom requests).
2. **Session Output Revisions & Contribution History**: SQLite `output_revisions` table schema, migration 32, file snapshot tracking, attribution extraction, atomic file replacement, and ACP endpoints (`crates/gosling/src/session/output_revisions.rs`, `output_revisions_storage.rs`).
3. **Artifact Management in Desktop UI**: Trashing/deletion, text content copying with UTF-8 enforcement, filesystem timestamps, and repository classification filtering (`ui/desktop/src/main/fileIpc.ts`, `ArtifactPane.tsx`, `ArtifactFileList.tsx`, `OutputHistory.tsx`).
4. **Permission & Scope Inspection**: Shell comment handling, AST tree-sitter whitespace splicing, temporary scratch directory boundaries, and elicitation readability (`crates/gosling/src/permission/working_dir_scope_inspector.rs`, `permission_inspector.rs`, `ToolApprovalButtons.tsx`).
5. **Desktop UI Layering & Navigation**: Unified `Z_INDEX` constants across modals, dialogs, dropdowns, and tooltips, plus workspace readiness dot indicators.

This independent scan analyzed these surfaces for failure behaviors, state drift, data loss, false success, and display lag.

---

## 2. Findings Summary Table

| ID | Domain | Severity | Confidence | Evidence Basis | Title |
|---|---|---|---|---|---|
| **DAT-GSL-001** | Data-Integrity | High | Confirmed | source-evidenced | Strict `\n` line-ending check causes duplicated contribution history and file corruption on CRLF or formatted Markdown |
| **DAT-GSL-002** | Data-Integrity | Medium | Confirmed | source-evidenced | Split transactions in `restore_output_revision` leave orphaned Baseline records on filesystem or lock failure |
| **WFG-GSL-001** | Workflow-GUI | Medium | Confirmed | source-evidenced | ArtifactPane output list does not clear trashed items immediately from view without refresh |
| **WFG-GSL-002** | Workflow-GUI | Low | Confirmed | source-evidenced | Preference `autoCompactReduction` allows settings $\ge$ threshold, causing silent reduction fallback |
| **REL-GSL-001** | Reliability | Medium | Confirmed | source-evidenced | Trashing missing files acknowledges missing status in UI but fails to prune `session_artifacts` in database |
| **REL-GSL-002** | Reliability | Low | Likely | source-evidenced | Full 20 MiB in-memory buffering and string decoding during `copy-artifact-contents` |
| **REL-GSL-003** | Reliability | Low | Confirmed | source-evidenced | Redundant double path canonicalization per mutation target in `out_of_scope_path` |

---

## 3. Detailed Material Findings

### DAT-GSL-001: Strict `\n` line-ending check causes duplicated contribution history and file corruption on CRLF or formatted Markdown

Severity: High  
Confidence: Confirmed  
Evidence basis: source-evidenced  
Domain: Data-Integrity  

Evidence:
- `crates/gosling/src/session/output_revisions.rs:16-17, 185-196`:
  ```rust
  const HISTORY_START: &str = "\n\n<!-- gosling:output-history:start -->\n";
  const HISTORY_END: &str = "<!-- gosling:output-history:end -->\n";

  pub(crate) fn markdown_body(path: &Path, bytes: &[u8]) -> Vec<u8> {
      if is_markdown(path) {
          if let Ok(text) = std::str::from_utf8(bytes) {
              if text.ends_with(HISTORY_END) {
                  if let Some((body, _)) = text.rsplit_once(HISTORY_START) {
                      return body.as_bytes().to_vec();
                  }
              }
          }
      }
      bytes.to_vec()
  }
  ```
- `ui/desktop/src/components/artifacts/OutputHistory.tsx:78-81`:
  ```typescript
  const marker = text.lastIndexOf('\n\n<!-- gosling:output-history:start -->\n');
  return marker >= 0 && text.endsWith('<!-- gosling:output-history:end -->\n')
    ? text.slice(0, marker)
    : text;
  ```

Observed behavior:
- `markdown_body` strips the existing history table if and only if `text.ends_with("<!-- gosling:output-history:end -->\n")`.
- If the file is saved on Windows (`\r\n`), has a trailing newline added by an editor/formatter (`\n\n`), or has trailing whitespace, `text.ends_with(HISTORY_END)` returns `false`.

Expected boundary:
- Markdown body extraction must be robust to standard line endings (`\r\n` and `\n`) and trailing whitespace so that previously appended history is always recognized and isolated.

Failure mechanism:
- When `ends_with(HISTORY_END)` fails:
  1. `markdown_body` returns `bytes.to_vec()` containing the previously generated `## Output contribution history` table.
  2. `digest(&after.body)` hashes the table along with the document body.
  3. `annotated_snapshot` appends a second `<!-- gosling:output-history:start -->` block after the unstripped first one.
  4. Every subsequent agent edit replicates the entire table, exponentially inflating file size and corrupting document structure.
  5. In the UI, `textPreview` fails to strip the comment and renders raw HTML comment delimiters and stale tables in the preview pane.

Break-it angle:
- Write a Markdown file in `Outputs/doc.md`. Open it in an editor that formats with CRLF or adds a trailing POSIX newline (`\n\n`), then trigger a tool update. The file now contains two full history blocks.

Impact:
- Accumulating file corruption, unbounded file growth, false revision detection, and broken diff previews in Desktop UI.

Operational impact:
- Blast radius: Workflow & User Documents
- Side-effect class: file
- Reversibility: compensatable (manual table cleanup)
- Operator visibility: UI-visible (in preview and raw document)
- Rerun safety: unsafe (each run adds another duplicate table)

Adjacent failure modes:
- WFG-004 (Stale/Corrupt Display)
- DAT-006 (Incorrect Normalization)

Recommended mitigation:
- Trim trailing whitespace/newlines before evaluating `ends_with("<!-- gosling:output-history:end -->")`.
- In Rust: Normalize CRLF or search for the `HISTORY_START` and `HISTORY_END` tokens via regex or token-based splitting rather than rigid literal suffix matching.
- In TypeScript: Apply `.trimEnd()` before checking `.endsWith('<!-- gosling:output-history:end -->')`.

Implementation assessment:
- Complexity: local_guardrail
- Cost: XS
- Cost drivers: modules, tests
- Nominal implementation agent: codex / rust-engineer
- Rationale: Self-contained string parsing utility in `output_revisions.rs` and `OutputHistory.tsx`.

Validation:
- Unit test with `\r\n` line endings, trailing `\n\n`, and trailing spaces confirming `markdown_body` cleanly extracts the original content.

Non-goals:
- Changing the table formatting or columns.

---

### DAT-GSL-002: Split transactions in `restore_output_revision` leave orphaned Baseline records on filesystem or lock failure

Severity: Medium  
Confidence: Confirmed  
Evidence basis: source-evidenced  
Domain: Data-Integrity  

Evidence:
- `crates/gosling/src/session/session_manager/output_revisions_storage.rs:433-487`:
  ```rust
  let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
  let history = history_in_tx(&mut tx, &path).await?;
  // ...
  if history.last().is_none_or(|last| last.content_hash != digest(&current.body)) {
      let baseline = revision(...);
      insert_revision(&mut tx, &path, &baseline, &current.bytes).await?;
  }
  tx.commit().await?; // <--- First transaction committed here!

  let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
  // ...
  insert_revision(&mut tx, &path, &next, &bytes).await?;
  tokio::task::spawn_blocking(move || {
      replace_if_unchanged(&session, &path, &expected, &bytes) // <--- Can fail!
  }).await??;
  tx.commit().await?; // <--- Second transaction committed here!
  ```

Observed behavior:
- `restore_output_revision` splits its state changes into two consecutive transactions. The baseline revision is permanently committed in Transaction 1 before file replacement is attempted in Transaction 2.

Expected boundary:
- State transitions representing an atomic restore must roll back completely if the filesystem write fails.

Failure mechanism:
- If `replace_if_unchanged` fails (e.g. concurrent modification, permissions error, full disk, locked file):
  - Transaction 2 is dropped and rolled back (the `Restored` revision is not saved).
  - Transaction 1 remains committed in SQLite.
- The database now contains a `Baseline` revision with action `Baseline` and attribution `Unknown`, even though no restore took place.

Break-it angle:
- Make the destination file unwritable (`chmod 444`) or change its content concurrently during the restore dialog prompt, then click "Restore". The operation fails with an error, but the history table now shows a new phantom version.

Impact:
- Orphaned database records; inaccurate provenance history in SQLite that does not correspond to any actual restored file state.

Operational impact:
- Blast radius: Local (session database)
- Side-effect class: DB
- Reversibility: compensatable (manual row deletion)
- Operator visibility: UI-visible (extra baseline row appears in history)
- Rerun safety: safe

Adjacent failure modes:
- REL-008 (Non-Atomic Recovery)
- DAT-003 (Orphaned Record)

Recommended mitigation:
- Keep the `Baseline` insertion, `Restored` insertion, and filesystem update within a single atomic flow or verify `replace_if_unchanged` before committing either revision. Alternatively, if `replace_if_unchanged` fails, execute a rollback deletion of the uncommitted baseline.

Implementation assessment:
- Complexity: persistence_recovery
- Cost: S
- Cost drivers: SQLite transaction management, error recovery
- Nominal implementation agent: codex / rust-engineer
- Rationale: Single file `output_revisions_storage.rs`.

Validation:
- Test: Inject write failure during restore; verify `output_revisions` count remains unchanged.

Non-goals:
- Redesigning the entire revision storage engine.

---

### WFG-GSL-001: ArtifactPane output list does not clear trashed items immediately from view without refresh

Severity: Medium  
Confidence: Confirmed  
Evidence basis: source-evidenced  
Domain: Workflow-GUI  

Evidence:
- `ui/desktop/src/components/artifacts/ArtifactPane.tsx:983-989` vs `1058-1062`:
  ```typescript
  // Research Library: explicitly prunes state
  onDeleted={(paths) => {
    forgetTrashedFiles(paths);
    setResearchLibraryFiles((files) =>
      files.filter((file) => !paths.includes(file.path))
    );
    void refreshResearchLibrary(true);
  }}

  // Outputs list: does NOT prune displayed state
  onDeleted={(paths) => {
    forgetTrashedFiles(paths);
    void refreshResearchLibrary(true);
  }}
  ```
- `ui/desktop/src/contexts/ArtifactWorkbenchContext.tsx:138-144, 303-310`:
  ```typescript
  const artifacts = useMemo(
    () =>
      (artifactsBySession[visibleSessionId] ?? EMPTY_ARTIFACTS).filter(
        (artifact) => deletedArtifacts?.[artifact.resolvedPath] !== artifact.lastSeenAt
      ),
    [artifactsBySession, visibleSessionId, deletedArtifacts]
  );
  ```

Observed behavior:
- In the session outputs view, when a file is moved to Trash, `onDeleted` calls `forgetTrashedFiles(paths)` and triggers `refreshResearchLibrary(true)` (which refreshes the library, not the session outputs!).
- `forgetTrashedFiles` marks `deletedArtifacts[path] = artifact.lastSeenAt`. But `artifactsBySession` is not re-fetched or filtered locally. If the file's path does not match `artifact.resolvedPath` exactly (e.g. symlink vs canonical path) or if `lastSeenAt` has microsecond mismatches, the item remains visible in the outputs list until the chat session changes.

Expected boundary:
- Trashed files must disappear immediately from the active session's outputs list upon successful deletion acknowledgment.

Failure mechanism:
- The outputs list callback omits a direct session artifact state filter and instead calls `refreshResearchLibrary` (a no-op for session outputs).

Break-it angle:
- Delete an output file from the "Outputs" pane in the Desktop app. Observe that the row continues to be displayed in the list until switching chats or triggering an agent tool turn.

Impact:
- Operator confusion; operator attempts to delete or click the item again, triggering repeated errors ("File was already missing").

Operational impact:
- Blast radius: Workflow (UI)
- Side-effect class: user-visible
- Reversibility: reversible
- Operator visibility: UI-visible
- Rerun safety: safe

Adjacent failure modes:
- WFG-004 (Stale Display)
- REL-003 (Silent Degradation)

Recommended mitigation:
- In `ArtifactPane.tsx:1058`, update the session artifacts state or dispatch an artifact refresh event for `visibleSessionId`.

Implementation assessment:
- Complexity: operator_ux
- Cost: XS
- Cost drivers: UI state wiring
- Nominal implementation agent: claude / react-engineer
- Rationale: Single callback fix in `ArtifactPane.tsx`.

Validation:
- UI component test: verify that triggering `onDeleted` removes the items from the rendered DOM list immediately.

---

### WFG-GSL-002: Preference `autoCompactReduction` allows settings $\ge$ threshold, causing silent reduction fallback

Severity: Low  
Confidence: Confirmed  
Evidence basis: source-evidenced  
Domain: Workflow-GUI  

Evidence:
- `crates/gosling/src/acp/server/config.rs:326-329`:
  ```rust
  if !reduction.is_finite() || !(0.0..1.0).contains(&reduction) {
      return Err(agent_client_protocol::Error::invalid_params()
          .data("autoCompactReduction must be at least 0 and less than 1"));
  }
  ```
- `crates/gosling/src/context_mgmt/mod.rs:563-568`:
  ```rust
  if reduction <= 0.0 || reduction >= threshold {
      return Ok(None);
  }
  ```

Observed behavior:
- An operator can successfully save `autoCompactReduction = 0.8` through the ACP preference API while `autoCompactThreshold = 0.7`.
- The ACP server accepts the value without error.
- Later, when compaction triggers, `auto_compact_reduction_budget` discovers `reduction >= threshold` and silently disables the reduction, falling back to full compaction.

Expected boundary:
- Preference validation should cross-validate against the effective `threshold` (or warn the user), rather than accepting an invalid combination that silently disables the feature.

Failure mechanism:
- Decoupled parameter validation: `config.rs` checks only `0.0..1.0` independently of threshold.

Break-it angle:
- Set `autoCompactThreshold: 0.6` and `autoCompactReduction: 0.6`. No error is returned, but incremental compaction never executes.

Impact:
- Silent departure from user configuration; operator expects soft incremental compaction but receives full context collapse.

Operational impact:
- Blast radius: Local
- Side-effect class: none
- Reversibility: reversible
- Operator visibility: silent
- Rerun safety: safe

Adjacent failure modes:
- REL-014 (Fragile Default Config)
- WFG-013 (Operator Cannot Diagnose)

Recommended mitigation:
- Validate `reduction < threshold` in `prepare_auto_compact_reduction` when threshold is known, or emit a trace/debug log when falling back.

Implementation assessment:
- Complexity: local_guardrail
- Cost: XS
- Nominal implementation agent: codex / rust-engineer
- Rationale: 5-line check in `crates/gosling/src/acp/server/config.rs`.

---

### REL-GSL-001: Trashing missing files acknowledges missing status in UI but fails to prune `session_artifacts` in database

Severity: Medium  
Confidence: Confirmed  
Evidence basis: source-evidenced  
Domain: Reliability  

Evidence:
- `ui/desktop/src/main/fileIpc.ts:547`:
  ```typescript
  if (!requestedFile) {
    results.push({ path: filePath, status: 'missing' });
    continue;
  }
  ```
- `ui/desktop/src/components/artifacts/ArtifactFileList.tsx:170-174`:
  ```typescript
  const removed = results
    .filter((result) => result.status !== 'failed')
    .map((result) => result.path);
  if (removed.length > 0) onDeleted(removed);
  ```

Observed behavior:
- When a file was deleted externally, `trash-artifact-files` notes `status: 'missing'`.
- The UI treats `'missing'` as successfully removed (`status !== 'failed'`) and hides it from the current view.
- However, no database update is dispatched to delete or mark the file as deleted in `session_artifacts`.

Expected boundary:
- Acknowledged missing artifacts should be reconciled against the session's artifact registry so they do not persist as ghost entries.

Failure mechanism:
- Missing-file detection is confined to the Electron main process IPC and renderer UI state. The SQLite backend is never notified of the deletion.

Break-it angle:
- Manually delete a file generated by an agent from the command line (`rm Outputs/test.txt`). In Desktop UI, select it and click "Delete selected". UI confirms deletion. Close and reopen the desktop app or reload the chat: `test.txt` is back in the list.

Impact:
- Persistent phantom files in session history across restarts.

Operational impact:
- Blast radius: Workflow
- Side-effect class: DB
- Reversibility: reversible
- Operator visibility: UI-visible (resurfaces after app reload)
- Rerun safety: safe

Adjacent failure modes:
- DAT-003 (Orphaned Record)
- WFG-004 (Stale Display)

Recommended mitigation:
- When `trash-artifact-files` returns `'missing'` or `'trashed'`, invoke an ACP or IPC handler to prune `session_artifacts` for the session.

Implementation assessment:
- Complexity: workflow_protocol
- Cost: S
- Nominal implementation agent: claude / full-stack
- Rationale: Crosses Electron IPC and ACP session management.

---

### REL-GSL-002: Full 20 MiB in-memory buffering and string decoding during `copy-artifact-contents`

Severity: Low  
Confidence: Likely  
Evidence basis: source-evidenced  
Domain: Reliability  

Evidence:
- `ui/desktop/src/main/fileIpc.ts:348-371`:
  ```typescript
  const buffer = Buffer.alloc(stats.size + 1);
  // reads full 20 MiB into buffer
  const contents = buffer.subarray(0, total);
  if (contents.includes(0)) throw new Error('Copy contents supports UTF-8 text files only.');
  let text: string;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(contents);
  } catch {
    throw new Error('Copy contents supports UTF-8 text files only.');
  }
  clipboard.writeText(text);
  ```

Observed behavior:
- Reads the entire file up to 20 MiB into a Node `Buffer`, scans the full buffer for null bytes, decodes it into a V8 `string` in RAM, and then copies to system clipboard.

Expected boundary:
- Large file validation should fail fast on binary detection without allocating full-sized buffers if the file is not text.

Failure mechanism:
- Scans for null bytes only *after* reading the entire file into memory. A 20 MiB binary file forces a 20 MiB buffer allocation before being rejected.

Break-it angle:
- Copy a 20 MiB binary file. The process allocates 20 MiB in Node buffer space before throwing an error on line 364.

Impact:
- Transient memory spike in Electron main process.

Operational impact:
- Blast radius: Local
- Side-effect class: process
- Reversibility: reversible
- Operator visibility: silent
- Rerun safety: safe

Recommended mitigation:
- Read the first 4–8 KB chunk first to check for null bytes (`contents.includes(0)`). If binary, fail immediately before reading the remaining 20 MiB.

Implementation assessment:
- Complexity: local_guardrail
- Cost: XS
- Nominal implementation agent: codex / node-engineer

---

### REL-GSL-003: Redundant double path canonicalization per mutation target in `out_of_scope_path`

Severity: Low  
Confidence: Confirmed  
Evidence basis: source-evidenced  
Domain: Reliability  

Evidence:
- `crates/gosling/src/permission/working_dir_scope_inspector.rs:913, 254`:
  ```rust
  let canonical_path = canonicalize_potential_path(resolved)?;
  // ...
  if !is_within_any(resolved, allowed_dirs)? { // <--- is_within_any calls canonicalize_potential_path AGAIN!
      return Ok(Some(canonical_path));
  }
  ```

Observed behavior:
- `out_of_scope_path` resolves and canonicalizes `resolved` to `canonical_path`, then passes `resolved` into `is_within_any`, which redundantly invokes `canonicalize_potential_path` a second time on the same path.

Expected boundary:
- Path canonicalization involves disk I/O and symlink resolution; it should be performed once per path.

Failure mechanism:
- Helper function `is_within_any` re-canonicalizes its input rather than accepting an already-canonical path slice.

Impact:
- Unnecessary filesystem stat and canonicalization calls during permission inspection loops.

Operational impact:
- Blast radius: Local
- Side-effect class: none
- Reversibility: reversible
- Operator visibility: silent
- Rerun safety: safe

Recommended mitigation:
- Update `is_within_any` or introduce `is_canonical_within_any(&canonical_path, canonical_dirs)` to reuse the already canonicalized path.

Implementation assessment:
- Complexity: local_guardrail
- Cost: XS
- Nominal implementation agent: codex / rust-engineer

---

## 4. Confirmed Non-Findings & Resilient Architectures

The following mechanisms touched today were actively probed for failure and confirmed resilient:

1. **Auto-Compaction Cutoff Boundaries (`REL-011 Partial Output Treated Success` / `WFG-009`)**:
   - Probed: Does `budget_capped_compact_end` split inside a turn or leave orphaned tool calls/responses?
   - Result: **Non-finding**. `budget_capped_compact_end` keys strictly on `turn_starts` indices derived from `is_turn_start(msg)`. It always steps by entire turn boundaries. The protected tail (`protect_last_n_turns`) is preserved with real tool calls and messages rather than reconstructed placeholders.
2. **Context Compaction Failure Recovery (`REL-008 Non-Atomic Recovery`)**:
   - Probed: Does an unexpected provider failure during auto-compaction leave the session corrupted or conversation truncated?
   - Result: **Non-finding**. If `perform_compact` errors out, the stream yields a failure notification message and exits without overwriting `session_manager.replace_conversation()`. The turn lease drops cleanly upon stream termination.
3. **Optimistic Concurrency on Output File Restore (`DAT-005 Corrupt Merge` / `DAT-007`)**:
   - Probed: Can a restore overwrite concurrent changes made to the target file while the modal was open?
   - Result: **Non-finding**. Both `restore_output_revision` and `replace_if_unchanged` enforce SHA-256 hash assertions against `expected_current_hash` before writing to a temporary file, and re-check hash before persisting atomically.
4. **Symlink Traversal in Output Revision Tracking (`DAT-001 Scope Leakage`)**:
   - Probed: Can a symlink inside `Outputs/` point to an outside sensitive file and allow revision extraction?
   - Result: **Non-finding**. `read_snapshot` and `canonical_output_path` explicitly check `!metadata.file_type().is_symlink()`, use `libc::O_NOFOLLOW | libc::O_NONBLOCK` on Unix, and verify `path.canonicalize()? == path`.
5. **UI Layering & Modal Dismissal (`WFG-006 Destructive Ambiguity`)**:
   - Probed: Do new confirmation dialogs or tooltips block interaction or clip off-screen?
   - Result: **Non-finding**. `Z_INDEX.OVERLAY` (10,000) and `Z_INDEX.DROPDOWN_ABOVE_OVERLAY` (10,001) establish clean stacking order. Confirmation modals encapsulate their scrollable lists within `max-h-60 overflow-y-auto`.

---

## 5. Recommended Patch Order

To address the findings systematically, follow this priority sequence:

1. **Step 1: Fix DAT-GSL-001 (High)**:
   - Make `output_revisions.rs:markdown_body()` and `OutputHistory.tsx:textPreview()` resilient to `\r\n` and trailing whitespace to prevent document history accumulation.
2. **Step 2: Fix DAT-GSL-002 & REL-GSL-001 (Medium)**:
   - Unify the baseline insertion with the restore transaction in `output_revisions_storage.rs` so failed restores do not leave orphaned baselines.
   - Dispatch `session_artifacts` cleanup when `trash-artifact-files` identifies missing files.
3. **Step 3: Fix WFG-GSL-001 (Medium)**:
   - Ensure `onDeleted` in `ArtifactPane.tsx` updates `displayedArtifacts` immediately.
4. **Step 4: Minor Guardrails (Low)**:
   - Add early binary check in `fileIpc.ts:copy-artifact-contents` (REL-GSL-002).
   - Eliminate redundant path canonicalization in `out_of_scope_path` (REL-GSL-003).
   - Cross-validate reduction vs threshold in `prepare_auto_compact_reduction` (WFG-GSL-002).

---

## 6. Verification Limits

- All findings were evidenced directly from static inspection of the current git tree (`main`, commit `687022855`).
- Live GUI interaction was simulated by analyzing React component state flows and Electron IPC handlers.
- Read-only operating constraints were observed throughout; no production files or git branches were mutated during this audit.

---

## 7. Repair disposition — 2026-09-08

This dated addendum supersedes the actionable status of the findings above; the original
static audit observations remain historical evidence. Source patches and scoped regression
checks are recorded in [the repair session](../logs/session/2026-09-08-system-surface-repairs.md).

| Finding | Current disposition | Evidence |
| --- | --- | --- |
| DAT-GSL-001 | Resolved | CRLF/LF and trailing-whitespace footer recognition in Rust and Desktop; exact body/export bytes retained; output-history integration, parser and preview regressions pass. |
| DAT-GSL-002 | Resolved | Baseline and restore revisions share one transaction; filesystem write-failure regression leaves history unchanged, including after storage reopen. |
| WFG-GSL-001 | Closed — not a defect | Existing reactive workbench filtering already removes acknowledged rows. Rendered ArtifactPane tests pass for both trashed and missing results. Requested paths are echoed unchanged and version strings are not reformatted. |
| REL-GSL-001 | Closed — not a defect | Accepted ADR-0013 deliberately retains database provenance. Persistent version dismissal survives remount, missing results follow the same dismissal path, and regenerated versions reappear. Database pruning would violate that contract. |
| WFG-GSL-002 | Resolved | ACP validates resulting threshold/reduction pairs before saves and resets; defaults, single/batch updates, rejection without persistence, and zero reduction pass targeted tests. |
| REL-GSL-002 | Resolved | Copy reads/decode use a reusable buffer capped at 64 KiB. A 20 MiB binary fails after one read; split/incomplete UTF-8 and clipboard all-or-nothing regressions pass. |
| REL-GSL-003 | Resolved | Scope membership receives the already-canonical target; 32 scope unit tests and 6 scratch integration tests pass. |

Validation is scoped source/test validation, not a new full audit or packaged-app playtest.
No security scan was run. SQLite/filesystem crash atomicity remains the explicit ADR-0018
limitation; valid clipboard text still requires a complete final string.
