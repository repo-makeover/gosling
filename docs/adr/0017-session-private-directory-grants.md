# ADR-0017: Session-private directory grants

Date: 2026-08-26
Status: implemented and locally validated
Related: ADR-0003, ADR-0011

## Context

Workspace sessions pin their primary folder, reference folders, output folders, and effective
folder policy when the session starts. The launcher could add session-only directories to that
snapshot, but an active workspace session rejected the same additive operation and told the user to
edit the workspace. Editing the workspace would grant the directory to future workspace sessions,
which is broader than the requested access.

The generic Electron directory chooser also grants the selected root to every renderer operation in
the current window. That capability is useful for general file workflows but is broader than a
folder chosen only to extend one agent session.

## Decision

An active workspace session may add an existing absolute directory to its own pinned folder policy.
The server canonicalizes the path, records it as read/write in only that session's
`workspace_context_json`, updates the same row's `additional_working_dirs_json`, and refreshes only
that loaded session's extension clients. The two persisted fields are written together. If an
extension refresh fails, both fields and the extension state are rolled back to the prior session
snapshot.

The operation is additive. It cannot replace the primary workspace directory, remove a pinned
workspace root, or alter an existing root's read-only/read-write classification. The workspace
record and unrelated session rows are never updated. Ordinary new sessions created later from the
same workspace therefore do not inherit the grant. Explicit session copies retain their snapshot,
consistent with ADR-0003's existing copy semantics.

Desktop uses a purpose-specific native session-directory chooser for launcher and active-session
additional folders. Unlike the generic chooser, it does not add the selected root to the renderer's
general file-access registry. The selected path is passed to the existing session working-directory
ACP operation, whose server-owned session ID is the persistence and enforcement scope.

## Consequences

- A user can add private working material to the current workspace chat without widening the
  workspace definition or sibling chats.
- The current session's Gosling-hosted tools and extension clients can use the directory; a sibling
  session receives an out-of-scope approval result when it would modify the same path.
- Amended 2026-09-05: a workspace session only prompts for out-of-scope *mutations* unless the
  operator turns on "restrict tools to working directories", which also prompts for out-of-scope
  reads. Read-only shell segments (`cat`, `ls`, `grep`, ...) are judged separately from the
  segments that follow them in a pipeline. Read-only workspace roots are still denied outright.
- Amended 2026-09-08 at the operator's request: unrestricted workspace sessions also allow
  temporary scratch paths under the runtime's OS temp directory and Unix `/tmp` and `/var/tmp`.
  Canonicalization handles aliases such as macOS `/private/tmp`; targets escaping through
  symlinks or parent traversal are checked against their resolved destinations. The temp roots
  themselves are not included in this exception. Every mutation destination is checked, so a
  scratch write cannot conceal another out-of-scope write. Explicit directory restriction and
  read-only workspace policy still take precedence. This allowance does not persist a folder
  grant, change workspace definitions, or authorize renderer file access.
- Existing workspace folder permissions remain pinned. Removing or replacing workspace roots still
  requires starting a new session from an updated workspace.
- This is an application capability boundary, not an operating-system ACL. Separate local programs
  and providers that manage their own external tool process remain subject to their OS permissions;
  Gosling does not claim to revoke filesystem access outside its hosted tool boundary.
