# documentation index

This file is the durable map for the repository's documentation surface.

## authority and governance

- [repository instructions](../AGENTS.md)
- [architecture registry](../.architecture/README.md)
- [documentation style guide](./AGENTS.md)
- [root overview](../README.md)
- [docs-site workflow](./README.md)
- [structure compliance](./STRUCTURE_COMPLIANCE.md)
- [documentation inventory](./DOCUMENTATION_INVENTORY.md)

## user-facing docs

- [quickstart](./docs/quickstart.md)
- [getting started](./docs/getting-started/)
- [guides](./docs/guides/)
- [workspaces guide](./docs/guides/workspaces.md)
- [Goose and gosling feature comparison](./docs/guides/goose-comparison.md) — Goose v1.49.0 / gosling v1.2.2, checked 2026-09-08
- [historical Goose v1.47 compatibility record](./docs/guides/goose-v1-47-compatibility.md)
- [troubleshooting](./docs/troubleshooting/)
- [v1.0.0 release notes](./docs/release-notes/v1.0.0.md)
- [v1.1.0 candidate notes](./docs/release-notes/v1.1.0.md)
- [v1.2.1 source notes](./docs/release-notes/v1.2.1.md)
- [v1.2.2 local build notes](./docs/release-notes/v1.2.2.md)
- [release-note archive](./docs/release-notes/)
- [tutorials](./docs/tutorials/)
- [experimental](./docs/experimental/)
- [mcp catalog](./docs/mcp/)
- [architecture docs](./docs/gosling-architecture/)

## site content and publishing

- [blog](./blog/README.md)
- [automation](./automation/README.md)
- [sidebar config](./sidebars.ts)
- [docusaurus config](./docusaurus.config.ts)

## release and validation

- [release process](../RELEASE.md)
- [release checklist](../RELEASE_CHECKLIST.md)
- [current engineering TODO](../docs/TODO.md)
- [documentation TODO](./TODO.md)
- [test ledger](../docs/polish/test-ledger.md)
- [110-card live playtest and repair closure](../docs/cloud/2026-07-20-live-all-scenarios-playtest.md)
- [test scenario cards](../docs/test_scenarios/)

## stewardship notes

- User-facing site content is canonical under `documentation/`; engineering,
  architecture, audit, governance, and session evidence is canonical under
  `docs/`.
- Root `README.md` is the product entry point; `documentation/README.md` is the docs-site build and publishing guide.
- Session-share deep links are documented with the `gosling://` scheme only. Legacy `goose://` share-link compatibility is not part of the current docs contract.
- Durable documentation governance artifacts currently live in this directory as point-in-time records rather than a full log/archive program.
- The current source/local-build version is `v1.2.2` as of 2026-09-08. It does not identify the version available from a published channel. Validation, tagging, signing, publication, and updater promotion remain separate release gates; see [the release process](../RELEASE.md).

## follow-up disposition

- The durable test ledger and scoped documentation TODO now exist and are linked
  above without replacing their source reports.
- `.dory/` is ignored local operational state, not canonical documentation or
  repository evidence. Durable evidence must be written explicitly to a
  reviewed, committed repository log.
