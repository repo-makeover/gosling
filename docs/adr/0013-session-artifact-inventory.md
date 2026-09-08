# ADR-0013: Durable session artifact inventory

Date: 2026-08-13
Status: accepted
Related: ADR-0005, ADR-0006

## Context

The Outputs workbench treated user-opened preview tabs as the artifact list. Files mentioned by a
successful tool or completed assistant response therefore remained invisible until the user clicked a
message chip, and global tab persistence could mix preview state across sessions. Scanning output
directories would be broader than conversation provenance and would make filesystem contents an
implicit authority source.

## Decision

Gosling owns a metadata-only, session-scoped inventory in SQLite schema v26. Discovery accepts exact
successful built-in write/edit targets first, local MCP resource links and embedded resources, explicit
tool artifact metadata, conservative output arguments from successful mutating tools, and completed
assistant Markdown file references. A unique session/resolved-path key makes updates idempotent and
retains the strongest provenance. Legacy sessions receive a one-time persisted-message backfill; no
directory scan occurs.

ACP exposes paginated `_gosling/unstable/session/artifacts/list` and an `artifact_update` session
notification sent only after storage succeeds. Older backends may be reconstructed conservatively from
already-loaded trusted assistant messages. The Desktop session store owns inventory state. The Outputs
workbench projects entries whose file extensions match the user's persisted display list, and keeps
that list separate from session-scoped preview tabs and selection; pane width and visibility remain
window preferences. The default display list is `.pdf`, `.md`, `.txt`, `.doc`, `.docx`, `.jpg`, `.png`,
`.yaml`, and `.json`. Files without an in-app renderer remain available for reveal and external opening.
The durable metadata remains intact even when an entry is not presented.

Inventory registration grants no filesystem capability. Relative paths retain their discovery working
directory, but selection still passes through the Electron artifact guard. Existing renderer roots,
validated workspace output roots, explicit file-picker grants, and exact session-generated user-facing
deliverables authorize a read/open/reveal/copy. The last category is limited to document-like files
created or modified by a built-in tool or referenced by the assistant; it never grants a directory or
authorizes code, configuration, or MCP/tool-metadata paths.

## Consequences

Outputs populate without click-driven side effects and survive restart, resume, and fork. Missing
files with a configured extension stay named, while other entries are omitted from the presented list
and count. Common code/config extensions receive a code preview kind when the user adds them to the
display list.
Files are never created, copied, moved, opened, or read merely because a record exists. Files created
by arbitrary shell commands and never referenced were initially undiscovered. ADR-0018 adds a
separate bounded observer around hosted mutating tools; inventory listing and legacy backfill remain
metadata-only.

## Explicit file deletion (2026-09-08)

Outputs support row deletion and checkbox-based batch deletion. After one confirmation naming
the selected paths, Electron applies the existing per-window artifact guard and moves authorized
regular files to the operating-system Trash. Directories and symbolic links are rejected; an OS
Trash failure never falls back to permanent deletion. Each file has its own result, so failed
items remain visible with an error and successful items close their previews.

Backend discovery metadata is retained as provenance. Desktop persists dismissal of the deleted
artifact's `resolvedPath`/`lastSeenAt` version in its session presentation state; a later inventory
version at the same path can appear again. Research Library deletion reflects the actual directory
contents and leaves separate copies elsewhere intact. Merely listing a file still grants no
additional filesystem authority.

## Repository display filter (2026-09-08)

The Desktop Outputs pane has a `Hide repository files` switch, off by default and persisted
with the workbench's window preferences. It further filters the configured extension list,
hiding recognized source-code/project filenames and files beneath `.git`, `.hg`, or `.svn`
markers. A `.git` file also identifies a worktree. The displayed count and hidden count
reflect this filter; existing preview tabs and durable artifact metadata remain intact.

Repository classification checks ancestor marker metadata in batches of at most 200 paths,
after the existing per-window artifact guard resolves and authorizes each file. It does not
read marker contents, scan directory listings, invoke Git, or grant file access. Paths whose
repository membership cannot be checked stay visible with an explanatory status. Ordinary
data/document extensions outside repositories are retained unless their filename identifies
a project file, such as `package.json` or `requirements.txt`.

## Filesystem timestamps (2026-09-08)

Outputs and Research Library share a file-row timestamp display. Electron returns filesystem
birth time and modification time through a typed metadata-only IPC, capped at 200 paths per
request and guarded by the existing per-window artifact authorization. Directories, missing
files, and denied paths return unavailable metadata. A zero birth time is unavailable, never
substituted with inode-change time or session discovery time.

The renderer formats both timestamps in the user's locale/local timezone with seconds and a
timezone tooltip. Metadata refreshes on list mount, inventory/file-version changes, and window
focus, including a refresh after native artifact capabilities finish applying. Requests are batched;
late responses from earlier list versions or refreshes are ignored.
No timestamp is persisted into artifact provenance and no file-content read or new grant occurs.

## Explicit contents copy (2026-09-08)

The preview toolbar offers separate Copy contents and Copy path actions. File-content copying is
an explicit, per-window authorized main-process operation through the same artifact guard. It reads
the complete UTF-8 file, independently of preview truncation, and writes directly to the native
clipboard without sending another full copy through renderer IPC. Reads are bounded to 20 MiB;
oversized files, non-text bytes, changed files, and access failures leave the clipboard untouched.
Text artifacts supplied inline use the existing text clipboard bridge. The action is disabled for
image, PDF, and unsupported document previews, and reports completion or failure.
