# TODO

## 2026-09-08 confirmation dialog layering

- [x] **MODAL-LAYER-001** — Put shared dialogs and their backdrops above the
      Outputs pane, retaining visible dropdowns and tooltips inside dialogs.
      Long confirmation content scrolls while action buttons remain visible.
      All 36 focused tests, Desktop typecheck, scoped lint/format checks, and
      the renderer build passed. Browser checks covered normal and compact
      windows. See the [repair record](logs/session/2026-09-08-modal-layering.md).
      Source validated; installed-app replacement is pending the next build/install step.

## 2026-09-08 temporary scratch approvals

- [x] **WDS-TMP-001** — Allow ordinary temporary scratch paths in unrestricted workspace
      sessions, including `/tmp` and its macOS alias. The triggering redirection failed before
      the fix and passed afterward; 32 scope unit tests and 23 permission regressions passed.
      Explicit restriction, read-only roots, and non-temp writes retain their guards. See the
      [repair record](logs/session/2026-09-08-temporary-scratch-permissions.md).
      Source validated; installed-app replacement is pending the next build/install step.

## 2026-09-08 recurring output-preview repair

- [x] **ART-PREVIEW-001** — Restore the live Desktop entrypoint's use of validated
      session document capabilities so Outputs previews do not demand a second
      native picker grant. Six live-entrypoint regression cases, all 1,166 Desktop
      tests, and the original document in the reinstalled application passed;
      see the [repair and installation evidence](logs/session/2026-08-28-artifact-preview-follow-up-repair.md#recurring-preview-denial-investigation-2026-09-08).

The older audit entries below remain historical findings. The current
ADR-0006/0013 contract permits transient, validated exact-file deliverable
capabilities; it does not grant their parent directories. This repair restores
that documented behavior without broadening the existing eligibility filter.

## Open items from the 2026-08-26 clean independent audit

Source: [`docs/cloud/2026-08-26-clean-independent-audit.md`](cloud/2026-08-26-clean-independent-audit.md).
This pass did **not** reuse 2026-08-15 evidence. Playtest was excluded.

### Patch now (High, source-confirmed)

- [x] **SEC-GSL-001** — Artifact routing IPC must not grant arbitrary dirs/files
      (`assertArtifactOutputRootAccess` catch-all + `artifactFiles` with no
      grant check).
- [x] **SEC-GSL-002** — Enforce `GOSLING_ALLOWLIST` in Rust on ACP/HTTP/CLI
      add-extension; stop prefix-matching in the deep-link modal.
- [x] **SECN-GSL-001** — Shell `openExternal` must use the same protocol
      allowlist as desktop `openExternalIfSafe`.
- [x] **WFG-GSL-002** — ACP `tools/permissions/set` must not return success when
      `permission.yaml` persist fails (`crates/gosling/src/acp/server/tools.rs`).
- [x] **WFG-GSL-001** — Desktop “Always Allow all extension tools” must persist
      the bulk grant before resolving the live approval
      (`ui/desktop/src/components/ToolApprovalButtons.tsx`).
- [x] **LLM-GSL-004** — Auto/subagent must not Allow `manage_extensions`
      (`permission_inspector.rs` Auto branch before the RequireApproval arm).
- [x] **WFG-GSL-004** — CLI non-interactive Auto must Deny/abort confirmations,
      including inspector-failure RequireApproval (`gosling-cli` session loop).
- [x] **IOP-GSL-001** — Assistant-mentioned absolute document paths must not
      become Desktop preview grants (`artifacts.rs` + `artifactFileAccess.ts`).
- [x] **DAT-GSL-001** — Same-schema `workspaces.json` validation failure must
      fail-closed, not wipe to Default (`workspace/store.rs`).
- [x] **AOC-GSL-001 / ARC-GSL-002** — Auto must not auto-ack vendor-CLI tool
      confirmations; CLI providers must not silently discard `stream()` tools.
- [x] **LLM-GSL-001 / LLM-GSL-003** — Expand Auto explicit-grant beyond
      write/exec (HTTP tools, mixed-risk MCP).

Closed on 2026-08-26 with focused regression and compile evidence recorded in
[`docs/logs/session/2026-08-26-high-severity-audit-repairs.md`](logs/session/2026-08-26-high-severity-audit-repairs.md).

### Next (Medium, confirmed)

- [x] **CAS-GSL-001** — Imported `imported_untrusted` history must be labeled or
      stripped at the model boundary, not only UI/artifacts.
- [x] **CON-GSL-002** — `permission.yaml` needs the same cross-process flock as
      `config.yaml`. Permission-file locking and fresh reads were repaired and
      tested on 2026-09-07 as **CON-GSL-901**; see the
      [permission repair report](cloud/2026-09-07-permissions-repair.md).
- [x] **WFG-GSL-005** — Chat mode should omit tools from the provider payload;
      skips must not render as tool success.
- [x] **NEG-GSL-006** — Unix session-dir `0o700` failure should abort pool init.
- [x] **CMP-GSL-001 / CMP-GSL-003** — README “70+ full compatibility” and
      prompt-injection default-on docs.

CAS-GSL-001, WFG-GSL-005, NEG-GSL-006, CMP-GSL-001, and CMP-GSL-003 were
closed on 2026-08-27 with focused regression, compile, Clippy, formatting, and
source/documentation consistency evidence in the second batch of
[`docs/logs/session/2026-08-27-medium-defect-campaign.md`](logs/session/2026-08-27-medium-defect-campaign.md).
Correction on 2026-09-07 (**CMP-GSL-901**): the earlier reconciliation incorrectly
attributed permission-file locking to `37804170e`. That commit closed the
`config.yaml` read-modify-write issue recorded below; it did not change
`permission.yaml`. The permission-file issue is now separately repaired, with
independent-process writer and existing-reader revocation regressions recorded
in the [permission repair report](cloud/2026-09-07-permissions-repair.md).

### Third criticality batch

- [x] **REL-GSL-001** — ACP prompt runs now register their cancellation token
      with `AgentManager`, so its LRU busy check cannot evict an in-flight ACP
      agent.
- [x] **CON-GSL-003** — shared atomic file writers use a UUID staging path per
      publication instead of a shared `<stem>.tmp`.
- [x] **IOP-GSL-002** — ACP/JSON and Nostr imports record and reuse the same
      source-payload fingerprint as CLI file imports.
- [x] **DAT-GSL-003** — deleting a session removes its session-private library
      rows in the same SQLite transaction while preserving project items.
- [x] **WFG-GSL-006** — the Ink permission prompt exposes the complete raw
      payload through a bounded, pageable region instead of ellipsizing it.

These five findings were closed on 2026-08-27 with focused Rust and TUI
regressions, Rust compile/Clippy/formatting, TUI typecheck, and scoped diff
evidence in the third batch of
[`docs/logs/session/2026-08-27-medium-defect-campaign.md`](logs/session/2026-08-27-medium-defect-campaign.md).
`DAT-GSL-002` was investigated but not counted in that batch: deleting
workspace-keyed library state conflicts with the accepted pinned-session
preservation contract in ADR-0015 and the existing workspace deletion
regression. The later maintainer-authority pass closed it by recording and
preserving that contract rather than deleting the retained data.

### Medium completion batch

- [x] **EAPI-GSL-001** — streamable-HTTP MCP clients now enforce the configured
      extension timeout as a total request timeout, including a stalled body.
- [x] **WEB-GSL-001** — tool states now use distinct accessible icons and labels
      instead of a color-only two-pixel dot.
- [x] **IOP-GSL-005** — updater downloads and archive extraction are bounded by
      compressed size, expanded size, and entry count.
- [x] **AID-GSL-001** — the architecture diagram now names session schema v28.
- [x] **XREPO-GSL-001** — the documented live Goose compatibility adapter now
      has a parity test that fails when its build-time and browser converters
      diverge. The live catalog policy remains intentional.
- [x] **RST-GSL-001** — reinspection found explicit workflow permissions in
      every current GitHub Actions workflow; no patch was needed.
- [x] **ACP-GSL-001** — a shared SQLite write gate is acquired before pool
      checkout, so queued `BEGIN IMMEDIATE` writers cannot exhaust every ACP
      connection while waiting for the write lock.
- [x] **ACP-GSL-002** — ACP normalizes external-tool providers out of Auto and
      exposes/persists Manual approval mode before a prompt is submitted.

The source findings and two installed-app ACP defects were closed on 2026-08-27
with focused regressions, Rust compile/Clippy/formatting, Desktop tests and
typecheck, documentation tests, release packaging, reinstall, and installed-app
Solo/Dual smoke evidence. See the completion section of
[`docs/logs/session/2026-08-27-medium-defect-campaign.md`](logs/session/2026-08-27-medium-defect-campaign.md)
and [`reports/2026-08-27-medium-defect-campaign.md`](../reports/2026-08-27-medium-defect-campaign.md).

`AOC-GSL-001` and `ACP-GSL-002` record the conservative policy implemented
from the audit evidence available at that time. They were superseded on
2026-08-27 by the operator-authorized autonomous permission policy below; the
historical records remain intact rather than being rewritten as if they had
never shipped.

### Operator-authorized autonomous permission policy

- [x] **AUT-GSL-001** — Auto is the product default and remains selectable for
      providers that execute tools outside Gosling. Provider-native tool calls
      proceed autonomously in Auto instead of failing or opening a prompt.
- [x] **AUT-GSL-002** — ordinary ACP provider permission requests proceed in
      Auto and Smart Approve. Requests carrying an explicit security warning
      still require an operator decision; Chat denies and Approve prompts.
- [x] **AUT-GSL-003** — ACP “Always Allow” decisions persist in
      `permission.yaml`, scoped to provider and tool, and are reused across
      sessions, threads, and restarts. Domain-specific and explicit-security
      decisions are never promoted into reusable tool-wide grants.

These records close the repeated installed-app WebSearch/WebFetch approval
regression observed in session `20260828_51`. Implementation and installed-app
evidence are recorded in
[`docs/logs/session/2026-08-27-auto-permission-persistence.md`](logs/session/2026-08-27-auto-permission-persistence.md).

### Workspace approval regression (2026-09-07)

- [ ] **WDS-GSL-001** — a comment apostrophe merges later shell commands into a
      false `/dev/null...` mutation target, prompting in Autonomous mode. Source
      repair and regression cases are prepared; Rust test execution and installed-app
      verification remain pending. See the
      [repair record](logs/session/2026-09-07-shell-comment-scope-prompts.md).

### Remaining Medium decisions and external prerequisites

- [x] **DAT-GSL-002** — workspace deletion preserves workspace-keyed project
      library data because pinned sessions retain the workspace snapshot and
      may still need those rows. The existing deletion path mutates only the
      workspace store; ADR-0015 and the workspace-service regression preserve
      this contract.
- [x] **NEG-GSL-001** — MCP Apps are untrusted interactive views, not
      independent chat actors. App-proposed chat text requires explicit user
      confirmation before it enters the transcript as user input, and app tool
      calls retain Gosling visibility and permission inspection.
- [x] **RSP-GSL-001** — the dependency graph no longer contains the vulnerable
      RSA path covered by RUSTSEC-2023-0071, so the stale deny exception was
      removed. A current `cargo-deny check advisories` validates the graph.
- [ ] **ARC-GSL-003 / ARC-GSL-004 / ARC-GSL-005** — provider-port, MCP
      dependency, and process-global state changes require an architecture pass.
- [x] **INV-GSL-001** — imported snapshots restore untrusted conversation and
      non-executable extension state only. They never restore provider/model,
      workspace, credential profile/binding, folder grants, enabled executable
      extensions, workflow ownership, or tool-permission authority; the caller
      selects the working directory and the new session starts in Approve.
- [ ] **CMP-GSL-004** — run a fresh Giles scan before changing advisory stale
      compliance YAML or promoting it to repo truth.
- [x] **ACP-GSL-003** — managed-context/external-tool providers remain valid in
      Solo Research but are excluded from every multi-model seat because those
      delegates cannot safely run with Gosling-hosted research tools. The
      multi-model prompt and Summon schema require ad-hoc delegates to omit
      `source`; focused selector and prompt regressions enforce both boundaries.

### Needs a design decision

- [x] **NEG-GSL-005** — `goslingd` is an enforced single-operator local control
      plane. Configuration rejects wildcard, LAN, VPN, public, and other
      non-loopback bind addresses, and the external-server guide documents only
      a separately managed process on the same host.
- [x] **PATH-GSL-001** — resolved by documenting the intentional shared AAIF
      interoperability path in the README coexistence contract while keeping
      product-owned configuration, databases, keyring, and deep links isolated.
- [x] **CON-GSL-001** — session schema v29 adds a durable, heartbeat-backed
      per-session turn lease at the shared Agent reply boundary. A second live
      process/window fails before message persistence or compaction; owner-matched
      release and stale/dead-owner takeover preserve crash recovery.

### Follow-up (from late SEC/REL fold-in)

- [x] **SEC-GSL-003** — goslingd's unauthenticated MCP proxy and guest routes
      require loopback connection metadata in addition to the server-wide
      loopback bind invariant.
- [x] **SECN-GSL-002** — main BrowserWindow needs `will-navigate` like the shell.
- [x] **REL-GSL-001** — ACP in-flight turns pin their AgentManager LRU entry;
      closed in the third 2026-08-27 criticality batch above.
- [x] **FSR-GSL-002** — do not persist-drop a failed MCP from the enabled set.
- [x] **REC-GSL-001** — publish the Desktop backend PID registry atomically.
- [x] **REL-GSL-002 / RES-GSL-001** — host default timeouts for shell and
      computercontroller.

The five checked findings above were closed on 2026-08-27 with focused
regression, typecheck, compile, lint, formatting, and Clippy evidence recorded
in the source audit's repair appendix.

### Upstream-triage and reliability follow-up — 2026-09-05

Source: [`docs/logs/session/2026-09-04-upstream-goose-v149-triage.md`](logs/session/2026-09-04-upstream-goose-v149-triage.md)
and [`docs/logs/session/2026-09-05-deep-research-stall-write-gate-deadlock.md`](logs/session/2026-09-05-deep-research-stall-write-gate-deadlock.md).

- [x] **UPSTREAM-GSL-001** — provider errors that have no HTTP status now use a
      URL-free message, so query credentials cannot reach the terminal fallback.
- [x] **UPSTREAM-GSL-002** — non-streaming OpenAI and OpenAI-compatible JSON
      responses are capped at 10 MiB before parsing.
- [x] **UPSTREAM-GSL-003** — Bedrock tool names, descriptions, and JSON-schema
      string values strip hidden Unicode prompt-control tags before serialization.
- [x] **UPSTREAM-GSL-004** — invalid `GOSLING_MODE` values no longer become
      autonomous mode in ACP providers; only an absent setting uses the default.
- [x] **UPSTREAM-GSL-005** — OpenAI streaming retains a tool name that arrives
      after its tool-call id.
- [x] **REL-GSL-003** — extension root notifications clone their client handles
      before awaiting remote calls, so a stalled extension no longer holds the
      global extension-map mutex.

The following findings were held open for an explicit owner decision rather than
a guessed timeout or policy. All four were decided and implemented on
2026-09-07; the reasoning behind each contract is recorded at its
implementation site and in
[the session log](logs/session/2026-09-07-reliability-decision-register.md).

- [x] **REL-GSL-004** — a synchronous `delegate` runs under a 30-minute
      wall-clock budget (`GOSLING_SYNC_DELEGATE_TIMEOUT_SECS`, `0` to disable).
      On expiry the delegate's cancellation token fires, it is given the same
      5-second unwind grace the `load(cancel: true)` path uses, and only then
      aborted. The tool call returns an **error** naming the budget and the
      subagent session that holds the partial work, and states that the task
      was not retried — re-running it would repeat any side effects that
      already landed. Background delegates keep their existing `load` contract.
- [x] **REL-GSL-005** — yes, a live owner's lease can expire, and it expires on
      heartbeat staleness alone at the existing 90-second TTL. Requiring the
      owner process to be dead first would mean the only way to recover a
      wedged turn is killing the app, which is what the 2026-09-05 deadlock
      actually required. A long turn proves liveness solely through its
      15-second heartbeat. What makes takeover safe is new **fencing**:
      takeover deletes the lease row, the evicted owner's next heartbeat
      updates zero rows, and it responds by cancelling its own turn, so a
      session that changes hands never has two writers. A renewal that *fails*
      is not revocation. A dead owner's lease is still free immediately, so
      crash recovery is unchanged.
- [x] **REL-GSL-006** — a `started` operation becomes visible `in_doubt` when
      its owner is not the process currently holding the session's turn lease.
      A live owner *process* is not a running turn; requiring only process
      liveness is what left three operations `started` for hours on 2026-09-05
      until an app restart. A tool call can only execute inside a turn, so an
      owner that no longer holds the turn has nothing in flight to interrupt.
      Recovery still surfaces the row as `in_doubt` and never as retryable, so
      an operation whose side effects did land is never repeated automatically.
- [x] **ARC-GSL-006** — `gosling_providers::transport_policy::TransportPolicy`
      is enforced for every provider client built through `ApiClient` and for
      the Ollama toolshim. A base URL must be `https`, or `http` on a loopback
      host; plaintext to any other host requires
      `GOSLING_ALLOW_INSECURE_PROVIDER_TRANSPORT` and logs a security event.
      Redirects may not downgrade `https`, may not change host or port, and are
      capped at four hops — `reqwest` drops `Authorization` across an origin
      change but not vendor API-key headers. This closes the last unported item
      from the [2026-09-04 upstream triage](logs/session/2026-09-04-upstream-goose-v149-triage.md);
      Snowflake keeps its own stricter vendor check.

## Open items from the 2026-08-15 exhaustive audit — recorded 2026-08-16

The repair campaign (`docs/logs/session/2026-08-16-audit-repair-campaign.md`)
closed roughly 62 of the 94 live High/Medium findings plus most Low items across
19 gated groups, merged as `c828a5895`. What remains is listed here so it is not
rediscovered from scratch.

### Needs a design decision, not a patch

- [x] **SEC-GOS-002** — closed in `5ea594f4b`. The guest CSP is derived
      server-side from declared domains and keyed to a single-use proxy token;
      verified live that a forged token carrying `default-src *` is refused.
- [x] **SEC-GOS-007** — superseded by the explicit local-control-plane product
      boundary. The unauthenticated MCP proxy and guest routes now require a
      loopback peer, while the state-changing guest POST retains its in-handler
      nonce authentication and server-derived CSP.
- [~] **SECN-GSL-001** — **Warning, not actionable. Closed as re-assessed
      2026-08-16, not as fixed.** The finding assumed untrusted MCP app HTML
      runs in the frame whose URL carries the backend secret. It does not: that
      frame is the proxy page, and the app runs in a nested guest iframe on a
      *different origin*, so same-origin policy blocks reading
      `parent.location`. The guest only gets a single-use nonce.

      Both variants are safe, for **different** reasons — and each is one edit
      from being unsafe:
      * `crates/gosling/src/acp/` — guest served from its own loopback listener
        on its own port, so `allow-same-origin` means the guest's origin.
        **Do not merge that route into the main ACP router.**
      * `crates/gosling-server/` — guest shares the router but drops
        `allow-same-origin` (opaque origin), which is also what makes its
        `srcdoc` fallback safe. **Do not add `allow-same-origin` there.**

      Both invariants are commented at their sites and
      `acp::mcp_app_proxy::tests::the_guest_is_served_from_its_own_loopback_origin`
      fails if the ACP guest stops owning its origin. Upstream goose is worse
      here (secret in the query string) and deleted its same-origin variant
      rather than fixing it, so there is nothing to port. Residual hardening
      only: the outer page's `script-src` is widened by app-declared domains,
      with no injection sink found in that page.

### Blocked on tooling

- [ ] **RSP-GSL-002 / RSP-GSL-003** — a secret-scanning job and
      `[licenses]`/`[bans]`/`[sources]` in `deny.toml`. `cargo-deny` is not
      available in the dev environment, so neither could be validated; shipping
      unverified CI config that fails on first run is worse than the open item.

### Deliberately not fixed, with reasoning recorded in-tree

- **SEC-GOS-011** — failing closed on an absent WebSocket `Origin` was
  implemented and tested live: it returns 403 to every non-browser ACP client
  while blocking no browser attack, because the spec requires browsers to send
  `Origin`. Reverted; reasoning is at the call site.

### Closed in later repair batches — 2026-08-16

- [x] **SEC-GOS-002** (`5ea594f4b`) — MCP-app guest CSP is derived server-side
      from declared domains and keyed to a single-use proxy token; a forged
      token carrying `default-src *` is refused. Verified live.
- [x] **LLM-GSL-004 / NEG-GSL-002** (`60e72c61a`) — project hints
      (`.goslinghints`, `AGENTS.md`) are labelled with their repo provenance and
      the scanner's "untrusted data, not commands" wording instead of sharing
      the operator's Additional Instructions framing.
- [x] **LLM-GSL-010** (`60e72c61a`) — an unbounded, model-chosen delegate
      extension grant now emits a security event naming the extensions.
- [x] **WEB-GOS-001** (`60e72c61a`) — approval buttons ranked by consequence;
      a persistent grant no longer looks identical to a one-shot allow, and
      Deny is no longer the faintest control.
- [x] **WEB-GOS-002** (`60e72c61a`) — the approval prompt discloses the full
      argument instead of a 140-character first line, so a multi-line command
      cannot be approved unseen.
- [x] **Upstream port** (`60e72c61a`) — `form-action 'none'` added to both
      MCP-app CSP builders, from goose `34adc70f1` (PR #10985). Neither gosling
      variant had it while both emit `allow-forms` on the guest sandbox.
- [x] **CON-GSL-002** (`37804170e`) — the four `config.yaml` read-modify-write paths
      hold the `.save.lock` flock across read, mutate, and write, not just the
      write. The new test fails deterministically without the lock, and the fix
      also removed a self-deadlock where `load_write_config` persisted
      migrations by re-acquiring the same flock.
- [x] **SEC-GOS-005** (`8e7bb759e`) — relay URLs from a shared `nevent` must be
      public `ws`/`wss` endpoints; private, link-local, and cloud-metadata
      destinations are refused. `nostr` feature only.

### Closed in the second repair batch — 2026-08-16

- [x] **STT-GOS-001** (`886c8df8b`) — Chat mode executed frontend tool
      requests because the execution loop sat above the Chat branch. Residual:
      verified structurally and by compile, not by a runtime test.
- [x] **STT-GOS-005** (`886c8df8b`) — permission write failures were swallowed;
      `persist` and the mutators now return the error and every call site
      handles it deliberately.
- [x] **ARCN-GSL-001** (`6a02881fb`) — the CSP handler keyed the ACP lease
      lookup by webContents id instead of `BrowserWindow.id`, so the CSP
      omitted the local ACP origin.
- [x] **SECN-GSL-002** (`6a02881fb`) — the extension allowlist fetch now
      requires https and is bounded by timeout and size.

See `docs/logs/session/2026-08-16-audit-repair-batch2.md`.

### High severity, repaired 2026-08-16 (repair-defect-campaign)

The four items below were confirmed open against `docs/logs/session/2026-08-16-audit-repair-campaign.md`'s
"Open / not yet started" list and `docs/logs/session/2026-08-16-acp-mcp-repair.md`'s
inventory table — no fix commit, absent from every "Closed" section above.
Nothing in this repo's own severity scheme uses bare "P0/P1"; High was the top
populated tier that cycle (Critical count was zero per the 2026-08-15 audit's
own tally). Three are now fixed; the fourth was scoped out as too large for a
patch and routed. Full campaign evidence:
[`docs/logs/session/2026-08-16-repair-defect-campaign.md`](logs/session/2026-08-16-repair-defect-campaign.md).

- [x] **AOC-GOS-004** (`1bf5a6ddb`) — `build_spec_from_agent` now drops a
      capability policy declared in a repo-committed agent file
      (`source.global == false`) instead of honoring it, so a cloned repo can
      no longer grant its own delegate an extension the parent has enabled
      just by declaring `capabilities: {extensions: [...]}`. Global
      (operator-authored) agent files are unaffected. Regression tests cover
      both. `docs/cloud/2026-08-15-audit-orchestration-contracts.md:234`.
- [x] **CON-GSL-001** (`c314dae6a`) — `recover_tool_operations` now probes the
      dispatching OS process (`tool_operations.owner_pid`, schema v27) via the
      existing `subprocess::process_is_alive` before treating a foreign
      `started` row as abandoned, instead of trusting a per-instance UUID with
      no liveness signal. A live peer's in-flight tool survives a concurrent
      recover; a genuinely dead owner's is still recovered.
      `docs/cloud/2026-08-15-audit-dataflow-core.md:187`.
- [x] **MCP-GOS-001** (`72b23086d`) — `automation_script` and `computer_control`
      (all platform `#[cfg]` variants) now carry MCP `destructive_hint` /
      `open_world_hint` tool annotations, so a host other than Gosling's own
      ACP layer (which already gates these by name) has a spec-level signal
      that they differ from the read-only tools on the same server.
      `computercontroller` was already disabled by default in both extension
      registries — that part of the mitigation needed no change. Splitting
      the server into separate read/exec servers is a product-policy call the
      audit itself routes to a human owner, not part of this patch.
      Severity was inconsistent between docs (High in
      `docs/cloud/2026-08-15-audit-orchestration-contracts.md:284-286` vs.
      Medium in `docs/logs/session/2026-08-16-acp-mcp-repair.md`'s inventory
      table); treated as High, the audit-of-record.
- [ ] **ARC-GSL-002** — `gosling-providers` crate still owns the conversation
      domain (inverted ownership: `Message` and friends live in the adapter
      crate, not core). Not fixed: the audit's own recommended mitigation is
      moving `conversation`/`gosling_mode`/`thinking`/`permission` into a
      domain crate, Cost L, "many import sites" across `gosling`,
      `gosling-cli`, `gosling-server`, and generated SDK types — a crate-
      boundary move, not a same-crate refactor, and the audit's own non-goal
      says not to fold provider consolidation into the same slice. Routed to
      a dedicated architecture pass rather than attempted as a repair-campaign
      patch, matching how `ARC-GSL-001`'s >=2000-line files were routed
      instead of split mid-repair (below).
      `docs/cloud/2026-08-15-audit-architecture-invariants.md:352`. Two
      `Provider` traits and 21 concrete impls left in core blur the boundary
      further (`docs/cloud/audit-architecture-seam.md:126`).

### Ledger correction

- [x] **SEC-GOS-012** (`6a02881fb`) — missing from this file even though the second
      repair batch fixed it: `--dangerously-unauthenticated` now refuses a non-loopback
      bind. See `docs/logs/session/2026-08-16-audit-repair-batch2.md`. (The batch's own
      closing note claimed "all five marked closed" here; only four were. Adding the
      fifth now.)

### Performance findings — reassessed 2026-08-17

Source audit: [`docs/cloud/audit-performance-profile.md`](cloud/audit-performance-profile.md).
This ledger previously tracked only PERF-GSL-002 (below). The full PERF-GSL-001
through 004 series and the new streaming finding recorded here are carried from
that audit's §5/§6 inventory so they are not rediscovered from scratch. Full
re-assessment and a new finding are in
[`docs/logs/session/2026-08-17-performance-review.md`](logs/session/2026-08-17-performance-review.md).

- [x] **PERF-GSL-001** — resolved 2026-08-17: the README now labels the command
      timings as historical and explicitly says they are not startup benchmarks.
      No performance numbers were changed or claimed for HEAD. A ready-to-prompt
      comparison remains a future measurement, not a prerequisite for honest docs.
- [x] **PERF-GSL-002** — resolved 2026-08-17: the Desktop E2E suite now has an
      opt-in, provider-independent renderer-readiness harness. It launches a fresh
      Electron process for at least five samples and reports p50/p95; page-cache
      state remains explicitly uncontrolled. Run with
      `GOSLING_RUN_PERFORMANCE=1 GOSLING_PERFORMANCE_RUNS=10 pnpm test-e2e -- performance.spec.ts`.
- [~] **PERF-GSL-003** — the avoidable clones are reduced: MOIM now borrows the
      conversation and only allocates a replacement when it injects context, and
      tool-pair summarization only clones after finding eligible pairs. Per-turn
      full-history tokenization and the remaining session reload are still open;
      no wall-time profile was captured, so this is not claimed as a measured win.
      The process-wide LRU encode-cache already removes the expensive re-encode;
      the residual includes blake3 keying on every cache hit and session reloads.
      The stale clone call-site claims were removed after current source
      inspection. Not fixed: per Amdahl this sits behind `p ≀ 0.01` and the
      audit's own §6 says do not touch it until a profile (the PERF-GSL-003
      break-it harness) shows a non-trivial share.
      `audit-performance-profile.md:284`.
- [x] **PERF-GSL-004** — resolved 2026-08-17: fallback pattern scanning now uses a
      case-insensitive `RegexSet` to select matching patterns before running
      `find_iter` only for those patterns. Existing match behavior and ordering are
      preserved by the focused pattern suite. No scanner input is truncated.
- [x] **PERF-GSL-005** — the OpenAI-compatible SSE decoder now deserializes each
      data line once into a typed `StreamingPayload`, checks both supported server
      error shapes, and moves the decoded fields into `StreamingChunk`. Focused
      tests preserve successful chunks, nested errors, object errors, and malformed
      missing-`choices` rejection. Implemented 2026-08-17 in
      `gosling-providers/src/formats/openai.rs`; validation and remaining
      measurement limits are recorded in
      [`docs/logs/session/2026-08-17-performance-review.md`](logs/session/2026-08-17-performance-review.md).

#### Performance repair follow-up — 2026-09-02

Source findings: PERF-GSL-006 through PERF-GSL-010 from the 2026-09-02
performance audit. Confirmation evidence, validation, and residual trade-offs are recorded in
[`docs/logs/session/2026-09-02-performance-repair.md`](logs/session/2026-09-02-performance-repair.md).

- [x] **PERF-GSL-006** — resolved 2026-09-02: FTS recall now materializes the
      bounded match set first and counts messages once for each matched session.
      A 10,000-message/50-hit SQLite scanstats harness reduced indexed count-row
      visits from 500,000 across 50 correlated executions to 10,000 in one
      execution while preserving the exact result.
- [x] **PERF-GSL-007** — resolved 2026-09-02: schema migration 31 and the fresh
      schema add `(session_id, created_timestamp, id)`. `EXPLAIN QUERY PLAN` now
      selects that index for `get_conversation` and no longer reports a temporary
      B-tree sort.
- [x] **PERF-GSL-008** — resolved 2026-09-02: replay-buffer accounting serializes
      into an exact counting writer, eliminating the discarded output `Vec` while
      retaining the existing failure fallback and byte-bound behavior.
- [x] **PERF-GSL-009** — resolved 2026-09-02: reply telemetry builds the latest
      tool-request-name index once, updates it as requests arrive, rebuilds it on
      history replacement, and resolves responses with a hash lookup.
- [x] **PERF-GSL-010** — resolved 2026-09-02: the Desktop render index precomputes
      prior-model and intervening-switch state in one pass; disclosures now use
      constant-time array reads and preserve recorded-switch suppression.

#### Snippet optimization — 2026-09-06

- [x] **REL-OPT-001** — resolved 2026-09-06: chat-list snippets borrow visible
      text and stop at known truncation, avoiding full-text normalization buffers
      and image/tool-content copies. The same local debug-build fixture reduced
      a 20-session list median from 149.93 ms to 31.26 ms; this is not a
      production or release-build latency claim. All 147 session tests and
      crate-wide all-target Clippy pass; follow-up patch review has no remaining
      findings. Discovery, benchmark limits, and the repaired test/lint issues
      are recorded in [the session log](logs/session/2026-09-06-snippet-optimization.md).

#### Session storage optimization — 2026-09-06

- [x] **REL-OPT-002** — resolved 2026-09-06: session listing derives each
      session's newest message time from two index-range maxima instead of
      aggregating a join over every message row, and the paged path counts
      messages only for the returned page (the unpaged path still counts every
      matching session). Same-harness debug-build fixture of 300
      sessions × 100 messages: first-page median 36.06 ms → 2.14 ms, unpaged
      list 41.17 ms → 6.92 ms. Exact ordering, `message_count`, and
      `last_message_at` are asserted against per-session `get_session`
      aggregates, including millisecond/second timestamp mixes and empty
      sessions. Not a production or release-build latency claim.
- [x] **REL-OPT-003** — resolved 2026-09-06: `begin_tool_operation` checks the
      newest checkpointed copy of a tool request id, so the `json_each` scan
      stops at the most recent message instead of parsing every message in the
      session under the write lock on every tool dispatch. Same-harness fixture
      of 300 tool rounds (about 6 MB of content): median 7.89 ms → 0.19 ms per
      dispatch. Newest-wins now matches the sibling artifact-discovery lookup.
      Discovery register, benchmark limits, and follow-up review for both items
      are in [the session log](logs/session/2026-09-06-session-storage-optimization.md).

#### v1.2.1 release preparation — 2026-09-06

- [x] **REL-CI-001** — resolved 2026-09-06: `resolve_tool` again falls back to
      owner-prefixed tool names (`code_execution__list_functions`) when the name
      is not in the advertised list but the prefix names a live extension.
      Commit `04114c2c7` removed that fallback without mentioning it in its
      message, which broke `test_prompt_codemode` and left the repository's CI
      test gate red on `main` from 2026-09-05 until this release. Platform
      extensions advertise their tools unprefixed, so a model addressing one by
      owner failed its turn. A prefix naming no extension still fails, and
      `test_resolve_tool_accepts_owner_prefixed_name_for_unprefixed_tool` guards
      both directions. Found while preparing the v1.2.1 release; see
      [the release notes](../documentation/docs/release-notes/v1.2.1.md).

- [x] **REL-CI-002** — resolved 2026-09-07: both tests now pin the threshold
      with `env_lock` for their own duration, so they assert against the 0.8
      default they were written for instead of the operator's real config.
      Scoping the variable to those two tests rather than the whole run is what
      makes it safe: `acp_custom_requests_test` runs in a separate binary and
      never sees it, so the collateral failure recorded below does not occur
      (verified — 17 passed). A third test of the same class surfaced while
      validating this and was fixed alongside it:
      `merge_environments_keeps_the_original_error_when_nothing_is_declared`
      read the operator's process environment, so exporting
      `MUNINN_MCP_BEARER_TOKEN` (which running the Muninn MCP server does)
      made its "no credential anywhere" assertion fail. Original report:
      `test_compaction_fires_before_first_llm_call`
      and `test_compaction_fires_inside_reply_loop` in `crates/gosling/tests/compaction.rs`
      read the operator's real `Config::global()` for `GOSLING_AUTO_COMPACT_THRESHOLD`
      instead of the default they assert against. Both fixtures sit between the
      0.8 default boundary (102,400 of 128,000) and a raised one, so on a machine
      whose config sets 0.95 they report that compaction never fired. They pass
      with `GOSLING_AUTO_COMPACT_THRESHOLD=0.8` and in CI, which has no operator
      config, so this is a test-isolation defect, not a compaction regression.
      Setting that variable is not a workaround: it then makes
      `test_custom_preferences_read_save_remove` in `acp_custom_requests_test`
      fail, because that test asserts an exact preference list and the variable
      adds an `autoCompactThreshold` entry. Both tests read the real
      `Config::global()`; isolate them the way `93a19738d` isolated the
      summarizer-mode tests.

### Lower priority, mechanical but needs a judgement call

ARCN-GSL-002 (49 scattered `process.env` reads), ARC-GSL-005 (duplicated
`GOOSE_EXCLUDED_SKILL_IDS` across a TS/JS boundary), MEM-GSL-004 (TUI `turns` grows
unbounded; capping changes reachable scrollback), DAT-GSL-006 (session create +
extension apply are two commits), NEG-GSL-003 (`GOSLING_SHELL` flavor is
unmodeled scanner input), ARC-GSL-001 (three files over 4000 lines, routed to a
dedicated modularization pass rather than split mid-repair).

### Low-severity mechanical completion — 2026-08-27

- [x] **BUILD-GSL-001** — Deep Research session validation compared a `String`
      workspace root using the nonexistent `String::as_path`, breaking the
      `gosling` test build. It now performs the intended `Path` comparison.
- [x] **PROC-GSL-001** — Linux zombie processes were treated as live by process
      ownership/recovery probes and orphan-cleanup regressions. Liveness now
      distinguishes `/proc` zombie state from a running process.
- [x] **TEST-GSL-001** — the permission persist-failure regression depended on
      Unix mode bits, which root can bypass. It now creates a deterministic
      non-directory parent failure and proves rollback under every user ID.
- [x] **INV-GSL-002** — new databases, historical migrations, and foreign
      transcript normalization now use the runtime `SmartApprove` default;
      schema and import regressions pin the contract.
- [x] **INV-GSL-003** — slash-command metadata now owns its typed builtin
      handler, eliminating the independently maintained advertisement and
      dispatch lists. A uniqueness regression prevents ambiguous names.
- [x] **CMP-GSL-002** — coexistence copy now discloses that Gosling
      intentionally shares AAIF interoperability paths such as `~/.agents`
      while isolating product-owned state.
- [x] **AID-GSL-002** — `CUSTOM_DISTROS.md` points to the existing TypeScript
      SDK instead of the forbidden, absent Desktop OpenAPI client.
- [x] **SIG-GSL-005** — reinspection found the scanner now returns an error if
      enabled ML detection cannot initialize any classifier and reports partial
      classifier initialization through warnings; the earlier log-only finding
      is stale and no code change is needed.

### Known-failing test predating this work

- [x] `context_mgmt::summarizer::tests::defaults_to_off` (`93a19738d`) — was
      never a production defect: the test called the bare `summarizer_mode()`,
      which reads `Config::global()`, the real process-wide config singleton
      keyed by this machine's actual config dir. On any machine with a real
      settings file setting `GOSLING_SUMMARIZER` (this dev environment's own
      `~/.config/gosling/config.yaml` has `GOSLING_SUMMARIZER: on`, a
      deliberate personal setting, left untouched), that ambient value beat
      the built-in default and the test failed for reasons unrelated to the
      code under test. Fixed by testing `summarizer_mode_from` against an
      isolated, temp-file-backed `Config`, matching the neighboring
      `settings_file_values_are_honored_and_env_overrides_them` test's
      existing pattern.

## Provider follow-up — observed 2026-08-16

- [x] **Mistral API rejects replayed assistant reasoning.** Resolved 2026-08-23
      in `8e1501aff`: the bundled Mistral profile now sets
      `preserves_thinking` to false, so stored thinking is not serialized as
      the unsupported `assistant.reasoning_content` request field. The focused
      profile regression, all provider tests, the full `gosling` library suite,
      build, and warning-denying Clippy pass. See
      [`2026-08-23-mistral-reasoning-content-422.md`](logs/session/2026-08-23-mistral-reasoning-content-422.md).
- [x] **Grok / xAI OAuth tool-schema rejection.** Fixed in `11806887c` —
      `formats/openai.rs::object_rooted_parameters` coerces union-rooted MCP
      tool schemas to an object root before they reach the provider. Residual:
      this is a compatibility shim at the provider seam; `math_mcp__math_analyze`
      still declares an `anyOf`/`oneOf` root upstream.
      Original report:

- [x] ~~**Grok / xAI OAuth tool-schema rejection.**~~ Duplicate historical row;
      closed by the provider-seam normalization recorded immediately above.
      The original report was: `gosling` with the
      `xai_oauth` provider fails a tool call with
      `Bad request (400): math_mcp__math_analyze: tool parameter root must be an
      object type (root schema is an anyOf/oneOf union with a non-object
      branch)`. Reported repeatable. Investigate whether Gosling forwards MCP
      tool schemas that xAI rejects, and normalize them at the provider seam.
- [x] **Mistral (`vibe`) CLI as an ACP provider option.** Done —
      `crates/gosling/src/providers/vibe_acp.rs`. Uses the `vibe-acp` console
      script the `mistral-vibe` package ships, so it is a normal `AcpProvider`
      registration rather than a CLI scraper. Verified end to end through the
      built binary. Follow-up worth knowing: Gosling's `Chat` maps to Vibe's
      `plan`, which writes a plan file under `~/.vibe/plans/` instead of
      running nothing — usable for planning, not a no-side-effects mode.
      This is the fleet's original `vibe-acp` integration: cuttlefish's
      `VibeAcpEngine` mirrors this module's wire behavior (session mode mapping,
      ACP method sequence) rather than re-deriving it independently — check here
      first if a `vibe-acp` protocol assumption needs to change anywhere in the
      fleet.

