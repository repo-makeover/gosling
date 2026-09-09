# Independent workflow audit — 2026-09-08

## Executive verdict

Two contained workflow defects are source-confirmed: unread indicators derive from execution state rather than unread replies, and output titles remain cached across different files or changed revisions. A third finding records a test-contract regression independently reported by the parent validation run and confirmed by comparing source labels. No security audit was performed. No runtime code was changed in this lane. Repair is recommended; this is not an application-wide clean bill.

## Scope and execution

- Target: `/Users/eric/Work/vscode/forked/gosling`, branch `main`, clean at initial inspection, HEAD `a48108750945e42509164980e49ad452c3e12e79`.
- Changes: commits since 2026-09-08 00:00 America/Denver, through that HEAD, with adjacent callers and existing cache behavior.
- Skill: catalog `audit-workflow-gui`, read-only authority. Required shared contracts, detection playbook, test patterns, OUTPUTS and report template loaded. User authorized repair at campaign level; implementation belongs to parent lane.
- Assumption: source audit is sufficient to identify deterministic UI defects; native Electron behavior requires separate runtime verification.
- Effort: approximately 25 implementation/test/doc files deeply read, changed Desktop file inventory inspected. Rust restore internals and CLI were assigned elsewhere and not independently audited here.
- Execution: independent of other audit lanes; no further agents spawned. Source reading and parent test validation ran independently. Reports are the durable checkpoint. Source findings were reached before consulting prior repair closure details; parent supplied its failing-suite result separately.
- Orientation: AGENTS, README, docs/INDEX, architecture, ADR-0013/0018, advisory Giles identity and relevant recent repair notes. GEMINI.md is absent. Large unrelated Giles reports were inventoried, not exhaustively reviewed.
- Validation: source and caller/callee traces, `git diff`, `git blame`, `rg`, and numbered reads. Parent reports 114 passing and 7 failing tests across nine changed suites, all seven failures in ArtifactFileList due to stale labels. No tests or native filesystem mutations executed by this lane.

## Surface inventory and boundary map

| Surface | Trigger | Actual effect | Shown state / boundary | Result |
|---|---|---|---|---|
| Outputs / Library Trash | Row or selected paths, confirmation | Per-path Electron Trash result | Per-item errors; success and missing counts | Held in source |
| Output dismissal | Successful/missing Trash paths | Session presentation versions dismissed; previews close | Backend provenance retained | Held in source |
| Copy contents | Explicit preview toolbar action | Guarded complete UTF-8 native clipboard write | Success after awaited IPC; error toast on rejection | Held in source |
| Revision restore | Selected revision and confirmation | ACP restore with expected current hash | Refresh/callback only after success; visible failure | Held in source |
| Revision export | Save picker | Exact saved base64 bytes | Picker cancellation respected by backend; errors visible | No false-success claim; success UI minimal |
| Output titles | Inventory/library mount | Read title then cache | Display-path identity and freshness | WFG-TODAY-002 |
| Workspace unread icon | Streaming to idle event | Local unread state | Claims a new reply regardless of actual new reply or current viewing | WFG-TODAY-001 |
| Permission buttons | Allow/deny or extension grant | Pending-generation decision resolution | Stale and transport failures visible | Held in reviewed paths; no security claim |
| Artifact timestamps | Mount/version/focus/event | Bounded metadata refresh | Old requests rejected via revision counter | Held in source |
| Repository filter | Toggle + classification | Display-only filtering | Unknown classification remains visible | Held in source |
| Trash regression tests | Accessible-label query | Test attempts UI actions | Queries obsolete Delete labels | WFG-TODAY-003 |

## Findings table

| ID | Severity | Confidence | Basis | Title | Priority |
|---|---|---|---|---|---|
| WFG-TODAY-001 | Low | Confirmed | source-evidenced | Workspace unread icon marks viewed/non-reply activity unread | 2 |
| WFG-TODAY-002 | Medium | Confirmed | source-evidenced | Title cache aliases files and never invalidates changed contents | 1 |
| WFG-TODAY-003 | Low | Confirmed | source-evidenced | Trash wording leaves seven interaction regressions querying obsolete labels | 3 |

