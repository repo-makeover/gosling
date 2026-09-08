---
title: Workspaces
sidebar_position: 31
sidebar_label: Workspaces
description: Create repeatable folders, output destinations, and secure provider profiles for gosling Desktop sessions
---

# Workspaces

:::info Desktop only
Workspace management is currently available in gosling Desktop. The CLI continues to use its
current working directory and global provider configuration.
:::

A workspace is a repeatable environment for new chats. It can define:

- one primary working folder;
- additional source or reference folders with read-only or read/write guidance;
- named product output folders for documents, spreadsheets, presentations, images, video, code,
  data, exports, or other deliverables;
- a secure credential-profile binding;
- optional default provider and model identifiers.

gosling creates a usable `Default` workspace automatically. Existing sessions remain valid and do
not need to be migrated manually.

## Start or filter workspace chats

Open the sidebar and expand **Workspaces**.

- Click a workspace row, or choose **Show its chats** from its menu, to filter the existing chat list.
- Choose the `+` action next to a workspace, or **New chat in this workspace** from its menu, to open New Chat with that workspace preselected.
- The global **New Chat** action preselects the active workspace, falling back to the default workspace. You can override it before starting the chat.
- Choose **All workspaces** to see sessions across every workspace.

Filtering workspaces does not move or restart the chat currently on screen and does not change
future-chat defaults. The chat header continues to show the session's pinned workspace.

A green dot beside a workspace means at least one of its chats has a new reply, matching the
green dot on the chat itself. It remains visible when you filter to another workspace or collapse
the Chats list. Selecting a workspace only filters the list; open each ready chat from the sidebar
to clear its indicator. The workspace dot clears after the last ready chat is opened, archived, or deleted.

The chat composer also exposes a credential-profile control. In New Chat, use it to choose from
available profiles or open **Manage credential profiles**. In an existing chat, it identifies the
pinned profile and keeps a deleted or unavailable profile visible as a relink problem; it does not
silently switch the session to another account.

## Create a workspace

1. Select **Add workspace** next to the Workspaces heading.
2. Enter a name and, optionally, a description, icon label, provider, and model.
3. Choose the primary working folder.
4. Add source or reference folders as needed and choose **Read only** or **Read/write** for each.
5. Add one or more product output destinations. Assign product types and select exactly one default
   output.
6. Optionally add a credential binding.
7. Select **Validate**, resolve any required errors, and save.

The primary folder must exist and be a directory before a new chat can start. A missing optional
reference or output folder produces a warning instead of disabling the entire app. If an output is
marked **Allow explicit creation if missing**, edit the saved workspace and choose **Create now**;
gosling asks for confirmation before creating it.

Removing a folder from a workspace removes only the reference. It never deletes, moves, or rewrites
the physical folder.

## Manage secure credential profiles

From the workspace editor, open **Manage credential profiles**.

1. Choose **New profile**.
2. Select a provider and enter a profile name.
3. Enter the provider's required values. Secret fields use password inputs.
4. Save the profile, then add or update the workspace's credential binding.

After save, gosling displays only metadata and status. It never sends the stored value back to the
Desktop renderer. Editing a profile shows `Configured — enter a replacement` instead of the old
value. Canceling, succeeding, or failing clears secret form values.

Credential values use gosling's existing OS keyring by default. If the keyring is unavailable,
gosling uses its existing owner-protected `secrets.yaml` fallback. Workspace metadata stores only a
profile UUID and configured-field metadata, using internal secure identifiers shaped like
`workspace-credential::<profile UUID>::<field>`.

Deleting a referenced profile requires confirmation and leaves affected workspaces in a visible
relink-required state. gosling does not silently substitute another credential. Global-migration
aliases and distribution-managed profiles are read-only in the workspace profile manager; create a
new local profile and relink the workspace when you need different values.

## Session behavior

When a new chat starts, the backend:

1. validates the selected workspace and primary folder;
2. resolves the selected credential profile through secure storage;
3. pins the workspace/profile references, effective working folder, workspace name, and non-secret
   folder/output snapshot to the new session;
4. gives the agent the workspace's non-secret folder and output context.

Resuming a session uses that pinned snapshot and profile reference, not whichever workspace is
active now. Deleting a workspace preserves its sessions and files; historical sessions continue to
show the saved workspace name. A missing profile produces a relink error instead of falling back to
another account.

Legacy sessions without a workspace ID remain resumable and appear with the default/unassigned
session behavior. Session search still works across workspaces when **All workspaces** is selected.

Temporary scratch files can be used without a workspace-scope approval when **Restrict tools to
working directories** is off. This includes the system temp folder, Unix `/tmp` and `/var/tmp`,
and their macOS aliases. Writes elsewhere still follow the workspace folder rules. Turning the
restriction on also restricts scratch access; read-only workspace folders remain protected.

## Product outputs and exports

The agent receives named output paths and product types as structured, non-secret session context.
Use specific destinations for their matching product types and the default destination otherwise.

The **Outputs** pane also shows a durable inventory for the visible session. Entries from
successful write/edit tools, local tool resources, explicit output metadata, and completed assistant
file references appear automatically as `Outputs N`; clicking a message chip is not required. The list
uses the extensions selected in **Settings → App → Output files**. The defaults are `.pdf`, `.md`,
`.txt`, `.doc`, `.docx`, `.jpg`, `.png`, `.yaml`, and `.json`. Add extensions such as `.rs`, `.ts`,
`.py`, `.sh`, or `.toml` to include code/configuration files. Files without an in-app preview can
still appear for reveal or external opening. Switching chats immediately switches the list. Missing
files that match the display filters remain named after restart or resume so they cannot be confused
with another file that shares a basename.