## Gemini OAuth retirement — 2026-08-23

- [x] **Gemini OAuth provider** — retired from the provider registry because its
      browser sign-in flow is not reliable. Gemini remains available through
      `Google Gemini (API Key)` with `GOOGLE_API_KEY`; no existing OAuth token
      cache was deleted. The provider registry regression is in `init.rs` and
      the generic OAuth-error rendering regression remains in
      `ProviderConfigurationModal.test.tsx`.

## Shared project-shell readiness — reassessed 2026-08-13

The host/process ACP foundation is merged, but the post-Gate-4
[readiness reassessment](build/shell-productization/readiness-reassessment.md) found that it is not
yet consumable by separate project shells. The renderer is hard-coded and lifecycle-only; main-owned
ACP exposes no safe renderer prompt/update/permission service; the Rust domain-adapter trait has no
production registration path; package metadata/resources are not fully project-neutral; and reusable
shell workflows are absent. R0 repaired the Linux V8 helper and restored the baseline; three
successive `main` CI runs through `31744291492` completed successfully. Reverify current CI before
execution, but do not mistake baseline health for project-shell readiness.

Forward Gates 5–8 are superseded by the
[project-shell readiness plan](build/shell-productization/project-shell-readiness-plan.md). R0 is
complete. Follow the focused
[pre-GUI backend implementation plan](build/shell-productization/pre-gui-backend-implementation-plan.md)
to freeze and implement R1–R4 before adding shared UI or widening preload. Named adapters, prompts,
workflows, UI, branding, real publication, and updater promotion remain outside this campaign. A
DAWES, math, or other named shell begins only after milestone M5 proves a copy-free neutral consumer
end to end, unless the operator explicitly accepts a narrower development-only exception.