## Detailed findings

### WFG-TODAY-001: Workspace unread icon marks viewed/non-reply activity unread

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `ui/desktop/src/components/Layout/NavigationPanel.tsx:318-324`: `const shouldMarkUnread = existing?.streamState === 'streaming' && streamState === 'idle';` and `hasUnreadActivity: existing?.hasUnreadActivity || shouldMarkUnread`.
- `ui/desktop/src/components/BaseChat.tsx:230-250`: `ChatState.Compacting` is mapped to `streamState = 'streaming'`; event also carries `messageCount`, which Navigation ignores.
- `ui/desktop/src/components/Layout/NavigationPanel.tsx:348-358,632-636`: unread clears only through `clearUnread(session.id)` in row-click handler, not active route changes or visible completion.
- `ui/desktop/src/components/workspaces/WorkspaceSidebarSection.tsx:30-33,301-307`: rendered badge label says `A chat in this workspace has a new reply`.

Observed behavior:
- Any streaming→idle sequence creates unread state, including the foreground chat the user is already reading. A compaction lifecycle can produce the same signal without a new assistant reply.

Expected boundary:
- Unread state must represent a new reply not yet viewed, rather than generic operation completion.

Failure mechanism:
- Status transitions are treated as a reply/read-state oracle. Active session and actual assistant message changes are not checked; route-based entry also bypasses the only read acknowledgement.

Break-it angle:
- Complete a foreground turn; navigate away and see a spurious unread workspace. Send compaction-only streaming→idle status with unchanged messages. Enter an unread chat through route navigation rather than its row.

Impact:
- False unread indicators distract and erode the meaning of workspace notifications; no data mutation.

Operational impact:
- Blast radius: Local; Side-effect class: user-visible; Reversibility: reversible; Operator visibility: UI-visible; Rerun safety: safe.

Adjacent failure modes:
- A stopped turn with no reply can mark unread; loading state can preserve older unread state.

Recommended mitigation:
- Pattern: event semantics plus explicit read acknowledgement.
- Minimal repair: clear/suppress unread for the active visible chat, clear on route-based activation, and require actual assistant-reply advancement for new unread state. Avoid relying solely on total message count because compaction can change message structure.
- Local guardrail: retain unread state for other background chats in the workspace, and preserve streaming/error filtering.
- Behavior tests: foreground completion does not badge; background new reply does; compaction-only does not; route activation clears only that chat.

Implementation assessment:
- Complexity: operator_ux; Cost: S; Cost drivers: modules, tests; Nominal implementation agent: codex.
- Rationale: bounded BaseChat event/Navigation reducer and tests; no persistence migration.

Validation:
- Source proves the unconditional state transition. Native app/runtime manifestation was not exercised.

Non-goals:
- No durable cross-window unread store or notification redesign.

### WFG-TODAY-002: Title cache aliases files and never invalidates changed contents

Severity: Medium
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `ui/desktop/src/components/artifacts/ArtifactPane.tsx:552-568`: requests map uses `requests.set(artifact.displayPath, { filePath: artifact.displayPath, baseDirectory: artifact.baseWorkingDir })`.
- `ArtifactPane.tsx:570-591`: `const pending = titleRequests.filter((request) => !(request.filePath in documentTitles));` and `next[request.filePath] = titles[request.filePath] ?? '';`.
- `ArtifactPane.tsx:682-687`: cached title is preferred over current preview content.
- `ArtifactPane.tsx:1059,1062-1067`: restore increments only preview revision; row name uses `documentTitles[artifact.displayPath] || artifact.displayPath`, although its actual path is `artifact.resolvedPath`.
- `git blame` pins cache implementation to `76355dd3b3` (2026-09-06), an existing defect reached through today's history/restore/filter workflow.

