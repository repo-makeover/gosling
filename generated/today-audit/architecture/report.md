# Architecture drift audit — 2026-09-08

## Executive summary

Target: `/Users/eric/Work/vscode/forked/gosling`, main at `a48108750`; tree clean at intake. Independent read-only audit using catalog `audit-architecture-drift`. Security excluded. One Low, Confirmed invariant violation: four new artifact IPC channel names bypass the shared command contract. No current caller/handler mismatch is claimed. A separate Low working-directory metadata propagation defect is handed to Workflow/Dataflow and the authorized repair stage.

Health score: not computed (narrative registry has no normalized scope/weights); risk mass: zero Critical/High, one Low. Trend and new-versus-known unavailable: no stored architectural baseline. This is a scoped static audit, not whole-repository architectural certification.

## Scope and method

Inventoried today's commits through HEAD using local September 8 boundary and their changed files. Deep scope: output history service/ACP/generated SDK/Desktop call path, artifact IPC additions and payload types, session metadata and directory mutation response, compaction preference definitions and consumers, relevant ADR-0013/0018 and architectural registry. One-hop main/preload contracts, tests, directory handler and context policy were inspected. Remaining daily file inventory is not a claim of deep review; translations, permission implementation and unrelated deployment are excluded or deferred. Approximate budget: 40 source/doc files and focused searches.

AGENTS, README, docs/INDEX, architecture, relevant ADRs, .architecture and .giles advisory metadata were read. GEMINI.md absent. .giles audit content is stale July 7 advisory material, not promoted evidence. Recent September 8 session repair logs describe prior validation only; their passing tests were not reused as independent proof. Catalog planner-owned normative graph/invariant references were read through the local filesystem after the MCP loader refused cross-skill paths. The repository uses its own narrative registry format, which was interpreted directly; no catalog-format migration is required or inferred.

## Category evaluation

| Category | Score | Mapped checks / violation mass | Evaluation |
|---|---|---|---|
| architecture | not computed | AID-009/014; 1 Low | Declared IPC contract drift |
| design | not assessed | 0 | No redesign audit |
| security | not assessed | 0 | Explicitly excluded |
| performance | not assessed | 0 | No benchmarks |
| scalability | not assessed | 0 | No load analysis |
| maintainability | not computed | AID-002/008; 0 | Focused ownership trace |
| documentation | not computed | AID-010; 0 | Output ACP/export boundary agrees |
| testing | not computed | AID-011; 0 | Tests mapped, not executed |
| observability | not assessed | 0 | No operational telemetry audit |
| deployment | not assessed | 0 | No package/deploy trace |
| developer experience | not assessed | 0 | Outside focused paths |
| consistency | not computed | AID-014; shared Low | IPC shared contract |
| naming | not assessed | 0 | No naming sweep |
| ownership | not computed | AID-012; 0 | Service/adapter owners explicit |
| technical debt | not assessed | 0 | No broad debt inventory |
| dependency health | not assessed | 0 | No dependency audit |
| agent coordination | not computed | AID-001; 0 | Hosted capture registration traced |
| memory usage | not assessed | 0 | Bounds read, runtime not measured |
| state management | not computed | AID-009; 0 | Inventory/history distinction held |
| error handling | not computed | AID-009; 0 | ACP typed error seam inspected |
| configuration | not computed | AID-009; 0 | Shared compaction validator |
| extensibility | not assessed | 0 | No future topology speculation |

## Inventory disposition

| Check | Disposition | Evidence / limits |
|---|---|---|
| AID-001 Partial implementation | Non-finding in sample | Hosted capture `tool_dispatch.rs:229,283` reaches core revision storage; Desktop outputRevisions.ts consumes all three typed ACP methods. |
| AID-002 Duplicate implementation | Non-finding in sample | Core owns persistence; Desktop owns presentation/export through Electron; not duplicate revision stores. |
| AID-003 Abandoned architecture | Non-finding in sample | ADR-0018 has live core/ACP/UI implementations. |
| AID-004 Accidental architecture | Non-finding in sample | New revision service traced to accepted ADR-0018; artifacts traced to ADR-0013. |
| AID-005 Dead interface | Non-finding in sample | Four new preload methods have main handlers and UI consumers. Dynamic/external consumers not exhaustively modeled. |
| AID-006 Orphan service | Non-finding in sample | Output capture reached from hosted dispatch; history reached from ACP. |
| AID-007 Unused abstraction | Not Reviewed | No exhaustive abstraction/consumer graph. |
| AID-008 Excessive indirection | Non-finding in sample | ACP maps transport/errors, core owns storage, Electron owns native save; each layer has a decision. |
| AID-009 Design contradiction | Non-finding in sample | ADR-0018 amended commit-before-publication ordering and separate ACP history/Electron export agree with current implementation. |
| AID-010 Documentation drift | Non-finding in sample | Hash meaning documented in Rust DTO, ACP schema and SDK; docs describe export through Electron. |
| AID-011 Testing gap | Non-finding in sample | Output revisions, ACP, file IPC, workbench and history tests exist; channel parity gap belongs to causal AID-014. No exhaustive coverage claim. |
| AID-012 Ownership ambiguity | Non-finding in sample | docs/architecture module table owns inventory, bridge, UI state; ADR-0018 owns core revisions. |
| AID-013 Coupling growth | N/A | No stored baseline. |
| AID-014 Invariant violation | Finding | ARC-TODAY-001 / registry ARC-003. |