## v1.0.0 release preparation — historical

- [x] Prepare the README, release notes, release process, release checklist, user-manual entry points, documentation index, inventory, and stewardship status for v1.0.0.
- [x] Preserve the historical v0.0.6 note and audit/playtest evidence as point-in-time records.
- [x] Preserve the original v1.0.0 preparation record without rewriting its published historical tag.

## v1.1.0 release readiness — 2026-08-23

- [x] Select `v1.1.0` as the next candidate and preserve the noncanonical historical `v1.0.1-optimization-and-workspaces` tag.
- [x] Align the workspace, lockfile, Desktop package, and OpenAPI version surfaces to `1.1.0`.
- [ ] Complete every source, documentation, packaged-GUI, signing, checksum, scenario, clean-install, and GitHub-readiness gate in `RELEASE_CHECKLIST.md`.
- [ ] Tag, publish, verify, and announce `v1.1.0` only after every release gate is complete.

## Documentation and CI repair follow-up — 2026-08-27

- [x] **DOC-GSL-001** — align Docusaurus runtime and type packages, repair the
      documentation TypeScript errors and broken release-checklist link, and
      restore a passing production site build.
- [~] **CI-GSL-001** — the shell consumer validator, scaffold defaults, and
      conformance regression now agree on session-extension capabilities;
      cross-platform test assertions normalize path separators and line endings.
      Local shell tests pass; the next remote revision must confirm every runner.
