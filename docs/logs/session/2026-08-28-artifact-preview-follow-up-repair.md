# Artifact preview and Deep Research follow-up repair

Date: 2026-08-28

## Task

Repair two regressions observed in Deep Research session `20260828_51`:

- a session output could not be previewed without granting it again through a native file chooser;
- asking for the path of an older output failed completion because Gosling demanded a new Research
  Library copy.

## Selected patch batch

1. **P1 reliability — exact-file preview capability rejected.** The active session published the
   backend-owned artifact path, but Electron accepted it only when it was also under the app's
   launch-time directory roots. This contradicted ADR-0006's transient exact-file capability.
2. **P1 reliability — re-mentioned output treated as current.** Assistant artifact discovery updates
   `last_seen_at` and `source_id` when a later answer mentions an existing path. Completion scoping
   treated that refresh as a new deliverable and required a new archive copy.

## Changes

- Canonicalize and admit existing exact-file capabilities only for the established document,
  spreadsheet, presentation, text, and JSON deliverable extensions. Directory access, source code,
  configuration files, and arbitrary extensions remain denied.
- Scope assistant-message deliverables to the run in which their artifact was first observed.
  Built-in tool writes and modifications continue to use current-run update provenance.
- Added regressions for an output outside launch roots and for a current path-only answer that
  re-mentions an older unpaired output while the session already has a verified report pair.

## Architecture and contract check

- `docs/architecture.md` and ADR-0006 are active and require backend-owned inventory plus transient
  exact-file preview capabilities; the preview repair restores that declared behavior.
- ADR-0016 remains unchanged: genuinely new Deep Research deliverables still require separately
  reported, byte-identical Output and Research Library copies. Gosling does not create or overwrite
  archive files after the fact.
- Pre-repair disposition: evidenced drift from ADR-0006 and incorrect current-run classification
  within ADR-0016's completion gate.
- Post-repair disposition: no new drift; the existing exact-file and dual-copy contracts remain in
  force.

## Validation

- `cargo test -p gosling research_completion::tests -- --nocapture`: pass, 7/7.
- `cargo clippy --all-targets -- -D warnings`: pass.
- Focused desktop artifact and preview suites: pass, 24/24.
- `pnpm test:run`: pass, 1,074/1,074.
- `pnpm run typecheck`: pass.
- `cargo fmt --all` and `git diff --check`: pass.
- `just package-ui`: pass.
- Packaged and installed sidecar SHA-256:
  `087d8abeea788e45f85d5f9bed4251b7ace5b6bfb41be8ef0a10bfcaca527739`.
- Installed `/Applications/Gosling.app` code-signature verification: pass.

## Installation and recovery

- Installed application: `/Applications/Gosling.app`, version 1.1.0.
- Previous application backup:
  `/Users/eric/.local/share/gosling/install-backups/Gosling-before-artifact-preview-fix-20260828-085854.app`.
- The macOS Keychain authorization dialog caused by the rebuilt ad-hoc signature was accepted and
  the installed backend reached its loopback listening state. ScreenCaptureKit returned `-3811`, so
  the preview click-through and a path-only follow-up remain partial live checks.

Final status: `completed_with_partial_verification`

## Recurring preview denial investigation (2026-09-08)

Source observation ART-PREVIEW-001: the operator's screenshot shows
`muninn-sync-repair-plan-2026-09-08.md` in the Outputs pane, but selecting it
requires a native file-picker grant. This is P1 Desktop correctness, localized
to the live Electron artifact authorization path. Intake: clean `main` at
`86ed0badb`, macOS arm64, installed Gosling 1.2.1.

Using catalog `repair-defect-patchset` and its playtest/contract/closure guidance,
with the screenshot as the supplied finding. Existing user authorization covers
repair/testing; the earlier no-application-backup preference remains in force.
Independent reads/checks are batched; edits, validation and packaging remain
dependent stages. This entry is the durable checkpoint.

