# Independent system and surface audit session

Date: 2026-09-08

## Overview

Performed an independent audit of work conducted on 2026-09-08 (commits `a6ee677a6..HEAD`), driven by catalog `agent-skills` (`audit-dataflow-integrity` v3.3, `audit-reliability` v3.0, `audit-workflow-gui` v3.1, `audit-dataflow-state-transition` v3.1). Per operator instruction, security scans were excluded.

Audit report generated and saved at `docs/cloud/2026-09-08-system-surface-audit.md`.

## Surfaces Inspected

1. **Context Management & Auto-Compaction Reduction**:
   - `crates/gosling/src/context_mgmt/mod.rs`
   - `crates/gosling/src/acp/server/config.rs`
   - `crates/gosling-sdk-types/src/custom_requests.rs`
2. **Session Output Revisions & Contribution History Persistence**:
   - `crates/gosling/src/session/output_revisions.rs`
   - `crates/gosling/src/session/session_manager/output_revisions_storage.rs`
   - `crates/gosling/tests/output_revisions_test.rs`
   - `ui/desktop/src/components/artifacts/OutputHistory.tsx`
3. **Artifact Management in Desktop UI (Trash, Deletion, Copy, Timestamps)**:
   - `ui/desktop/src/main/fileIpc.ts`
   - `ui/desktop/src/components/artifacts/ArtifactPane.tsx`
   - `ui/desktop/src/components/artifacts/ArtifactFileList.tsx`
   - `ui/desktop/src/contexts/ArtifactWorkbenchContext.tsx`
4. **Working Directory Scope & Permission Inspection**:
   - `crates/gosling/src/permission/working_dir_scope_inspector.rs`
   - `crates/gosling/src/permission/permission_inspector.rs`
   - `crates/gosling/src/components/ToolApprovalButtons.tsx`

## Key Findings Identified

- **DAT-GSL-001 (High)**: Line-ending sensitivity (`\r\n` or trailing whitespace) in `markdown_body()` causes existing contribution history footer to fail detection, resulting in duplicate history tables appended on subsequent tool writes.
- **DAT-GSL-002 (Medium)**: Two-phase split transaction in `restore_output_revision` commits a `Baseline` revision before attempting `replace_if_unchanged`. On write failure, an orphaned baseline revision remains committed in SQLite.
- **WFG-GSL-001 (Medium)**: Outputs list in `ArtifactPane.tsx` does not prune trashed session artifacts immediately from `displayedArtifacts` upon deletion acknowledgment.
- **REL-GSL-001 (Medium)**: Trashing already-missing files marks them removed in the UI but does not reconcile or prune `session_artifacts` in the database.
- **WFG-GSL-002 (Low)**: ACP preference validation allows `autoCompactReduction >= threshold`, silently bypassing reduction during compaction.
- **REL-GSL-002 (Low)**: Full 20 MiB in-memory buffer allocation prior to binary detection in `copy-artifact-contents`.
- **REL-GSL-003 (Low)**: Redundant double canonicalization of paths in `out_of_scope_path`.

## Files Changed

- `docs/cloud/2026-09-08-system-surface-audit.md` (new)
- `docs/logs/session/2026-09-08-system-surface-audit.md` (new)
- `docs/INDEX.md` (updated)

## Validation

- Read-only analysis and AST traversal across all 122 modified files from 2026-09-08.
- Verified test suite assertions in `crates/gosling/tests/output_revisions_test.rs` and `permission_audit_regressions.rs`.
- Target working tree verified clean via `git status`.

## Repair follow-up — 2026-09-08

The [repair session](2026-09-08-system-surface-repairs.md) and
[audit disposition addendum](../../cloud/2026-09-08-system-surface-audit.md#7-repair-disposition--2026-09-08)
record five repaired findings and two closed as not-a-defect after source and regression checks.
The original observations above remain historical; database provenance retention and immediate
version dismissal are intentional, tested behavior under ADR-0013.