- [~] **CI-GSL-002** — Windows-only Rust warnings are cfg-scoped so
      `RUSTFLAGS=-D warnings` does not reject imports, arguments, or helpers used
      only on Unix. Host validation passes; authoritative Windows validation
      requires the next remote revision.
- [ ] **RSP-GSL-004** — documentation `npm audit --package-lock-only` reports
      25 transitive advisories (19 high, 6 moderate) rooted in Docusaurus build
      dependencies (`image-size`, `serialize-javascript`, and `uuid`/`sockjs`).
      The lockfile is current and `npm audit fix` has no non-breaking repair;
      update when Docusaurus/webpack publish a compatible fixed chain.

## Chat reliability and CLI usage backlog — 2026-07-17

- [x] Keep the chat view pinned to the bottom while a new user input is typed
      and while new content is appended, so the most recent chat item remains
      visible instead of the scroll position jumping to the middle of the window.
      `ScrollArea` tracks following state, pauses after upward user scrolling,
      and resumes at the bottom; its focused regression covers the behavior.
- [x] Make chat persistence incremental and crash-resilient: store each user
      message as soon as Enter is submitted, and store assistant output as it is
      written to the chat window, so an abrupt Gosling exit does not erase the last
      chat item.
- [x] For CLI usage with subscription-backed providers where usage data is
      available, including Codex and Claude, `/status` now shows provider,
      model, mode, current-turn tokens, and accumulated session tokens. It
      reports unavailable usage honestly and does not estimate a remaining
      account allowance.

