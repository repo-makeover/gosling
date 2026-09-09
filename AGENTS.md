# AGENTS Instructions

gosling is an AI agent framework in Rust with CLI and Electron desktop interfaces.

## Setup
```bash
source bin/activate-hermit
cargo build
```

## Commands

### Build
```bash
cargo build                   # debug
cargo build --release         # release  
just release-binary           # release binary
```

### Test
```bash
cargo test                   # all tests
cargo test -p gosling          # specific crate
cargo test --package gosling --test mcp_integration_test
just record-mcp-tests        # record MCP
```

### Lint/Format
```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
```

### UI
```bash
just run-ui                  # start desktop
cd ui/desktop && pnpm run typecheck
cd ui/desktop && pnpm test   # test UI
```

## Structure
```
crates/
├── gosling              # core logic
├── gosling-acp-macros   # ACP proc macros
├── gosling-cli          # CLI entry
├── gosling-mcp          # MCP extensions
├── gosling-providers    # model/provider adapters
├── gosling-sdk          # Rust SDK
├── gosling-sdk-types    # shared ACP/SDK types
├── gosling-test         # test utilities
└── gosling-test-support # test helpers

ui/desktop/            # Electron app
ui/text/               # Ink terminal UI
```

## Development Loop
```bash
# 1. source bin/activate-hermit
# 2. Make changes
# 3. cargo fmt
```

### Run these only if the user has asked you to build/test your changes:
```
# 1. cargo build
# 2. cargo test -p <crate>
# 3. cargo clippy --all-targets -- -D warnings
```

## Rules

- Test: Prefer tests/ folder, e.g. crates/gosling/tests/
- Error: Use anyhow::Result
- Provider: Implement Provider trait see providers/base.rs
- MCP: Extensions in crates/gosling-mcp/
- UI Desktop: Use ACP SDK types or local `src/types/*` types. Do not import generated OpenAPI types/client code from `ui/desktop/src/api`
- Goose compatibility: Extension and skills discovery intentionally falls back to Goose's AAIF-maintained catalogs through a deterministic gosling compatibility adapter. See `documentation/GOOSE_COMPATIBILITY.md` before changing those links or normalization scripts.

## Code Quality

- Comments: Write self-documenting code - prefer clear names over comments
- Comments: Never add comments that restate what code does
- Comments: Only comment for complex algorithms, non-obvious business logic, or "why" not "what"
- Simplicity: Don't make things optional that don't need to be - the compiler will enforce
- Simplicity: Booleans should default to false, not be optional
- Errors: Don't add error context that doesn't add useful information (e.g., `.context("Failed to X")` when error already says it failed)
- Simplicity: Avoid overly defensive code - trust Rust's type system
- Logging: Clean up existing logs, don't add more unless for errors or security events

## Ink / Terminal UI (ui/text)

- Ink renders React to a fixed character grid — not a browser. Content that exceeds a Box's dimensions is NOT clipped; it visually overflows into neighboring cells and breaks the layout.

- Ink-Text: Never use `wrap="wrap"` inside a fixed-height Box — wrapped text can exceed the Box height and bleed into adjacent components. Use `wrap="truncate"` and pre-truncate the string to fit the available character budget (lines × width).
  
- Ink-Layout: When changing card/cell dimensions, always recalculate how much content fits. Account for borders (2 chars), padding, margins, and sibling elements when computing the
remaining space for dynamic text.
  
- Ink-Overflow: Ink has no `overflow: hidden`. The only way to prevent overflow is to ensure content never exceeds the container size — truncate text, limit list items, or cap height.
  
- Ink-FlexGrow: Avoid `flexGrow={1}` on text containers inside fixed-height cards — the text will try to fill available space but Ink won't clip it if it exceeds the boundary.
  
- Ink-HeightBudget: When computing how many rows/items fit vertically, count EVERY line used by headers, footers, margins, borders, and scroll indicators. Under-reserving vertical space (e.g., `height - 8` when chrome actually uses 16 lines) causes Ink to squeeze out margins between items, making borders collapse. Always audit the actual line count.
  
- Ink-TrailingMargin: Don't apply `marginBottom` to the last item in a list — it wastes a line and can push content out of the container. Use conditional margins or container `gap`.

## Never