Observed behavior:
- Once `report.md` is cached, another session's different `report.md` reuses its title. Tool changes and revision restore cannot refresh a cached title, so title and actual contents disagree. Empty title results are permanent for the component lifetime.

Expected boundary:
- Labels must derive from the canonical file identity and current version; shared display names are not identities.

Failure mechanism:
- A display-path-only, permanent cache discards the working directory and version. The restore refresh path reaches the preview and timestamps but not title cache invalidation.

Break-it angle:
- Switch from workspace A/report.md headed Alpha to workspace B/report.md headed Beta while retaining pane mount. Restore a different heading at a previously cached path. Generate a heading after an initial empty title lookup.

Impact:
- Wrong document name appears on open and Trash controls, potentially causing the user to select the wrong document; confirmation still lists full paths and actual deletion scope remains correct.

Operational impact:
- Blast radius: Workflow; Side-effect class: user-visible; Reversibility: reversible; Operator visibility: UI-visible; Rerun safety: safe.

Adjacent failure modes:
- External edits and temporarily unavailable title reads remain stale until remount. Library and Outputs title requests can have different identities for the same canonical file.

Recommended mitigation:
- Pattern: canonical cache identity plus versioned invalidation.
- Minimal repair: request/cache by resolved canonical path and inventory/library version; refresh affected title after restore and focus invalidation. Render row/preview titles through the same identity. Retry unavailable reads rather than permanently treating failure as a confirmed absent heading.
- Local guardrail: preserve filename fallback and ensure late title responses cannot overwrite newer file versions.
- Behavior tests: two sessions with equal displayPath/different resolvedPath display independent headings; changed lastSeenAt reloads heading; restore reloads row and preview title; stale result after switch is ignored.

Implementation assessment:
- Complexity: local_guardrail; Cost: S; Cost drivers: modules, tests; Nominal implementation agent: codex.
- Rationale: one component cache contract plus focused tests; no new IPC or database schema needed.

Validation:
- Source confirms the cache key and missing invalidation. No live GUI trace was recorded.

Non-goals:
- No title extraction redesign or filesystem watcher.

### WFG-TODAY-003: Trash wording leaves seven interaction regressions querying obsolete labels

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- `ui/desktop/src/components/artifacts/ArtifactFileList.test.tsx:96,112,125,143,156,167,183`: queries include `name: 'Delete selected'` and `name: 'Delete One'`.
- `ui/desktop/src/components/artifacts/ArtifactFileList.tsx:29-30`: default messages are `Move {name} to Trash` and `Move selected to Trash`.
- Parent validation message: nine suites, 114 passed / 7 failed, all seven in this suite at old labels. This lane did not independently run that command and does not relabel it as its own reproduced test.

Observed behavior:
- The interaction tests cannot reach their intended selection, partial failure, batching and repeated-submit assertions after the wording change.

Expected boundary:
- Behavioral regression tests must find the current accessible control labels and exercise their assertions.

Failure mechanism:
- Product copy changed while sibling interaction tests remained on obsolete accessible names.

Break-it angle:
- Run the existing ArtifactFileList test file; label queries fail before behavior is exercised.

Impact:
- Failing regression gate and unavailable behavior protection until updated.

Operational impact:
- Blast radius: Repo; Side-effect class: none; Reversibility: reversible; Operator visibility: UI-visible; Rerun safety: safe.

Adjacent failure modes:
- Weakening queries to hide copy mismatches could remove the accessible-label check.

Recommended mitigation:
- Pattern: contract synchronization.
- Minimal repair: update queries to explicit current Trash names, retain all behavior assertions, rerun the suite.
- Local guardrail: keep row versus bulk accessible names distinct.
- Behavior test: existing selection/partial-failure/batch/repeated-confirmation tests all execute and pass.

Implementation assessment:
- Complexity: local_guardrail; Cost: XS; Cost drivers: tests; Nominal implementation agent: codex.
- Rationale: test-only literal update, followed by focused Vitest execution.

Validation:
- Parent owns already-announced repair and rerun; no code edits in this lane.

Non-goals:
- Do not revert improved Trash wording or relax test coverage.