## Exhaustive defect-repair campaign — 2026-07-17

Audit checkpoint:
[`reports/2026-07-17-exhaustive-defect-audit-checkpoint.md`](../reports/2026-07-17-exhaustive-defect-audit-checkpoint.md).
Repair plan and evidence:
[`reports/2026-07-17-defect-campaign-plan.md`](../reports/2026-07-17-defect-campaign-plan.md)
and
[`reports/2026-07-17-defect-campaign-session-log.md`](../reports/2026-07-17-defect-campaign-session-log.md).

The synchronized audit froze 34 findings. The repair campaign fixed 33 and
left one explicitly dispositioned architectural residual; it also fixed one
post-freeze SDK request-shape defect found by verification. Only the audit
checkpoint is synchronized to the remote. All repair and closeout commits are
local until a separate push is authorized.

- [x] AUD-031: sessions.db schema v23 adds a durable tool-operation ledger with
      stable operation identities, explicit in-doubt recovery, terminal-result
      replay, and MCP operation-id propagation for servers that support external
      deduplication. Tool requests are checkpointed before dispatch and terminal
      responses are linked back to the ledger transactionally.
      Residual risk: Gosling cannot prove whether a non-idempotent external server
      committed an operation before a transport or process failure. Such operations
      remain visibly in doubt and require external verification; Gosling does not
      retry them automatically.