- Never: Recreate `ui/desktop/src/api` or add `@hey-api/openapi-ts` to `ui/desktop`
- Cargo.toml: For human-authored dependency changes, use `cargo add` instead of manually editing dependency entries unless there is a specific reason not to.
- Cargo.toml: Automated dependency bump PRs are exempt; when manual edits are necessary, keep `Cargo.lock` consistent.
- Never: Skip cargo fmt
- Never: Merge without running clippy
- Never: Comment self-evident operations (`// Initialize`, `// Return result`), getters/setters, constructors, or standard Rust idioms

## Entry Points
- CLI: crates/gosling-cli/src/main.rs
- UI: ui/desktop/src/main.ts
- Agent: crates/gosling/src/agents/agent.rs


---

## Required execution clauses

These clauses are load-bearing for the Giles `repo_docs_agent_watcher`
contract (GDOC-058) and must remain literally present in AGENTS.md.

- **Read existing code before edits.** Inspect before modifying any file;
  do not edit by analogy or assumption.
- **Do not invent APIs/paths.** Follow existing patterns; do not invent
  APIs, imports, paths, or commands that are not already present.
- **Surface partial success.** Report work as partially validated when
  only part of the change was verified; never round partial up to done.
- **One task per run.** Prefer one workflow completed end-to-end over
  many partial surfaces added at once.
- **Preserve style/conventions.** Preserve the established formatting,
  naming, and structural conventions of the file being edited.
- **Run validation before done.** Validate before completion: any change
  that touches code or governance must execute the relevant tests/scans
  and report the result before being declared complete.
- **Do not delete/overwrite user work.** Do not delete or overwrite
  in-progress files. Never revert existing changes the operator did not
  authorize.
- **No fake success/stub completion.** MUST NOT present stubs,
  placeholders, or canned responses as working behaviour.


<!-- GILES:DOCS-GOVERNANCE:START -->
## Documentation and agent operating contract

This repository carries its own local operating instructions. Fleet-wide documentation governance may be scanned or enforced by Giles, but agents must not assume external memory or external policy is available while working in this repo.

### Required read order

Before making code, documentation, configuration, schema, or test changes, read the relevant files in this order when present:

1. `AGENTS.md`
2. `GEMINI.md`
3. `README.md`
4. `docs/INDEX.md`
5. architecture, development, usage, ADR, and governance docs referenced by `docs/INDEX.md`
6. `.giles/repo.yaml` and other `.giles/*.yaml` advisory metadata
7. recent `docs/logs/session/` entries relevant to the task

### Authority rules

- **Read AGENTS.md first.** AGENTS.md is the canonical repo-local operating contract; every agent must start by reading AGENTS.md before consulting any other instruction file. Adapter files (GEMINI.md, CLAUDE.md, etc.) defer to AGENTS.md.
- `GEMINI.md` contains Gemini-specific execution guidance and must defer to `AGENTS.md` for repo authority.
- `.giles/*.yaml` files are advisory mirrors unless a fresh Giles scan or explicit repo policy marks them as promoted evidence.
- Agents record evidence; they do not declare fleet compliance.
- If code, docs, and Giles metadata disagree, preserve the conflict explicitly and log it as a follow-up unless the requested task is to resolve that conflict.

### Documentation patch rules

Documentation changes must be source-grounded.

Allowed:
- update stale paths
- add missing repo-local operating instructions
- update docs indexes
- document existing behavior
- add explicit TODOs or follow-ups for unresolved conflicts
- create or update session logs in the repo's established format

Not allowed without explicit request:
- runtime code changes
- architecture changes disguised as documentation
- invented behavior
- deleting repo-specific constraints because they appear redundant
- converting advisory Giles metadata into compliance claims
- broad reorganization of docs

### Validation expectations

For documentation-only tasks, run the lightest available validation that proves the patch is structurally safe. Prefer:

```sh
git diff -- AGENTS.md GEMINI.md docs README.md
grep -R "GILES:DOCS-GOVERNANCE:START" -n AGENTS.md
grep -R "GILES:GEMINI-DOCS-GOVERNANCE:START" -n GEMINI.md
```

If the repo has doc linting, markdown linting, tests, or a Giles scan command documented locally, run the relevant targeted command and record the result.

### Logging

For non-trivial documentation or governance changes, create or update a session log under `docs/logs/session/` using the repo's established format. If no format exists, create a concise Markdown log with:

* date
* task
* files changed
* validation run
* risks or follow-ups
<!-- GILES:DOCS-GOVERNANCE:END -->