## Required inventory dispositions

| Check | Disposition |
|---|---|
| WFG-001 Fake success | Not confirmed: copy awaits IPC before toast (ArtifactPane:709-724), restore waits before callback (OutputHistory:223-233). |
| WFG-002 UI/API mismatch | Not confirmed for Trash: non-failed removed and failures surfaced separately (ArtifactFileList:191-215). Test contract mismatch separately recorded as WFG-TODAY-003. |
| WFG-003 CLI/API mismatch | Not reviewed: selected outputs UI features have no equivalent CLI in this scope. |
| WFG-004 Stale display | WFG-TODAY-002. Timestamp hook guards requestKey and revision; restore refresh event included. |
| WFG-005 Hidden failure | Not confirmed for copy/Trash/restore; title failures fall back to filenames rather than claiming title success (ArtifactPane:587). |
| WFG-006 Destructive ambiguity | Not confirmed: full selected paths and Trash explanation in confirmation (ArtifactFileList:371-411). |
| WFG-007 Approval gate bypass | Security review excluded. Non-security restore confirmation only sends mutation on explicit confirm (OutputHistory:430-453). |
| WFG-008 Status lies | WFG-TODAY-001. |
| WFG-009 Partial success presented complete | Not confirmed: per-file failed count and error text (ArtifactFileList:195-215); native handler continues siblings (fileIpc:535-589). |
| WFG-010 Disabled control active backend | Not confirmed on duplicate Trash submission: local deleting ref protects re-entry (ArtifactFileList:183-187). Backend auth/security bypass not audited. |
| WFG-011 Mutation without feedback | No material finding: Trash and restore change visible state; revision export has native picker and visible failures, but no explicit success toast (OutputHistory:243-260). |
| WFG-012 Workflow step skipped | No supported finding in inspected explicit-confirm restore/Trash flow. |
| WFG-013 Cannot diagnose | No supported finding: copy error names UTF-8/size/change failures (fileIpc:342-384); restore preserves reason in alert (OutputHistory:232-235,280-286). |
| WFG-014 Derived shown confirmed | Not confirmed: observed attribution disclaimer and unknown identities visible (OutputHistory:61-67,314-329). |
| WFG-015 Bulk semantics mismatch | Not confirmed: confirmed pending paths map to request batches, native handler loops exact posted Set (ArtifactFileList:188-190; fileIpc:551-553). |

## Break-it review and validation limits

Source-traced batch partial failure, missing files, duplicate confirm, confirmation scope, copy truncation independence, invalid UTF-8/size bounds, restore conflict, previous-revision failure and latest-history failure. No OS Trash, clipboard, native picker, foreground/background window behavior or fresh-process app drill was executed. Existing green tests are not treated as production end-to-end proof; their state-reset isolation was not audited. The source-based non-findings are narrow, not runtime guarantees. Tests for findings 001/002 are proposed, not passed.

Not reviewed: all unrelated Desktop components, accessibility beyond control naming, full localization catalog consistency, CLI parity beyond identifying absent equivalents, backend revision durability, security, or remaining repository files. Budget stop: all 15 required checks have scoped dispositions; remaining surfaces are parent/sibling coverage or deferred runtime work.

## Skill escalation

| Finding | Primary | Secondary | Why |
|---|---|---|---|
| WFG-TODAY-001 | Workflow-GUI | State-Transition / Temporal | Generic completion is not reply/read state. |
| WFG-TODAY-002 | Workflow-GUI | Data-Integrity / Temporal | File identity and revision freshness are dropped by projection. |
| WFG-TODAY-003 | Workflow-GUI | Architecture/Seam | Accessible-label producer and test consumer drift. |

## Patch order and next action

1. Repair title identity/freshness with focused component tests.
2. Repair unread semantics and visible-session acknowledgement; preserve background workspace behavior.
3. Synchronize Trash test labels and rerun existing suite (parent already owns this step).

