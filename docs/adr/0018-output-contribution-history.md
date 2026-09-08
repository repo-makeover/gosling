# ADR-0018: Output contribution history and saved revisions

Date: 2026-09-08
Status: accepted
Related: ADR-0013

## Decision

The core owns a separate output revision service in SQLite schema v32. Each canonical file path
has increasing revision numbers, immutable saved bytes, a content hash, recording time, action,
attribution kind, and the contributing chat, agent, provider, selected model, and resolved model.
Model identity comes from the tool-request message checkpoint, including after provider failover;
changing the model picker does not rewrite earlier attribution. Missing identity remains unknown.
Delegated sessions carry their named agent identity and register their outputs in the parent chat.

Capture surrounds successful hosted mutating tools. Explicit built-in write/edit targets are
identified as tool writes; changes observed around other tools are labeled observed, not proof of
exclusive authorship. Read-only tools, references, failed tools, unchanged content, imported requests,
and the parent delegation call do not acquire authorship. Previously unrecorded contents are saved
as unknown baselines before tracked replacements or restore. Detected overlapping writes receive
unknown attribution. There is no retrospective authorship inference from the current model picker.

The observer examines explicit document targets and existing configured product-output directories,
plus `Outputs`/`outputs` beneath the working directory. It skips symlinks and hidden subdirectories.
It is bounded to four subdirectory levels, 2,000 entries, 200 documents, 32 MiB per observation,
8 MiB per saved file, and 1,000 revisions per path. Supported extensions are Markdown, plain text,
CSV/TSV, PDF, DOC/DOCX, RTF, ODT, XLSX, PPTX, HTML, PNG, JPEG, SVG, and WebP. A bound or storage failure
is reported in the successful tool result; it does not turn a completed tool into a retryable failure.

Markdown files in these output directories receive a managed contribution-history footer. Other
formats and explicit document targets outside output directories retain their contents. Saved
snapshots include their historical footer. Content equality ignores the managed footer, so replacing
only that footer or rewriting unchanged content does not invent a contributor.

## Access and presentation

Typed ACP endpoints expose paginated history, individual saved revisions, and restore. Every request
requires a current session inventory entry and a path within the session's authorized folders.
Restore additionally requires write access. Inventory registration alone grants no new access.
The metadata-only inventory list and legacy message backfill retain the ADR-0013 contract; this
separate observer is the explicit bounded-observation extension anticipated by that ADR.

Outputs rows show the latest version and contributor. History shows model and chat identity,
recorded times, older pages, text comparison with the previous revision, and exact-byte export
through the native save picker. Restore requires confirmation, checks the current file hash,
preserves untracked current contents, and appends a new user-restore revision. It neither replaces
history nor silently recreates a missing file; saved bytes can still be exported while its parent
directory and session authorization remain available. Library copies have separate paths and do not
automatically inherit output history.

## Limits and retention

This is local saved revision history, not Git commits or a continuous filesystem watcher. Tools
executed outside the hosted pipeline, external edits between observations, renames, and removed
directories cannot provide complete change tracking. Concurrent external writers cannot be assigned
exclusive authorship. Hash checks and same-directory atomic replacement reject observed conflicts,
but SQLite and the filesystem do not share a transaction: a crash between replacement and commit
can leave a footer ahead of stored history. The next successful observation can reconcile content;
the UI only treats committed SQLite revisions as saved history.

Revision bytes are retained in the private session database independently of chat deletion, allowing
later authorized chats using the same path to continue its history. Deleting a chat or trashing an
output does not erase these snapshots. This initial feature has no revision-pruning UI, cross-device
sync, Git integration, or attribution backfill for old products. Limits prevent unbounded per-file
capture, but database size can grow across files and revisions.
