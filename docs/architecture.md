# Gosling Desktop Workspaces architecture

Status: accepted design for REQ-001–REQ-030. See `docs/adr/` for decisions.

The Workspaces I/O contract that this document referenced was pinned to session
schema migration target 22 and was removed with the rest of the completed
campaign's build records; the live schema is well past that, so treat the
formats below as design intent and the code as authoritative.

## Dependency direction

```mermaid
flowchart LR
    UI[Workspace React components] --> Context[WorkspaceContext]
    ArtifactUI[Exports / Outputs / downloads] --> Router[ArtifactRouterContext]
    ArtifactUI --> ArtifactStore[Desktop session artifact state]
    ArtifactStore --> ArtifactACP[Artifact list/update ACP]
    ArtifactACP --> Sessions
    Router --> Context
    Router --> Electron[Electron save/download bridge]
    Context --> DesktopACP[desktop/acp/workspaces.ts]
    DesktopACP --> SDK[Generated Gosling SDK]
    SDK --> Handlers[ACP workspace handlers]
    Handlers --> Service[WorkspaceService]
    Service --> Domain[Canonical workspace DTO/domain rules]
    Service --> Store[WorkspaceStore]
    Service --> Validator[Workspace path validator]
    Service --> Credentials[Credential profile resolver]
    Store --> JSON[(workspaces.json)]
    Credentials --> Config[Config secure storage]
    Config --> Keyring[(OS keyring / protected fallback)]
    Handlers --> Sessions[SessionManager v31 snapshot and turn lease]
    Sessions --> DB[(sessions.db)]
    Agent[Agent / provider construction] --> Service
    Agent --> Credentials
```

Interface and adapter layers depend on application/domain contracts. The domain never
imports React, Electron, ACP transport, SQLite, keyring libraries, or provider SDK types.

## Primary workflow

```mermaid
sequenceDiagram
    actor User
    participant UI as Workspace UI
    participant API as ACP handlers
    participant WS as WorkspaceService
    participant Store as WorkspaceStore
    participant Sessions as SessionManager
    participant Agent as Agent/provider
    participant Secrets as Config secure storage

    User->>UI: Create/edit and activate workspace
    UI->>API: Typed workspace request
    API->>WS: Validate and mutate
    WS->>Store: Locked atomic write
    Store-->>UI: Canonical workspace + warnings
    User->>UI: New Chat
    UI->>API: newSession(workspaceId, cwd hint)
    API->>WS: Prepare authoritative session context
    WS->>Store: Re-read and validate paths/profile
    API->>Sessions: Create row + pinned snapshot
    API->>Agent: Activate saved session
    Agent->>WS: Resolve pinned profile scope
    WS->>Secrets: Read namespaced fields only
    Agent->>Agent: Construct provider inside scope
    API-->>UI: Session metadata with pinned workspace
```

An active workspace session may add an existing directory to its own pinned snapshot. That additive
grant updates only the selected session row and its loaded extension clients; it does not mutate the
workspace or sibling sessions. Primary-folder replacement, removal of pinned workspace roots, and
live refresh from later workspace edits remain prohibited. See ADR-0017.

## Module contracts