Inventory and preview are intentionally separate. Discovering an output does not open the pane, read
the file, create an output directory, or grant access. Selecting an item opens a session-scoped preview
tab through the same Electron authorization boundary used by **Reveal**, **Open externally**, and
**Save a copy**. Supported session-generated deliverables such as JSON, Markdown, PDF, and plain text
can be previewed directly through an exact-file capability. A file outside the session's approved
roots or validated workspace outputs remains blocked unless you
explicitly select it with the file picker; code, configuration (including `.env`), and MCP/tool
metadata never receive that automatic capability.

### Filter repository files

In the Outputs pane, enable **Hide repository files** to hide:

- recognized source-code files, such as `.rs`, `.ts`, `.py`, and `.sh`;
- recognized project files, such as `Cargo.toml`, `package.json`, and `requirements.txt`;
- files inside Git, Mercurial, or Subversion repository directories, including Git worktrees.

The switch is off by default and remembers its setting. It applies after the extension filter and
updates the visible list/count, with a separate count of hidden entries. A report saved inside a
repository is hidden too; turn the switch off to see it again. Ordinary documents and data outside
repositories remain visible unless their filename identifies a project file.

Repository membership is checked only for files gosling is authorized to inspect. If it cannot be
checked, the pane shows a status and keeps those entries visible unless their filename already
identifies source/project content. Filtering preserves files, stored inventory, and open preview tabs.
It changes presentation only; it does not change file access or the agent's working folders.

### File timestamps

Rows in both **Outputs** and **Library** show **Created** and **Modified** dates and times
from the filesystem. They use your locale and local timezone, including seconds; hovering a
timestamp also shows its timezone. These are the file's timestamps, not the time it was mentioned
in a chat or added to the inventory. Copies in Outputs and Library can therefore have different times.

Timestamps refresh when the list opens, its file versions change, or the app regains focus.
If creation time is unavailable, **Created: Unavailable** is shown. Missing or inaccessible files
remain listed with **File timestamps unavailable**. Reading timestamps does not open the file or
grant additional access.

### Save, export, and download

gosling's artifact router applies the same rule to every Desktop-owned save, export, and native
download. It recognizes documents, spreadsheets, presentations, images, video, code, data,
exports, and other files. Workspace and session exports use an `export` destination; **Save a
copy** in the Outputs pane uses the artifact's type; browser-style downloads are placed directly in
the matching folder with a collision-safe name. A save dialog still lets you deliberately choose a
different location.

Artifacts opened from a chat retain that session's pinned workspace, even after the active
workspace changes. An unavailable or deleted pinned workspace never silently redirects the save to
another workspace. Missing outputs show a relink error, or—when **Allow explicit creation if
missing** is enabled—ask before creating the directory. If a native download cannot be routed,
gosling shows a warning instead of silently claiming it used the workspace. Rapid workspace
switches are ordered so a slower validation of an older selection cannot restore its download
destination.

The router never moves an already-generated file. **Save a copy** copies the complete source file,
not a truncated preview. Direct absolute-path writes performed inside an independent third-party
extension or agent tool remain controlled by that tool and the user's permissions; app updates,
automatic transcript archives, and configuration files keep their dedicated storage locations.

## Workspace actions

Open the actions menu next to a workspace to:

- open/switch or filter its chats;
- edit or duplicate it;
- reveal its primary folder;
- export non-secret metadata;
- delete it.

The `Default` workspace cannot be deleted. Deleting another workspace preserves all sessions and
files. Exports never include credential values, keyring identifiers, or stored provider tokens.

## Persistence and recovery

The backend is the source of truth. Workspace metadata and the active workspace are stored in the
gosling data directory at `workspaces/workspaces.json` with schema version 1. Writes use an
owner-only directory/file, a lock, fsync, and atomic rename. Benign unknown future fields are
preserved; secret-shaped unknown fields are rejected.

If the current store is malformed, gosling preserves it as an owner-only
`workspaces.corrupt-<id>.json` recovery file and creates a usable `Default` workspace. It never reads
or modifies an upstream Goose workspace, config directory, or keyring namespace.

Only harmless UI preferences—the section's collapsed state and chat filter—use browser local
storage. Workspace definitions and credentials do not.

## Troubleshooting

| Symptom                                    | What to do                                                                                       |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| Primary folder warning                     | Edit the workspace and choose a current directory. A new chat cannot start until it is relinked. |
| Optional folder warning                    | Relink or remove the reference; other workspace features remain usable.                          |
| Credential missing or needs authentication | Create/update a local secure profile and relink the default binding.                             |
| Deleted profile on resume                  | Relink the workspace/session dependency; gosling does not choose another profile automatically.  |
| Output folder missing                      | Enable explicit creation, save, then choose **Create now**, or select an existing folder.        |
| Chats seem hidden                          | Select **All workspaces** in the Workspaces section.                                             |

## Current limits

- Workspace definitions are local only; cloud/team synchronization is not included.
- Extension defaults are not stored per workspace because current extension configuration is not
  cleanly session-scoped.
- An independent third-party tool that writes directly to an explicit absolute path does not pass
  through the Desktop save/download router; Gosling-owned export, Outputs, and download surfaces do.
- Credential network testing is reported as unsupported unless a provider exposes a safe validation
  hook; configured status currently proves required secure values are present, not that a remote
  provider accepted them.
- The workspace manager does not clone repositories, create Git worktrees, or move existing files.