- [x] `crates/gosling/src/session/session_manager.rs` (9349 lines) modularized
      2026-08-22: the `impl SessionStorage` monolith is carved into 12
      `session_manager/*.rs` submodules by responsibility (schema,
      migrations, legacy import, pool lifecycle, tool operations, message
      storage, artifacts, library, summaries, session CRUD, listing,
      transfer); the facade (now ~1,460 lines of production code plus the
      untouched inline test module) keeps every public path unchanged.
      Behavior-preserving; no MOD-B suspects surfaced. Full run log:
      [`docs/logs/session/2026-08-22-modularize-session-manager.md`](logs/session/2026-08-22-modularize-session-manager.md).
- [x] `crates/gosling/src/agents/platform_extensions/summon.rs` (2772 lines)
      modularized 2026-08-23 into seven `summon/*.rs` responsibility modules
      for source discovery, task tracking, loading, delegation, delegate
      configuration, asynchronous delegation, and MCP dispatch. The original
      module remains a compatibility facade and preserves every public path.
      Behavior-preserving; no MOD-B suspects surfaced. Full run log:
      [`docs/logs/session/2026-08-23-modularize-summon.md`](logs/session/2026-08-23-modularize-summon.md).
- [x] `ui/desktop/src/main.ts` (3614 lines) modularized 2026-09-01 into
      eleven `main/*.ts` responsibility modules for menu localization,
      authorized Git IPC, backend certificate trust, allowlist retrieval,
      file/system/renderer/settings/application IPC, window chrome, and
      application-menu installation. The original executable remains the
      Forge compatibility facade at 1,861 lines. Behavior-preserving; BUG-001
      was recorded and routed without repair. Full run log:
      [`docs/logs/session/2026-09-01-modularize-desktop-main.md`](logs/session/2026-09-01-modularize-desktop-main.md).