| Module                              | Layer               | Owns                                                                                                 | Must not own                                            | Allowed dependencies                                          | Public surface                                            |
| ----------------------------------- | ------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------- | --------------------------------------------------------- |
| `gosling-sdk-types::workspace`      | domain contract     | canonical DTOs, enums, typed requests/responses                                                      | persistence, provider construction, UI                  | serde/schemars/ACP derive                                     | workspace/profile DTOs and request types                  |
| `gosling::workspace::model`         | domain              | store envelope and internal snapshot conversions                                                     | IO, UI, keyring calls                                   | canonical DTOs                                                | internal versioned record types                           |
| `gosling::workspace::validation`    | domain/application  | normalization, folder status, template/import validation                                             | persistence or UI messages                              | std paths, canonical DTOs                                     | validation report and normalized workspace                |
| `gosling::workspace::store`         | adapter             | lock/read/migrate/atomic-write of workspace metadata                                                 | secret values, session rows, provider creation          | model, fs2, std filesystem                                    | load/mutate/export/import primitives                      |
| `gosling::workspace::credentials`   | application         | metadata lifecycle, secure key naming, scoped resolution                                             | raw keyring API, renderer response values               | store, provider registry metadata, Config                     | create/update/delete/resolve/test profile                 |
| `gosling::workspace::context`       | domain/application  | non-secret session snapshot and prompt rendering                                                     | credentials or global active state                      | canonical DTOs                                                | snapshot builder/prompt renderer                          |
| `gosling::workspace::service`       | application         | CRUD policy, active/default invariants, preparation for sessions                                     | transport/UI/SQLite details                             | store, validator, credentials, context                        | operations used by ACP and Agent                          |
| `acp::server::workspaces`           | interface           | request parsing, error mapping, response mapping                                                     | domain decisions or direct file writes                  | WorkspaceService                                              | `_gosling/unstable/workspaces/*` methods                  |
| `SessionManager` workspace fields   | adapter             | nullable session snapshot columns and queries                                                        | live workspace/profile mutation                         | canonical snapshot DTO                                        | create/copy/read/update/filter snapshot                   |
| `SessionManager` artifact inventory | adapter             | durable session/path metadata, discovery provenance, legacy message backfill                         | file copies, file reads, or renderer authorization      | persisted messages and successful tool results                | list/upsert/copy/delete artifact metadata                 |
| `ConfigResolutionScope`             | infrastructure seam | task-scoped logical config/secret resolution                                                         | workspace metadata policy                               | Config secure storage                                         | scoped async execution + typed reads                      |
| `Agent` workspace integration       | application         | use saved profile on create/recreate/resume                                                          | active-workspace selection                              | session snapshot, WorkspaceService, providers                 | fail-closed provider restore and prompt context           |
| `ui/desktop/src/acp/workspaces.ts`  | interface adapter   | generated-client calls and no domain state                                                           | persistence, local schema copies                        | generated SDK                                                 | typed async workspace/profile operations                  |
| `WorkspaceContext`                  | UI application      | observable workspace state, mutations, selection/filter derivation                                   | durable persistence or secrets                          | ACP adapter, Electron broadcast                               | required `useWorkspace` API                               |
| `components/workspaces/*`           | UI interface        | accessible presentation/forms/actions                                                                | persistence, session rules, secret retrieval            | WorkspaceContext, existing UI primitives                      | sidebar/editor/profile components                         |
| Electron workspace IPC              | adapter             | folder chooser/reveal and cross-window refresh signal                                                | workspace metadata or secrets                           | typed IPC channels                                            | existing folder APIs + change broadcast                   |
| `ArtifactRouterContext`             | UI application      | pinned/active workspace destination selection, missing-output confirmation, single save API          | durable workspace state, direct filesystem writes       | WorkspaceContext, pure resolver, Electron bridge              | `saveArtifact`, visible-session routing                   |
| Electron artifact bridge            | adapter             | save dialog, authorized full-file copy/content write, exact built-in session-output preview, revisioned validated native-download placement | workspace persistence, secret data, agent file movement | renderer file-access guard, Node filesystem, Electron session | `save-artifact`, per-window routing config, failure event |
| `ArtifactWorkbenchContext`          | UI application      | active session inventory projection plus session-scoped preview tabs/selection                       | inventory persistence or file authorization             | Desktop ACP session store, Electron artifact bridge           | inventory selection and explicit preview actions          |

## Seam catalog

| Seam                       | Extension axis                          | Mechanism                                                                    |
| -------------------------- | --------------------------------------- | ---------------------------------------------------------------------------- |
| Workspace schema evolution | future non-secret metadata              | `schema_version`, explicit migrations, top-level unknown-field preservation  |
| Credential auth shapes     | provider config fields                  | provider registry metadata + namespaced logical field resolver               |
| Folder kinds/access        | future folder policies                  | typed enums and centralized validator                                        |
| Product types              | future artifact kinds                   | typed enum set and named output selection helper                             |
| Artifact egress            | future Gosling-owned artifact producers | one `saveArtifact` call plus revision-guarded native `will-download` routing |
| Artifact discovery         | future tool/result shapes               | ordered metadata-only discovery with session/path idempotency                |
| Custom distributions       | additional templates                    | non-secret config template array resolved only on first initialization       |
| Multi-window UI            | additional Desktop windows              | backend source of truth + typed invalidation broadcast                       |
| Legacy sessions            | pre-v26 data                            | message-only artifact backfill plus existing nullable workspace fallback     |