| Registry invariant | Disposition |
|---|---|
| ARC-001/002 | Not Reviewed: HTTP reply transport unchanged and outside selected ACP/service one-hop sample. |
| ARC-003 | Finding: new artifact command literals not in shared IPC contract. Main-to-renderer event registry exists; all new preload operations have handlers. |
| ARC-004 | Not Reviewed: Goose catalog adapter unchanged and outside selected scope. |
| ARC-005/006/007 | N/A: explicitly retired in registry; no missing-path defects emitted. |
| ARC-008/009 | Not Reviewed: shell composition/runtime unchanged; Desktop main/preload additions do not establish full shell contract review. |
| ARC-010 | Not Reviewed: CLI change is compaction validation, not domain negotiation; no full domain runtime audit. |

## Findings

Full schema-conformant record and source quotes: `findings.json`; stable invariant identity: `findings-drift.json`.

ARC-TODAY-001 (Low, Confirmed, source-evidenced): `.architecture/invariants.yaml:27` requires privileged renderer-to-main channels in a shared contract. Today's `copy-artifact-contents`, `classify-artifact-repositories`, `get-artifact-file-timestamps` and `trash-artifact-files` are repeated strings in `preload.ts:250-265` and `main/fileIpc.ts`, absent from `ipc/channels.ts:19-28`. `fileIpc.test.ts:26` compares handlers only to main's own inventory. Cause: extending implementation lists without extending the declared shared contract. Minimal repair: share the four new constants across both sides and verify bridge-to-handler parity. No live mismatch, data loss or security defect is claimed.

## Opportunities, waivers and trend

No unrelated simplification recommended. No exception records present in the inspected registry; no waivers invented. Trend unavailable without baseline; generated output is not promoted into .architecture.

## Skill escalation

| Item | Primary | Secondary | Reason |
|---|---|---|---|
| ARC-TODAY-001 | Architecture drift | Internal API | Shared IPC declaration/payload consumer parity; no security escalation per user exclusion. |
| Directory metadata | Workflow/Dataflow | Internal API | `WorkingDirectoriesMenu.tsx:151-157` updates directories only; `:220` reads unchanged workspace_folder_roots. `SessionWorkingDirsResponse` has only path lists, while `manage_sessions.rs:143-152` changes effective policy. New granted folder shows generic Workspace policy until full reload. Low presentation drift; return authoritative roots and consume them. |

## Planner/repair handoff

Prioritization proposal, not a plan: near-term authorized repair of the single Low shared-command contract finding plus the directory response/UI propagation stub. No immediate High/Critical issue surfaced by this lens. findings.json and findings-drift.json are the handoff artifacts; parent owns aggregate disposition and repair record.

## Validation limits and final confidence

Static source trace only; no repository code execution, GUI, packaged application, test replay, network or kill drill in this audit phase. Schema validation of the report is separate from product validation. Exact imports and generated schema read, not full compiler graph extraction. High confidence in the explicit registry contradiction, medium confidence in broader sampled conformance. Remaining frontier: full output capture mutation ordering belongs to reliability/dataflow lanes; full IPC prior surface, shell runtime, HTTP transport and dynamic consumers remain Not Reviewed. No security audit performed. This agent wrote only generated report artifacts during the audit phase.

## Repair-stage intake clarification

The directory metadata stub is now separately tracked as ARC-TODAY-002 (Low, Confirmed), preserving the same baseline evidence. It is included in findings.json and the drift sidecar. The source scan was frozen before repairs. Parent locale drift item WFG-TODAY-004 is routed to the parent repair lane; no duplicate discovery claim. See repair.md for dated repair/validation closure.

## 2026-09-08 repair addendum

ARC-TODAY-001 and ARC-TODAY-002 are locally repaired with 18 passing targeted UI tests, successful canonical ACP schema/SDK generation (including Rust core compile), formatter and diff checks. Parent union Rust/Clippy and Desktop checks remain pending; repair.md and validation.json carry exact evidence and limits. Historical baseline findings above remain unchanged.
