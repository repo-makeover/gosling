# Independent evening audit and repair

Date: 2026-09-08. Baseline: `main` at `a48108750945e42509164980e49ad452c3e12e79`,
clean. Scope: 19 commits made today (America/Denver), starting after `cb1aac7ed`,
and their adjacent consumers. This is a focused change-set audit, not a claim
that every repository path was reviewed. Security audit was explicitly excluded.

## Independent evidence

Four separate reviewers applied the catalog's dataflow-integrity, workflow-gui,
reliability, and architecture-drift skills. Each recorded its source findings
before patching. The parent validated reported causes and grouped repairs using
repair-defect-patchset. Audit reports preserve their original observations;
post-repair dispositions belong below and in the linked execution record.

- [Data flow](../../generated/today-audit/dataflow/report.md)
- [Workflow](../../generated/today-audit/workflow/report.md)
- [Reliability](../../generated/today-audit/reliability/report.md)
- [Architecture and drift](../../generated/today-audit/architecture/report.md)
- [Execution and validation record](../logs/session/2026-09-08-evening-audit-repairs.md)

## Findings and disposition

| ID | Severity | Defect | Current disposition |
|---|---|---|---|
| DAT-TODAY-001 | Medium | Unreadable or oversized live output blocks retrieval of valid saved revisions | Repaired; exact saved bytes remain exportable, unsafe restore stays disabled |
| DAT-TODAY-002 | Medium | Skipped pre-images can become false creation/authorship records | Repaired; bounded/incomplete observation remains unknown, including filesystem permission recovery |
| REL-TODAY-001 | Medium | Compaction failure and terminal-error metadata can produce successful run outcomes | Repaired; ACP/CLI outcomes retain failure and lease-revocation signals |
| REL-TODAY-002 | Medium | Manual compaction ignores prompt cancellation before saving | Repaired; cancellation prevents save, including CLI cancellation/EOF race |
| WFG-TODAY-001 | Low | Viewed or non-reply activity is marked unread | Repaired; visible reply identity and foreground acknowledgement drive unread state |
| WFG-TODAY-002 | Medium | Output title cache aliases relative paths and retains stale titles | Repaired; canonical identity, version, restore/focus refresh and stale-result rejection |
| WFG-TODAY-003 | Low | Interaction tests retain obsolete Trash labels and output-default assumptions | Repaired; artifact-list and extension-setting interaction tests pass |
| WFG-TODAY-004 | Low | Message catalogs retain stale Trash and revision-retention wording | Repaired; extraction, compilation and locale validation pass |
| ARC-TODAY-001 | Low | Four new artifact IPC channels bypass the declared shared contract | Repaired; shared constants and actual preload-to-handler parity test |
| ARC-TODAY-002 | Low | Adding a directory does not refresh the displayed effective access policy | Repaired; authoritative roots round-trip through generated SDK and Desktop |

## Verification boundary

The initial nine-suite Desktop check passed 114 tests and failed seven stale-label
assertions. TypeScript passed. The initial locale check failed on stale English
extraction; the corrected catalog passes extraction consistency, 21 sync tests,
and validation of all 15 non-English catalogs. Existing English fallbacks were
updated without replacing translations.

Final checks passed:

- Core library: 1,884 passed, 3 ignored; compaction: 10 passed; revisions: 28 passed.
- CLI: 20 session tests passed with isolated configuration. The ambient run passed
  18 and failed two model-switch expectations because of a stored thinking-effort
  preference. No real configuration was changed.
- Desktop: all 160 suites / 1,271 tests passed; TypeScript and scoped ESLint/Prettier passed.
- Canonical ACP/SDK generation and SDK build passed; i18n extraction consistency,
  compilation, 21 sync tests and all locale validations passed.
- Combined core/CLI Clippy with warnings denied, workspace format and diff checks passed.

An independent reviewer checked the repaired workflows and found three completeness
gaps within the original findings: filesystem uncertainty, lease-only cancellation,
and CLI cancellation versus EOF. All were repaired and rechecked. The reviewer
implemented the final CLI consumer fix; the parent independently reviewed that patch.
See the [closing review](../../generated/today-audit/review/report.md).

The final completeness pass matched all ten IDs to repaired paths and evidence,
checked generated contracts and documentation, and found no remaining in-scope
repair blocker. Manual command confirmations remain visible reply activity;
compaction without a new visible reply does not create unread activity.

Status: `completed_with_partial_verification`. Source regressions and checks pass;
native Electron interaction, packaged installation, cross-platform execution,
live providers and production crash drills were not performed. This is not a full
workspace Rust test run. Prior closures and deliberately deferred architecture work
are preserved. No commit, merge, installation or publication was performed.