The design deliberately does not introduce a generic plugin system, arbitrary template
expression language, secret export format, cloud synchronization port, or a second
session store.

## Session artifact inventory

Schema v26 stores artifact metadata in `session_artifacts`; source files stay in place. Successful
write/edit tool targets, local MCP resources, explicit tool metadata/output arguments, and completed
assistant Markdown references are discovered in that order and deduplicated by session plus resolved
path. Forking copies metadata, deleting a session cascades metadata, and missing files remain named.
Legacy migration parses persisted messages once and never scans output directories.

The Desktop loads the paginated inventory with the session and applies durable `artifact_update`
notifications idempotently. Inventory metadata is backend-owned; the renderer presents entries
whose extensions match the user's persisted Outputs display list. An in-app preview renderer is not
required for listing, reveal, or external opening. The optional, remembered `Hide repository files`
switch further excludes recognized source/project filenames and paths beneath Git, Mercurial, or
Subversion markers, including Git worktrees. Authorized ancestor-marker metadata checks determine
repository membership; unavailable checks leave entries visible with a status. This filter changes
the displayed inventory and count without removing metadata or closing previews. See ADR-0013 for
the classification boundary. Preview tabs and
active selection are separate, session-scoped user state. Listing an artifact never opens the pane,
reads a file, creates an output folder, copies a file, or grants access. Selection still traverses the
Electron file guard, so only
renderer directory grants, validated workspace output roots, explicit picker grants, and the exact
session deliverable capabilities defined by ADR-0006/0013 authorize reads, reveal, copy, or external
opening. Session capabilities are taken from the current window's validated routing configuration;
they are not retained as directory grants or as permanent picker grants.

Outputs and Research Library lists also expose explicit single-file and batch Trash actions.
The file IPC handler checks each path with the artifact guard, rejects directories and symbolic
links, and returns per-file outcomes without falling back to permanent unlink. Desktop closes
successful previews and persists deleted Outputs versions as session presentation state; backend
artifact provenance is retained. The Research Library list refreshes from its bounded disk scan.

## Error taxonomy

| Class       | Examples                                                         | ACP behavior                                             | UI behavior                               |
| ----------- | ---------------------------------------------------------------- | -------------------------------------------------------- | ----------------------------------------- |
| validation  | missing name, invalid enum/path, secret field in import          | invalid params with stable code + safe field details     | inline/actionable warning                 |
| not found   | workspace/profile removed                                        | resource not found                                       | refresh and show deleted/relink state     |
| conflict    | only workspace deletion, referenced profile without confirmation | invalid params/conflict code                             | explicit confirmation or prevented action |
| unavailable | missing primary folder, inaccessible path                        | safe recoverable error                                   | relink/temporary replacement action       |
| credential  | missing secure field/profile, unsupported auth scope             | safe relink/auth-required error; no provider detail dump | configured/missing/needs authentication   |
| storage     | malformed file, lock/rename failure                              | internal error with safe summary and path class          | persistent error state; no crash          |

Logs include method, stable error class, workspace/profile UUID, and path only when needed
for repair. They never include secret request bodies, secret values, or provider error
strings that have not been sanitized.

## Change analysis

- A UI framework swap touches only Desktop context/components and ACP adapter.
- A storage-format swap touches WorkspaceStore/migrations, not UI or session behavior.
- A new provider secret field is discovered through registry metadata and derives its
  namespaced key; it does not require a workspace schema field.
- A new product type changes the canonical enum, generator output, output selector, and
  editor tests; the traceability/test plan pins that fan-out.
- A future cloud-sync design would need a new conflict/identity ADR and is not implied by
  the current store port.
