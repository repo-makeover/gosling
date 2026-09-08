# ADR-0016: Durable Deep Research library

Date: 2026-08-26
Status: implemented and locally validated
Related: ADR-0011, ADR-0013, ADR-0015

## Context

Deep Research reports and tutorials were ordinary session artifacts. Their metadata survived in the
session inventory, but the files could remain scattered across workspace output folders and were not
browsable as a reusable body of prior research. Reusing the input Library would blur the distinction
between operator-supplied evidence and model-produced documents. Expanding the Outputs inventory into
a directory scan would also violate ADR-0013's session-provenance boundary.

## Decision

The Desktop owns a separate Research Library directory. Its effective default is
`Documents/Gosling Research Library`; the operator may change it only through a native directory
chooser. The persisted renderer setting is readable but cannot be written through generic setting
IPC, so a compromised renderer cannot turn the library preference into an arbitrary directory grant.
The Electron main process creates and grants the effective root when the library is used.

The right artifact pane exposes a third, boxed-count `Library` tab. Main performs a bounded metadata
scan of the configured root: at most 500 document-like files, six directory levels, using the same
configured display extensions as Outputs. Hidden entries and symbolic links are excluded. Selection
then uses the existing artifact preview/open guard. This is a distinct browse contract; scanned files
are never inserted into the session Outputs inventory or the input Library. The listing result
includes a truncation flag; the tab renders `500+` and an explicit complete-folder recovery action
when more matching files exist.

Every Deep Research session receives the library root as an additional working directory and a
session system instruction that requires final user-facing reports, tutorials, appendices, and exported
data summaries in two places: a canonical copy in the session's active workspace output location, and
a separate, identical final copy in the library. The first retains the session-specific Outputs
provenance defined by ADR-0013; the second is a cross-session, cross-thread archive. The instruction
permits relevant prior reports as optional secondary context, labels them potentially stale, and
requires verification of load-bearing claims against current primary evidence. It explicitly forbids
treating model agreement or a prior report as independent corroboration. Scratch files and caches
remain outside the library.

Desktop includes the selected Research Library path in authenticated ACP new-session metadata and
also grants it as an explicit additional session folder. Rust canonicalizes both the library path and
the workspace product-output roots, verifies the library is the granted additional root, and persists
those paths as Deep Research session state.

Before ACP records an otherwise successful prompt as `Completed`, Gosling verifies the terminal
assistant message against the backend-owned session artifact inventory. A deliverable must be a
created or modified artifact beneath a configured workspace output root, its separately reported
library copy must be beneath the configured Research Library root with the same filename, and the
two bounded files must have identical SHA-256 content. Missing, misplaced, unreported, oversized, or
mismatched copies make the prompt `Failed`; cancellation remains `Cancelled`.

## Consequences

- Each research deliverable has a session-specific canonical copy in Outputs and an operator-visible,
  cross-session archive copy in the Research Library; both remain browsable from Gosling.
- Future research can consult relevant prior work without silently elevating it to source-of-truth
  status or auto-injecting the entire library into model context.
- The library is not a content-addressed archive. Operators can edit, move, or delete its files, and
  the next bounded listing reflects that filesystem state.
- Added 2026-09-08: Library rows expose Delete and checkboxes with Select all, Clear selection,
  and Delete selected. One confirmation lists the selected paths, then authorized regular files
  move to OS Trash. Successful files disappear on refresh and their open previews close;
  failed files retain their selection and a per-file error. Separate Outputs copies are kept.
- The agent instruction creates and reports the dual-destination artifacts; the ACP completion gate
  independently checks their provenance, placement, and identity. Gosling does not copy arbitrary
  session artifacts after the fact or rewrite files the user produced elsewhere.
- Amended 2026-09-06: the completion gate now closes out the contract itself before judging it.
  A report the turn wrote on only one side is mirrored to the other — Outputs to a Research
  Library topic folder (the report's own folder, or one named after the session), or Library
  back to Outputs — never replacing a different file of the same name (a dated copy is made
  instead), and each copy is recorded in the inventory and announced in the chat. A report
  written with a shell command is found by its mention in the final message. A research turn
  that ends without any report gets one hidden nudge to write it before the gate runs. If it still
  ends without one — the model is asking the operator something — the turn is reported as
  "waiting for your reply" with an inline chat notice rather than as a failure; the run state
  stays `Failed` so nothing unverified is ever recorded as `Completed`. The gate still verifies
  placement and byte identity; what changed is that a model that did the research but not the
  bookkeeping no longer fails the turn.
- Amended 2026-09-05: appended session system instructions (the library contract among them) are
  persisted in the session's extension data and re-applied whenever the session is activated, so a
  session resumed after an app restart or agent eviction is still bound by the dual-destination
  contract the completion gate enforces. Imported sessions never carry such instructions in.

## Rejected alternatives

| Alternative                                             | Reason rejected                                                                          |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Scan workspace Outputs globally                         | Loses session provenance and turns unrelated workspace files into research inventory.    |
| Store generated reports in the input Library database   | Conflates supplied evidence with generated analysis and duplicates filesystem documents. |
| Let renderer code submit a library path directly        | Creates a generic persistent directory-grant primitive.                                  |
| Auto-attach every prior report to every research prompt | Causes unbounded context growth and turns stale generated work into implicit authority.  |