Final confidence: high in the quoted deterministic source properties; runtime validation remains partial. Next action: parent validates/adjudicates these bounded repairs, then runs focused Desktop tests/typecheck.

## Additional parent validation finding — WFG-TODAY-004

### WFG-TODAY-004: Message catalogs retain obsolete Trash and revision-retention wording

Severity: Low
Confidence: Confirmed
Evidence basis: source-evidenced
Domain: Workflow-GUI

Evidence:
- At audited HEAD, `ui/desktop/src/i18n/messages/en.json:41-54` still contains `Delete {name}`, `Delete selected`, and `Other copies are kept.`; `outputHistory.note` at 1925 contains only the original authorship disclaimer.
- `ui/desktop/src/i18n/messages/de.json:3383-3400` contains the same obsolete English fallback for these keys. Parent independently checked all 15 non-English locales and found the same untouched fallback pattern.
- Source component defaults already say Trash and disclose saved revisions; `outputHistory.noRevisions` exists in source but was absent from the baseline catalog. Parent reports `pnpm i18n:check` failed on stale en.json.

Observed behavior:
- Source and runtime catalogs disagree on destructive-action names and retained revision explanation; the catalog check fails.

Expected boundary:
- Extracted source and unchanged fallback catalogs must track current user-facing product semantics.

Failure mechanism:
- Source defaultMessage edits were not extracted/synchronized after the prior repair.

Break-it angle:
- Run the repository i18n check or load a locale with the old fallback.

Impact:
- Users can see obsolete delete/retention copy despite the source repair; failing validation gate.

Operational impact:
- Blast radius: Workflow; Side-effect class: user-visible; Reversibility: reversible; Operator visibility: UI-visible; Rerun safety: safe.

Adjacent failure modes:
- Unreviewed source changes can leave translation hashes out of date; replacing actual translations indiscriminately would cause a new regression.

Recommended mitigation:
- Pattern: generated contract synchronization.
- Minimal repair: regenerate English source, replace only unchanged English fallbacks in other locales, synchronize hashes with documented accept-source-change mode, compile and check catalogs.
- Local guardrail: preserve actual translated strings.
- Behavior test: `pnpm i18n:check` and compile pass, all six keys have expected fallback/source semantics.

Implementation assessment:
- Complexity: local_guardrail; Cost: S; Cost drivers: modules, tests; Nominal implementation agent: codex.
- Rationale: catalog regeneration and exact unchanged fallback replacement; parent owns the locale edits.

Validation:
- Source comparison performed here; catalog check and repair executed by parent, not this lane.

Non-goals:
- No new translations or localization-system changes.

## Repair disposition addendum — 2026-09-08

Source snapshot above is retained as audit history. Current changes and validation are in `repair-results.md`.

- WFG-TODAY-001: repaired and focused tests pass. Precise boundary: foreground visible/focused chat is read; background new visible assistant reply is unread; user insertion or compaction without a new visible reply is not unread. A manual-command confirmation is itself user-visible assistant text and can mark a background chat unread; suppressing that message class is outside this repair's accepted scope. ACP does not carry agentVisible metadata, so no unsupported assumption or transport redesign was added.
- WFG-TODAY-002: repaired and focused tests pass; canonical identity/version caching plus focus and restore invalidation, stale-result guards, relative preview coverage.
- WFG-TODAY-003 and WFG-TODAY-004: parent owns repairs and final validation; consult parent campaign ledger for closure.

Native UI execution remains unverified. New code preserves existing catalog defaults and introduces no locale edits.

### Parent closure — 2026-09-08

WFG-TODAY-003/004 are closed in the campaign's evening repair record. The parent
corrected seven stale Trash-label tests and the extension-settings test that
still treated CSV as a non-default addition. All 1,271 Desktop tests pass.
The five stale English defaults plus missing noRevisions key were refreshed in
all 16 catalogs; no translated string was replaced. Extraction consistency,
compilation, 21 sync tests and all 15 non-English locale validations pass.
See `docs/logs/session/2026-09-08-evening-audit-repairs.md` for commands and limits.