The renderer publishes document-like session artifacts, and the main process
validates/canonicalizes them into routing configuration. However,
`main.ts::assertRendererArtifactFileAccess` omits `routingConfig.artifactFiles`
when calling the file guard. `main/rendererAccess.ts` contains the correct union
but is not imported by the live entrypoint. Existing controller/helper tests
therefore miss the integration failure. ADR-0006 and ADR-0013 already authorize
these transient exact-file capabilities; the repair restores that contract.

Baseline: 43 focused Desktop artifact/main tests and typecheck passed. A new
integration test executes the actual `main.ts` authorization functions and IPC
registration with temporary files, without Electron startup or user settings.
Before the fix, three cases failed with the screenshot's exact "outside approved
roots" error; three denial/control cases passed. With the fix, all six pass.

The runtime patch adds the current routing configuration's validated exact-file
capabilities to the artifact guard. It does not persist them as picker or
directory grants. The unused controller's misleading header now identifies the
live entrypoint, and the architecture paragraph matches the existing ADR policy.
No cross-file authorization refactor was performed.

### Review and validation

- Adversarial self-review: traced the renderer's existing deliverable filter,
  main-process canonicalization, actual IPC dependency injection, routing
  revision protection and window cleanup. Regression cases preserve neighboring
  file, other-window and generic-read denials; reject source/directory
  capabilities and a retargeted symlink; and exercise routing replacement/clear.
- Distinct completeness self-review: checked the screenshot's path against the
  read-only session inventory (`20260906_50`, `created`, `built_in_tool`), confirmed
  the file exists, and reconciled the runtime path, ADR-0006/0013, earlier repair
  record and TODO closure. These are self-review passes, not an independent audit.
- Focused Desktop suite: **49/49 tests, 9 files passed**.
- Complete Desktop suite (`pnpm --dir ui/desktop test:run`): **1,166/1,166 tests,
  153 files passed**.
- Desktop typecheck, targeted ESLint with zero warnings, targeted Prettier check,
  `cargo fmt --all -- --check`, and `git diff --check`: passed.
- `just copy-binary` and `pnpm --dir ui/desktop run package`: passed. The unchanged
  Rust sidecar was reused; no Rust runtime source changed in this repair.
- Build/test evidence: `/tmp/gosling-artifact-{red,green,full-desktop,typecheck,eslint,prettier,rustfmt,package}.log`.

### Installed application and original-scenario retest

- Installed `/Applications/Gosling.app`, version 1.2.1, bundle identifier
  `com.electron.gosling`; packaged and installed deep/strict signature checks pass.
- Packaged and installed `app.asar` SHA-256:
  `32be11edd1e1ad4ec091e59d5460b636638e70f8962e376bf74ae4bb1e40fbca`.
- Packaged and installed sidecar SHA-256:
  `b419c384f54625113108d9a2611bf123f12f85c3177a6be718c4ca59423f8f2c`;
  identical to the previously installed sidecar.
- Quit the idle application normally before replacing it. No application backup
  was created. A delete-and-copy command was rejected before execution; used
  `ditto` over the existing bundle after confirming no old-only bundle files,
  then verified the installed signature and hashes.
- Initial ScreenCaptureKit failures were overcome by reconnecting Computer Use;
  native accessibility inspection then reproduced the denial in the old app.
  This retest used actual UI interactions, not direct IPC invocation.
- After reinstall, opening "this is a macos" automatically restored the selected
  `/Users/eric/Documents/muninn-sync-repair-plan-2026-09-08.md` and rendered its
  heading, metadata and plan sections. No "Preview unavailable", file access
  denial or grant-picker button was present. No file-picker approval was given.
- Switched to another chat and back, then clicked the same Outputs entry directly;
  the report remained readable. The application is left on that preview.

ART-PREVIEW-001: **closed — source regression, full Desktop suite and installed
original-scenario retest passed**. This supersedes the August 28 partial live
verification for this preview defect; it makes no new claim about the separate
Deep Research completion scenario or unrelated OS/tool permission prompts.
