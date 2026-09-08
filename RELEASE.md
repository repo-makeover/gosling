# Making a Release

gosling releases are built and published by GitHub Actions from version tags. Preparing documentation or pushing a release branch does not publish a release.

## Current release target

The current source/local-build target is **v1.2.2**; see the
[local build notes](documentation/docs/release-notes/v1.2.2.md). This version bump
does not itself publish a release. Release versions increment the single-digit
patch component and carry at 9: `v1.2.1` through `v1.2.9`, then `v1.3.0`, and
`v1.9.9` carries to `v2.0.0`.

Two candidate versions, `1.1.0` and `1.2.0`, were prepared in the source
manifests but never tagged or published. `v1.2.1` supersedes both. The
[v1.1.0 candidate notes](documentation/docs/release-notes/v1.1.0.md) are
retained as a record of that unpublished candidate.

The previous stable GitHub release is titled `v1.0.1` but is tagged
`v1.0.1-optimization-and-workspaces`. That historical tag does not match the
normal `[v]major.minor.patch` grammar. Preserve it as published history: do not
retag it or globally replace historical version strings. The historical
[v1.0.0 release notes](documentation/docs/release-notes/v1.0.0.md) remain a
point-in-time record, not the current release target.

## Required version alignment

Before tagging a candidate, update and review every version-bearing surface for
that version with `just bump-version <version>`, including:

- `Cargo.toml` workspace package version;
- workspace package entries in `Cargo.lock`;
- `ui/desktop/package.json` and the applicable pnpm lockfile entries;
- `ui/desktop/openapi.json` `info.version` and generated SDK metadata;
- packaged Desktop metadata and About/version output;
- README and candidate-specific documentation release notes.

## Automated release path

1. Run the [minor release workflow](https://github.com/cephalopod-ai/gosling/actions/workflows/minor-release.yaml) manually, or use its scheduled version-bump PR, if it matches the intended target.
2. Review and merge the version-bump PR into `main`.
3. Use the generated `release/<version>` branch and release PR for QA and release-only corrections.
4. Complete every required item in `RELEASE_CHECKLIST.md`, including installed artifacts on supported platforms.
5. Create and push the final version tag only from the reviewed release commit.
6. Confirm `release.yml` completes and the GitHub release contains the expected signed artifacts, checksums, install scripts, and notes.
7. Perform the post-release checks before promoting updater behavior or announcing availability.

`release.yml` is currently tag-limited to `v1.*` releases. The previously inherited automatic patch-branch creation and tag-triggered release-PR cleanup workflows were intentionally retired. Patch releases therefore require an explicit reviewed branch/PR and tag; do not rely on an automatic next-patch branch.

## Tagging

Use the exact reviewed release commit. Replace `<release-commit>` only after the
checklist is complete:

```bash
git tag -a v1.2.2 <release-commit> -m "gosling v1.2.2"
git push origin v1.2.2
```

Do not move or recreate a published tag to repair an artifact. Fix forward with a new patch version.

## Release boundary

- Documentation may be merged before the tag, but install links continue to resolve to the latest published artifact.
- Historical audit and release notes remain point-in-time evidence and are not rewritten to make a release look green.
- A successful source test suite is not a substitute for installed Desktop, signing, updater, and clean-machine checks.
- The release owner, not documentation automation, approves signing, tagging, publication, and announcement.
