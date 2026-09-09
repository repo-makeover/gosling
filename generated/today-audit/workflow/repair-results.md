# Workflow repair results — 2026-09-08

Status: completed_with_partial_verification.

Two accepted findings repaired: WFG-TODAY-001 (P3/frontend) and WFG-TODAY-002 (P2/frontend). WFG-TODAY-003 and 004 are parent-owned and were not modified by this lane. No commits, merge, external actions, security audit or native file mutation.

## Stages and behavioral evidence

1. Titles (ArtifactPane.tsx/test): cache now keyed by resolved path and inventory/library version; restore metadata event and window focus invalidate title versions. Response cancellation prevents an obsolete read from winning. Read requests remain guarded and use batches of at most 200, matching the existing main-process ceiling. Filename fallbacks, preview selection, copy and Trash flow preserved.
   - Baseline reproduction: title-baseline.log, 2 failed / 33 passed. Same-display-path cross-session title and restored/focused title failed on original source.
   - Regression coverage now includes changed version, cross-session relative filename, stale read arriving after a newer version, relative preview resolving to canonical title, restore/focus refresh.
2. Unread (NavigationPanel.tsx/test, BaseChat.tsx, utils/sessionActivity.ts/test): BaseChat publishes additive lastReplyId derived from genuine user-visible assistant text/image messages. Navigation compares prior idle/start reply identity and only badges completion with a new reply; missing metadata retains old compatibility. Active focused/visible chat is acknowledged; route activation, focus and visibility changes clear only current chat. Workspace grouping, background unread, error/stream filtering, deletion/archive cleanup and row click behavior preserved.
   - Baseline reproduction: unread-baseline.log, 3 failed / 11 passed. Visible-chat unread, navigation/focus read acknowledgement and user-count-only completion fail before fix.
   - Regression coverage: user insertion after streaming start without a reply, compaction reducing messages without reply, first streaming event already carrying reply after batching, hidden/background active chat, route/focus acknowledgement, real image reply, no-ID and hidden/status-only messages.
   - Manual command confirmation remains a visible assistant reply. It may badge a background chat; no ACP field or slash-command policy duplication was added to suppress it.

The two stages have independent state and owned files; combined tests verify the union. Parent retains ownership of shared backend, i18n and ArtifactFileList regression fixes.

## Validation

Hermit environment; Desktop commands from ui/desktop:

- `pnpm exec vitest run src/components/Layout/NavigationPanel.test.tsx src/utils/sessionActivity.test.ts src/components/artifacts/ArtifactPane.test.tsx`: 54 passed across three files. Evidence `verified.log`.
- `pnpm exec eslint` scoped to all seven owned TypeScript files `--max-warnings 0`: passed. Evidence `lint.log` (empty output, exit 0).
- `pnpm exec prettier --write` on owned files: completed. Parent final format check covers union.
- `git diff --check` for owned files: passed.
- Parent running full Desktop suite/typecheck and required cargo fmt across campaign. Results belong to parent ledger.

Tests use jsdom and mocked native/ACP bridges. Source paths and rendered behavior are verified; native OS focus, packaged Desktop, clipboard, Trash and save dialog not executed. Fixture reset uses per-test mock/localStorage cleanup; no DB/production-state oracle is borrowed. No test failure was suppressed.

## Gate 8: focused workflow re-audit

Reapplied workflow-gui failure/producer-consumer/stale-state angles to BaseChat→event→Navigation→workspace badge; real visible message IDs and image fallback, missing-field compatibility, active session ref, focus/visibility listeners and cleanup. Reviewed server hidden-compaction filtering and explicit command-confirmation semantics. Rechecked title inventory/library source→canonical request→revision cache→row/preview and restore event. Existing preview resolves relative source to canonical filePath; relative-preview regression confirms canonical cache use after resolution.

No new defaultMessage, persisted format, permission path, backend API, external dependency or schema change. No uncovered security claim. Parent independent reviewer also inspected the changes; feedback about command semantics was resolved by preserving visible confirmation behavior rather than broadening transport.

## Gate 9: distinct completeness / drift comparison

Matched both original reproductions to failing baseline then passing fixed tests. Inspected all seven owned files and final diff, listeners/cancellation, base event producer and render consumer. No in-code TODO/FIXME marker names either repaired defect. Source record `report.md` receives this dated closure addendum; parent adds campaign session log.

Authority map: AGENTS remains canonical; accepted ADR-0013 governs session inventory presentation and file authorization; accepted ADR-0018 governs restored output presentation. Pre-repair drift: title projection dropped canonical identity/freshness and unread status lacked read acknowledgement. Post-repair: no new drift; same Rust authority and Electron guarded reads, same session persistence, internal additive event only. No declaration edited to manufacture conformance.

Residuals: native focus/window behavior and full app interaction untested by this lane; parent full-suite results determine campaign-level validation. Existing unread state is per-window and ephemeral. Title extraction remains a bounded prefix read; absent/unavailable headings use filenames and retry on version/focus/access-refresh events.