- [x] `crates/gosling/src/acp/server.rs` (5136 lines) modularized 2026-09-01
      into responsibility modules for tests, extension selection, activation,
      initialization, message/tool projection, prompt runs, configuration, and
      transport. The original module remains a 655-line compatibility facade.
      Behavior-preserving; no MOD-B suspects surfaced. Full run log:
      [`docs/logs/session/2026-09-01-modularize-acp-server.md`](logs/session/2026-09-01-modularize-acp-server.md).
- [x] `crates/gosling/src/agents/agent.rs` (5521 lines) modularized 2026-09-01
      into responsibility modules for tests, hooks/steering, frontend and
      extension state, durable tool dispatch, reply preparation/streaming,
      provider transitions, and prompt APIs. The original module remains a
      532-line compatibility facade. The existing 1,124-line streaming
      turn-loop stays intact as one documented state-machine cohesion exception.
      Behavior-preserving; no MOD-B suspects surfaced. Full run log:
      [`docs/logs/session/2026-09-01-modularize-agent.md`](logs/session/2026-09-01-modularize-agent.md).
- [x] Modularize all routed >=2000-line files in dedicated changes, preserving
      behavior and avoiding mixed repair/refactor commits.
- [x] Run the added Rust regression suite, workspace build, and Clippy before
      merge when explicitly authorized. The 2026-07-18 twelve-lens follow-up ran
      the workspace build, serialized `gosling` library suite, related crate suites,
      and all-target Clippy successfully.

