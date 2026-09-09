# gosling Release Checklist

This is a maintainer-owned publish gate. Documentation preparation does not check
any item automatically. The current candidate is `v1.2.4`; preserve published
historical tags rather than rewriting them.

## Version and source identity

- [ ] The release commit is reviewed, immutable for the tag, and based on the intended `main` revision.
- [ ] `Cargo.toml`, `Cargo.lock`, `ui/desktop/package.json`, pnpm lockfiles, OpenAPI metadata, generated SDK metadata, packaged app metadata, About, and `gosling --version` all report the selected candidate version.
- [ ] `README.md`, `RELEASE.md`, and the candidate release notes match the final release scope.
- [ ] Contributor and goose upstream attribution remain intact.
- [ ] No release note claims a test, platform, signature, updater state, or artifact that was not observed.

## Source validation

- [x] `cargo fmt --check` passes. (Observed 2026-09-06 for `v1.2.1`.)
- [x] `cargo clippy --all-targets -- -D warnings` passes. (Observed 2026-09-06 for `v1.2.1`.)
- [x] The complete Rust workspace test suite passes. (Observed 2026-09-06 for `v1.2.1`.)
- [x] `cd ui/desktop && pnpm run typecheck` passes. (Observed 2026-09-06 for `v1.2.1`.)
- [x] `cd ui/desktop && pnpm test` passes. (150 test files observed 2026-09-06 for `v1.2.1`.)
- [x] The documentation build, test, and typecheck commands pass from `documentation/`. (Observed 2026-08-27.)
- [ ] Release workflow integrity, lockfile integrity, and archive/checksum verification pass.
- [ ] The full scenario-card suite is replayed, or every non-replayed card is explicitly dispositioned with focused replacement evidence.

## Installed Desktop: macOS 26 Apple Silicon

- [ ] Install the signed Apple Silicon release artifact on a clean or isolated account; do not test only `just run-ui`.
- [ ] Launch, onboarding, local ACP connection, app restart, and `Cmd+Q` cleanup work without orphaned Electron or `gosling serve --platform desktop` processes.
- [ ] `File > New Chat Window` opens an independent usable window; closing one window does not terminate another.
- [ ] The chat composer visibly includes the credential-profile selector and **Manage credential profiles** action.
- [ ] Creating, selecting, replacing, and deleting a credential profile preserves secret redaction and produces an actionable missing/relink state.
- [ ] Clicking a workspace filters the sidebar chat list and does not change the default workspace for global New Chat.
- [ ] Starting a chat from the `+` action beside a workspace preselects that workspace.
- [ ] Global New Chat allows explicit workspace selection, and an existing chat remains pinned to its original workspace and credential profile.
- [ ] Packaged Content Security Policy permits only the required loopback ACP HTTP/WebSocket connection.
- [ ] Signed/notarized status, Gatekeeper launch, updater metadata, and architecture identity are correct.

## CLI and interoperability

- [ ] Clean installs and upgrades work on each published OS/architecture.
- [ ] `gosling --version`, `gosling doctor`, session creation/resume, interruption, provider failure, and machine-readable output behave as documented.
- [ ] gosling and goose coexist without sharing config, data, database, keyring service, deep-link scheme, or single-instance lock.
- [ ] Provider API-key and subscription-backed ACP flows are tested without committing credentials or logs containing secrets.
- [ ] MCP extension discovery and deterministic goose compatibility adapters preserve source attribution.

## Artifact and publication checks

- [ ] Release-branch artifacts are downloaded and tested before tagging.
- [ ] Every expected CLI and Desktop artifact is present, named correctly, and associated with the correct OS/architecture.
- [ ] Checksums verify against downloaded artifacts.
- [ ] macOS and Windows signatures are verified where applicable.
- [ ] The install scripts resolve the candidate version correctly in an isolated environment.
- [ ] The GitHub release body uses the final candidate notes and contains no inherited goose release boilerplate.

## Publish and post-release

- [ ] Create and push the selected candidate tag only after all blocking items above are complete.
- [ ] Confirm the tag-triggered `release.yml` run succeeds.
- [ ] Read back the GitHub release, assets, checksums, and install commands from an unauthenticated client.
- [ ] Confirm `releases/latest` and the stable install path resolve to the candidate version only after publication is complete.
- [ ] Keep native macOS auto-update disabled until a compatible shipped version and updater metadata make promotion safe.
- [ ] Record any failed or deferred gate in the release notes or a follow-up issue; do not silently waive it.
