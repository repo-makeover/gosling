# Repair plan and baseline

2026-09-08. Supplied findings WFG-TODAY-001 and WFG-TODAY-002 from report.md; parent owns 003 and 004 (Trash tests/localization source drift). Parent authorized these two repairs and focused validation. Apply repair-defect-patchset.

Baseline main@a48108750; tree now dirty from independent sibling repairs. The four owned files have no other pending changes. Parent baseline says NavigationPanel and ArtifactPane suites pass. Native GUI not exercised.

Stages:
1. WFG-TODAY-001 (frontend/UX-bug, P3, medium): NavigationPanel.tsx and test. Preserve workspace filtering, background unread, error/stream exclusion, removal cleanup and row selection. Defect exceptions: foreground completion, compaction/no-new-message completion, route activation. Use existing messageCount event without public contract change.
2. WFG-TODAY-002 (frontend/UX-bug, P2, medium): ArtifactPane.tsx and test. Cache canonical path plus inventory/library version; discard stale responses and refresh after restore/focus. Preserve filename fallback, bounded title IPC and preview selection.

No shared schema/config or overlapping owned file; stages can be implemented independently, validation combined. Architecture map: ADR-0013 inventory/session ownership and metadata authority unchanged; ADR-0018 restore should refresh displayed current document. Preexisting cache drift is the exact defect. No security changes, dependencies, native writes, or architecture redesign. No source files beyond owned four.

Validation: add regression first, run targeted Vitest and observe failure; implement; repeat both files; typecheck, scoped ESLint/Prettier, git diff --check; parent runs required cargo fmt with its Rust edits. Final re-audit actor/event→Navigation state and inventory/restore→title projection, then distinct completeness pass. Checks are restartable per suite; reports are checkpoint.

## Gate 3 amendment before unread implementation

Message count alone is insufficient: user insertion may happen after streaming starts. Parent authorized additive BaseChat `lastReplyId` and `utils/sessionActivity.ts`/test, in addition to Navigation. Helper reads genuine IDs from visible assistant text/image content; Navigation compares idle/start baseline against completion and retains legacy fallback when metadata is absent. Native focus/visibility and route activation acknowledge current chat. This is an internal additive event field; existing consumers ignore it.

Compaction/source review: hidden summary and continuation are agent-only (`context_mgmt/mod.rs:416,438-441`), and load replay skips user-invisible messages (`acp/server/load_session.rs:64`). A manual command completion is user-visible assistant text and should retain background new-reply semantics. Parent explicitly constrained the repair to avoid ACP metadata expansion solely to distinguish that product case.