## Defect-repair campaign — 2026-07-16

Full inventory, skill disposition, and repair log:
[`reports/2026-07-16-defect-audit-and-repair.md`](../reports/2026-07-16-defect-audit-and-repair.md).
42 defects found across 12 audit lenses, grouped into locality-based repair
stages. 22 repaired under `repair-defect-campaign` gates (patch, regression
test, change review, commit per stage) across three passes; the remaining 20
were carried forward and repaired (13) or deferred with reasoning (5, plus
the 3 already-deferred from this pass) by the 2026-07-18 follow-up campaign
below. Track per-stage status in those reports rather than duplicating them
here.

Corroborates two previously-deferred, still-open findings from
`reports/2026-07-10-audit-skills-pack-report.md`: the `/status` static-200
health lie (there: FSR-SRV-001, here: OPS-001 — repaired in this pass) and
the hardcoded `exit_type="normal"` telemetry (there: FSR-SRV-002, here:
OPS-003 — repaired in the 2026-07-18 follow-up campaign below). Correction:
this session's sandbox cannot build `gosling-server` either (`cargo build -p
gosling-server` fails downloading `v8-goose`'s prebuilt V8 binary from a
blocked GitHub-releases host) — the underlying `gosling` crate change
(`SessionManager::healthy()`) is compiled and tested, but the
`gosling-server` route handlers themselves are
unverified by `cargo build`/`test`/`clippy` in this environment. Recommend
CI confirm both before merge.

## Audit and repair campaign — 2026-07-18

Full disposition, architecture-invariant compliance check, and repair log:
[`reports/2026-07-18-audit-repair-campaign.md`](../reports/2026-07-18-audit-repair-campaign.md).
Repaired 13 of the 2026-07-16 campaign's 20 open defects (ORCH-003, CON-003,
OPS-002, OPS-003, OPS-004, OPS-005, INV-001, INV-002, GUI-002, GUI-004,
GUI-005, SEC-003, CON-001); deferred 5 with stated reasoning (ORCH-002,
RES-002, RES-003, REC-001, REC-002), same sandbox build limitations as
above for `gosling-server` and `ui/desktop`.

## Twelve-lens audit and defect-repair campaign — 2026-07-18

Audit report and machine-readable inventory:
[`reports/2026-07-18-twelve-lens-agent-skills-audit.md`](../reports/2026-07-18-twelve-lens-agent-skills-audit.md)
and
[`reports/2026-07-18-twelve-lens-agent-skills-findings.json`](../reports/2026-07-18-twelve-lens-agent-skills-findings.json).
Repair plan and execution evidence:
[`reports/2026-07-18-twelve-lens-defect-campaign-plan.md`](../reports/2026-07-18-twelve-lens-defect-campaign-plan.md)
and
[`reports/2026-07-18-twelve-lens-defect-campaign-session-log.md`](../reports/2026-07-18-twelve-lens-defect-campaign-session-log.md).

The catalog-driven audit froze 10 findings. All 10 were repaired: plaintext
prompt secret profiles, renderer filesystem self-authorization, unenforced
workspace folder access, delegated-role capability inheritance, lossy JSONL
imports, imported transcript authority, unvalidated settings IPC, tear-prone
Desktop JSON writes, unbounded import payloads, and invalid-host startup panic.
The reports retain the full threat analysis, repair stages, regression proof,
and the one upstream Nostr allocation limitation. No campaign commit or remote
mutation was performed.

## Open-defect campaign reconciliation (2026-07-20)

- [x] Chat auto-follow remains enabled while the user is at the bottom and pauses after upward user scrolling.
- [x] Interrupted chat/tool operations are durably recorded and recovered without redispatching an in-doubt side effect.
- [x] ACP runtime config, data, state, identity, and request execution are scoped to the server instance rather than the process default.
- [x] Desktop browser-global lint debt and unstable workspace-filter hook dependencies are repaired.
- [x] Provider inventory startup reads are cached and concurrent reads are coalesced; mutations invalidate the cache.
- [x] The ACP schema check resolves repository paths from the Justfile location.
- [x] CLI usage reporting is implemented by `/status`; the command reports
      provider/model/mode plus current-turn and accumulated token usage when the
      provider supplies it, and states when usage is unavailable.
- [ ] Session Handoff remains a feature backlog item, not an open defect.
- [ ] Giles's internal uniqueness-constraint failure remains external tool debt.
- [ ] Release execution remains maintainer-owned.

### Provider inventory concurrency closure (2026-07-20)

- [x] Mutation epochs invalidate provider inventory at both mutation boundaries.
- [x] Reads superseded by an invalidation retry against the current generation.
- [x] Reads completing during a mutation cannot repopulate the shared cache.

### Open-defect campaign verification closure (2026-07-20)

- [x] Rust formatting, library tests, server tests, and workspace clippy are green.
- [x] Desktop typecheck, 547 tests, ESLint, and i18n validation are green.
- [x] ACP schema generation and generated TypeScript consistency are green.
- [x] Credential selector, chat scrolling, parent supervision, Claude permissions, and container cleanup regression cards pass.

## Six-lens audit and repair campaign — 2026-08-12

Full inventory and repair evidence:
[`reports/2026-08-12-six-lens-agent-skills-audit-repair.md`](../reports/2026-08-12-six-lens-agent-skills-audit-repair.md).

- [x] On explicit CLI turn cancellation, close undispatched sibling tool requests with terminal,
      idempotent responses while preserving ledger-only reconnect recovery and the existing in-doubt
      boundary after dispatch.
- [x] Bound diagnostics disk reads as well as returned content, report real truncation, and
      create full diagnostic bundles owner-only with an explicit sharing warning.
- [x] Serialize `projects.json` read-modify-write operations and atomically replace private
      tracker state.
- [x] Coordinate shared memory JSONL readers and batch writers with file locks and durable
      flushes.
