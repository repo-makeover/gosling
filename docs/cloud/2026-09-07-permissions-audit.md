# Permissions audit suite — 2026-09-07

Status: scoped audit and authorized tests completed, with open findings. Existing
tests pass; isolated probes reproduce defects. Findings were recorded before tests
and reconciled with their results. This is a permission-focused audit of all 13
base lenses, not a clean bill for the entire repository or installed application.

## Verdict

The screenshot's malformed `/dev/null launchctl ...` path comes from shell comment
parsing. Commit `0105cd449` contains the narrow repair. Autonomous mode still
intentionally preserves workspace and egress safety prompts; ordinary tool grants
do not suppress those separate checks.

Seven additional findings remain: three High, three Medium, one Low. Four relate
directly to repeated prompts or misleading approval state. None of these broader
production repairs was applied in this audit.

| ID | Severity | Finding | Evidence after testing |
| --- | --- | --- | --- |
| SEC-GSL-901 | High | Simplified shell grammar misclassifies executable commands and drops heredoc segments | Inspector defect reproduced in isolated tests |
| SEC-GSL-902 | High | Batch-wide egress deduplication suppresses checks for later calls | Inspector defect reproduced in isolated tests |
| CON-GSL-901 | High | Independent permission managers overwrite grants/revocations and retain stale decisions | Stale handles and concurrent thread writers reproduce loss |
| WFG-GSL-901 | Medium | Single-tool/domain save failure is swallowed after approval; UI says always allowed | Rollback/normal return reproduced; UI consequence source-traced |
| WFG-GSL-902 | Medium | Desktop approval cache confuses identical request IDs across sessions | React component failure reproduced |
| INV-GSL-901 | Medium | Legacy Claude Code converts Always Allow to Allow Once without storing it | Source-confirmed |
| CMP-GSL-901 | Low | Closed permission-locking TODO cites a commit that only fixed config.yaml | Source and Git history confirmed |

## Scope and authority

- Target: Gosling `main` at `0105cd449`; initially clean. No production code
  changed during this audit. Report and isolated test artifacts are the only writes.
- User requested another full suite of audits, followed by tests, and explicitly
  prohibited building the app yet. No application/release build, packaging,
  installation, restart, live provider request, or host-management command replay.
  Cargo compilation needed by a test invocation is test work.
- Scope: workspace paths, shell parsing, tool classes and annotations, inspector
  aggregation, egress, stored grants, approval dispatch, provider bridges,
  Desktop approval state and CLI/headless handling.
- Method: catalog base suite (13 lenses), shared evidence/severity/confidence
  contracts, operator-signal deepening, repository-state reconciliation.
  All 28 returned standalone candidates received applicability triage.
  One reviewer; no independent-consensus claim.
- Budget: up to eight boundary functions per base lens, reusing shared reads.
  Rust source and React component/store/service interpretation; generated/vendor
  code excluded from size/ownership claims. ACP schemas are protocol seams.
- Repository orientation: AGENTS, README, docs index and relevant architecture/
  ADR-0017, Giles advisory metadata and recent permission/session logs were read.
  GEMINI.md was absent. Giles metadata remains advisory.

## Inventory and boundary map

| Owner/layer | Producer → contract → consumer | State and failure boundary |
| --- | --- | --- |
| `permission/working_dir_scope_inspector.rs` (policy) | Session folders + tool arguments → InspectionResult → manager | Canonical paths; workspace writes and restricted reads; SEC-901 |
| `permission/permission_inspector.rs` (policy) | Mode + explicit grant + annotations → allow/deny/ask | User deny dominates; server metadata can tighten, not self-grant |
| `tool_inspection.rs` (coordination) | Inspector verdicts/errors → PermissionCheckResult → dispatch | Deny dominates; hard prompts survive Auto; persistence error erased |
| `security/egress_inspector.rs` (policy) | Per-call text → destinations/direction → domain decision | Loopback/saved grants clear destination; batch dedupe is SEC-902 |
| `config/permission.rs` (persistence owner) | Tool/domain/provider decisions → permission.yaml | Local lock, atomic rename, rollback; no interprocess refresh/transaction |
| `agents/tool_execution.rs` (execution) | Confirmation → dispatch → optional persistent grant | Tool may already execute when persistence fails |
| `agents/tool_confirmation_router.rs` (routing) | Request ID → one-shot waiter | Per-Agent map, unknown/stale delivery rejected |
| `acp/provider.rs` (external provider adapter) | ACP options + mode + provider/tool key → provider answer | Saves reusable grant before forwarding answer; propagates write error |
| `providers/claude_code.rs` (legacy adapter) | can_use_tool → approval card → control_response | Auto handled separately; persistent option flattened to one response |
| `acp/server/tool_events.rs` (transport) | Hosted action-required → ACP option set | Security prompts withhold tool-wide Always Allow |
| Desktop `permissionRequests.ts` (store) | session ID + tool-call ID → pending request → response | Correct composite key; response resolves locally, no save acknowledgement |
| Desktop `ToolApprovalButtons.tsx` (view) | Operator answer → local resolution → rendered status | Request-only cache; optimistic “Always allowed”; bulk save has error path |
| CLI `session/mod.rs` (view/lifecycle) | Prompt or headless refusal → confirmation | Noninteractive DenyOnce; cancellation persists cancelled response |

Permission lifecycle: requested → inspected → denied / pending / approved →
confirmation → dispatch → completion. Persistent grant is an additional transition:
the hosted-tool path performs it after dispatch, the ACP provider before forwarding,
and legacy Claude Code omits it. UI resolution currently acknowledges sending the
answer, not successful durable storage.

Mode/role scope: hosted Auto may downgrade advisory inspections, while workspace,
egress, and inspection-error prompts survive; Chat skips tool execution; interactive
Approve asks; SmartApprove uses user grants before read-only judgement. Provider Auto
has its own native mode mapping. Delegated subagents cannot surface inspector prompts
to a human and are denied through `redirect_unapprovable_subagent_requests`.
Provider-native shell/sandbox configuration is distinct from Gosling's hosted-tool
inspection and was not asserted equivalent.

## Detailed findings

### SEC-GSL-901 — shell grammar cannot reliably enforce the advertised path boundary

**High; Confirmed; source-evidenced and test-reproduced.** Domains: security, input/output, negative
space, state transition. This is a defect in the inspector, not a claim of a
successful attack on the installed app.

Evidence:
- `crates/gosling/src/permission/working_dir_scope_inspector.rs:479`:
  `shell_words::split(segment).unwrap_or_default()` silently discards a segment
  the simplified splitter makes unparsable.
- Same file:407 splits `&&` but not background `&`, and has no heredoc state.
- Same file:689 includes `"env"` in the unconditional read-only executable list.
- Same file:62 checks read-only workspace denial only when
  `is_mutating_tool_call(tool_call)` is true.
- `crates/gosling/src/agents/platform_extensions/developer/shell.rs:700` invokes
  the actual shell with the original command, so execution uses richer grammar.

Observed/expected: `env touch /outside/file` and `true & touch /outside/file`
are executable mutations but classify as read-only. An apostrophe in literal
heredoc data causes the following real redirect to be dropped in the probe.
The adjacent benign-comment probe passed, rejecting an initial suspicion of a
remaining comment false positive in read-only workspaces. Explicit writes must be checked;
literal comments/heredoc data must not invent filesystem effects.

Mechanism/break-it: separate shell tokenizers disagree about executable words,
quotes and command boundaries. Test env wrappers, background separators, quoted
heredoc data, and a real redirect immediately following the heredoc. Compare
benign diagnostic control and structured out-of-scope write.

Impact: a model-generated shell call with an existing tool grant can evade a
workspace inspection even after the narrow comment repair. Blast radius: host
filesystem accessible to that tool; side effect: FS; reversibility depends on the
executed command; visibility silent for misses and UI-visible for false prompts;
rerun safety unknown for arbitrary shell.

Mitigation: use one shell-aware syntax representation for supported syntax, and
explicitly handle unknown constructs; unwrap executable wrappers before classifying
them. Preserve narrow device-stream exemptions and independently check mutations.
Do not solve false prompts by broadly disabling scope checks.
Guardrail/test: read-only roots reject env/background writes; later heredoc writes
remain flagged; comments and /dev/null-only diagnostics stay allowed.
Complexity: workflow_protocol; cost M; drivers shell syntax/platform coverage and
negative tests; nominal agent codex with review. Non-goal: claim string inspection
is an OS sandbox. Adjacent: awk system(), sed w, shell substitutions require the
same grammar/authority review; not separate counted findings.

### SEC-GSL-902 — checking one destination suppresses later calls to it

**High; Confirmed; source-evidenced and test-reproduced.** Domains: security, integrity, cascade.

Evidence:
- `crates/gosling/src/security/egress_inspector.rs:427` creates
  `seen_destinations` outside the tool-request loop.
- Same file:448 filters each request with
  `seen_destinations.insert(d.destination.clone())`; :451 skips an empty list.
- Direction and the network-client check happen only after that filtering (:455,
  :470), while the resulting verdict is keyed to one request (:527).

Observed/expected: a GET to a URL followed by a POST to the same URL in one batch
leaves the POST without an egress verdict. Even a literal URL in a non-network
command can consume the destination first. Every tool call must receive its own
direction/approval decision.

Mechanism/break-it: batch display deduplication is applied to authorization inputs.
Use distinct request IDs and identical URL, first download/literal then upload;
assert both requests are independently inspected.

Impact: outbound request can lose the extra approval gate when other inspectors
allow the tool. Blast radius workflow/network destination; side effect network;
data disclosure may be irreversible; visibility silent; rerun safety unsafe for
arbitrary outbound requests. No network request is needed to demonstrate the
inspection defect.

Mitigation: deduplicate destinations within each call; deduplicate logging separately
if needed. Guardrail: two calls to one URL, opposite directions, plus two uploads,
must each have appropriate results. Complexity local_guardrail; cost S; drivers
batch/direction fixtures; nominal agent codex. Non-goal: remove egress approval or
claim URL regexes provide network isolation.

### CON-GSL-901 — permission persistence is atomic but neither coordinated nor fresh across processes

**High; Confirmed; source-evidenced and test-reproduced.**
Domains: concurrency, temporal, integrity. Related old identifier: CON-GSL-002.

Evidence:
- `crates/gosling/src/config/permission.rs:50` reads the file only at construction;
  :86 shares a manager through a process-local `Weak` map.
- :114 locks `persist_lock: Mutex<()>`, mutates the cached map, then serializes
  the whole map. No reload or filesystem lock occurs.
- `crates/gosling/src/config/base.rs:81` writes a unique temporary file and
  renames it. This helper contains no cross-process lock.
- Permission getters at :170/:238 use the cached map; `remove_extension` at :322
  also persists the cached full map.

Observed/expected: two independently opened managers can both report successful
updates while the second overwrites the first's grant or denial. An already-open
manager retains a grant after another writer revokes it. Independent writers must
merge under a shared transaction; checks must observe revocation according to an
explicit freshness contract.

Break-it: open A and B over a fresh temp directory, write denial through A, unrelated
grant through B, reopen C and inspect both principals. Separately seed Allow,
open a stale reader, revoke via another manager, inspect stale reader. These
deterministic handle probes model independent-process state. A separate real-thread
probe uses a barrier after opening both managers and before concurrent writes;
both writes return success but both decisions do not survive. No separate-process
scheduler or crash/power-loss stress claim is made.

Impact: approval repeats or a denial disappears; affected scope is all sessions
sharing that configuration. FS/policy side effect; restoring decisions compensates
policy state but cannot undo tools already run; visibility silent; repeating a
stale write is unsafe. Normal CLI plus Desktop is a real second-writer surface.

Mitigation: lock the complete file read/modify/write across processes, reload inside
that lock, and define invalidation/read freshness for decisions. A lock around
rename alone is insufficient. Regression: independent handles and real subprocess
writers preserve unrelated grants and observe revocation. Complexity
cross_process_coordination; cost M; drivers file protocol, failure/refresh tests;
nominal agent codex with review. Non-goal: reuse config.yaml's lock without proving
ownership and lock order.

### WFG-GSL-901 — failed persistent approvals are displayed as durable success

**Medium; Confirmed; source-evidenced.** Domains: workflow, reliability, operator
signal, state transition.

Evidence:
- `crates/gosling/src/agents/tool_execution.rs:185` dispatches an approved tool
  before the AlwaysAllow persistence call at :202.
- `crates/gosling/src/tool_inspection.rs:195` and :222 return unit and only log
  failed writes. Log claims “permission decision applied for this session”.
- `crates/gosling/src/config/permission.rs:158`: `*map = previous`; the grant is
  rolled back in memory too.
- `ui/desktop/src/components/ToolApprovalButtons.tsx:148` calls
  `setResolvedDecision(action)` after local request resolution; :199 renders the
  decision as “Always allowed”. No durable acknowledgement is checked.

Observed/expected: storage failure can leave the tool executed once and no saved
or in-memory grant, while the UI claims always allowed. The next call asks again.
The UI should distinguish “allowed once; saving failed” and offer retry of the
permission save without rerunning the tool.

Failure analysis: normal_run; cause erased persistence result; local effect
rollback; workflow effect repeat prompt; end effect misleading operator belief.
Detection log, log-only visibility, immediate log emission but user detection
unknown; audience log reader (no delivery-to-human claim). Compensation currently
rollback of policy only. Expected safe state fail_visible; resilience withstand /
understand. Log has tool/domain, level and root cause but reports incorrect current
state and supplies no safe next action. Actual logged vs required obvious
durability feedback: one-step gap plus false-success modifier.

Mitigation: propagate typed save outcome through approval completion; preserve the
already-executed result, surface save failure and a save-only retry. Correct the
log. Guardrail: injected rename failure must not render durable success and must
not re-execute tool on retry. Complexity workflow_protocol/operator_ux; cost M;
drivers backend/ACP/UI completion contract; nominal agent codex with review.
Non-goal: weaken denial or silently assume a failed disk write is remembered.
Adjacent: domain approval has the same swallow; bulk extension save already has
an explicit error branch and is a held control.

### WFG-GSL-902 — Desktop approval state is keyed too broadly

**Medium; Confirmed; source-evidenced and test-reproduced in React/jsdom.**

Evidence:
- `ui/desktop/src/components/ToolApprovalButtons.tsx:118` reads
  `globalApprovalState.get(id)`; :102 stores by `id`; effects also omit session.
- :199 suppresses the buttons when the remembered decision is clicked.
- `ui/desktop/src/acp/permissionRequests.ts:129` keys live requests with
  `sessionId\u0000toolCallId`.

Observed/expected: after resolving ID X in session A, rendering a pending ID X in
session B can show A's status and hide B's buttons. Live request identity and
display identity must agree.

Break-it: approve one fixture, unmount, mount a second session with the same
request ID; its live approval must be actionable. Also test in-place session switch.
Impact: workflow stall/misleading history, not evidence that B's tool executed;
UI side effect, reversible by clearing state/reloading, user sees inconsistent
status, rerun requires care. Cache cap 500 bounds memory but does not provide
scope isolation.

Mitigation: shared composite identity plus request-generation handling where IDs
can be reused; reset local state when identity changes. Regression must assert
actual rendered buttons and correct session resolution. Complexity local_guardrail;
cost S; drivers React lifecycle fixtures; nominal agent codex. Non-goal: change
provider ID formats or approve the second request automatically.

### INV-GSL-901 — legacy Claude Code ignores the persistence meaning of Always Allow

**Medium; Confirmed; source-evidenced.** Domains: invariant, architecture, workflow.

Evidence:
- `crates/gosling/src/providers/claude_code.rs:1337` emits an ordinary approval
  with no security prompt, so ordinary Always Allow is offered.
- :1349 matches `Permission::AlwaysAllow | Permission::AllowOnce` into the same
  `PermissionResponse::Allow`, then writes one control_response at :1365.
- No grant lookup/update occurs in that control-request branch.
- Contrast `crates/gosling/src/acp/provider.rs:734` lookup and :789 durable
  provider/tool permission update. `agents/agent/reply_entry.rs:57` routes provider
  confirmations directly to the provider, returning before the hosted-tool handler.

Observed/expected: in approval modes, the next legacy Claude tool request asks
again after “Always Allow”; newer ACP persists it. A persistent choice must be
honored by the adapter or not offered.

Impact: repeated interruption for legacy Claude users; workflow/policy side
effect; reversible; UI-visible repetition; no claim this caused the supplied
Gpt/mac-control screenshot. Rerun of the provider tool is not necessarily safe.
Mitigation: adopt the existing provider-and-tool scoped persistence contract;
surface save errors. Regression: two sequential fake-provider requests plus
provider recreation, allowing the first persistently causes no second prompt.
Complexity workflow_protocol; cost M; drivers adapter fixture and scoped identity;
nominal agent codex. Non-goal: broaden one tool grant to an entire provider.

### CMP-GSL-901 — completion ledger overstates the permission-lock repair

**Low; Confirmed; source/Git-evidenced.** Domain: compliance/posture and state
reconciliation; no external compliance determination is made.

Evidence:
- `docs/TODO.md:42` checks off “permission.yaml needs the same cross-process
  flock as config.yaml”; :54 says it closed in `37804170e`.
- `git show --stat 37804170e` changes only
  `crates/gosling/src/config/base.rs`; commit text names four Config methods.
- Current permission writer remains as CON-GSL-901 describes.

Observed/expected: a status record intended for permission.yaml cites evidence
about a different store. Preserve the valid config.yaml completion and reopen or
correct the permission-specific claim. This explains why previous completion
records were insufficient evidence that the approval problem was solved.

Impact documentation/governance; reversibility reversible; visibility misleading
until source reconciliation; rerun safe. Mitigation: link the reopened permission
item to this report and later to an actual independent-writer regression.
Complexity local_guardrail; cost XS; drivers ledger consistency; nominal agent
codex. Non-goal: rewrite historical logs or retract the actual Config fix.

## Held controls and negative results

- Comment repair preserves literal hashes and escaped continuations while consuming
  real comments (working_dir_scope_inspector.rs:407); all 32 scope tests pass.
- Canonical path checks handle symlink escape, missing children and narrower
  read-only roots; ambiguous canonicalization errors propagate to inspection
  fallback. This does not excuse the shell grammar gaps.
- Inspection errors synthesize a security approval for each request
  (tool_inspection.rs:141); Auto does not erase this fallback.
- Explicit denials dominate aggregated allows (permission_inspector.rs:45 and
  tool_inspection.rs aggregation); server readOnlyHint cannot grant authority.
- ACP provider grants are scoped by provider plus its reusable title key and
  saved before forwarding. Requests offering only one-time/domain options are
  excluded from reusable tool grants. Title stability and external security-request
  option combinations are protocol frontiers; legacy Claude differs as INV-GSL-901.
- CLI headless approval denies once and terminates with a reason
  (session/mod.rs:1211); it does not silently run or persist a permanent denial.
- Desktop live request map scopes by session; unknown requests return false;
  bulk extension save failure leaves the approval pending.
- Atomic file rename prevents mixed-byte publication; on persistence error the
  permission map rolls back. These are distinct from cross-process lost updates.
- Router registration/delivery is mutex protected; dropped receivers are pruned
  on registration; unknown or late delivery returns false.
- ToolPermissionStore is exported but a repository-wide symbol/call search found
  no live consumer beyond its own implementation and re-export. Its expiry/file
  code is not evidence of behavior in today's permission path.

## Frontier and tests to run

Static-only limits: no installed-app replay, paid/live provider round trip, real
cross-process scheduler stress, OS crash/power-loss drill, Windows shell test or
Electron package test. Cached UI/permission defects are not assertions that the
user experienced every defect. Confirmation duplicate-ID/cancellation races,
filesystem TOCTOU after inspection, large-input/resource profiling, and complete
vendor mode/version compatibility remain follow-ups rather than confirmed findings.

Test plan (after this source checkpoint): existing shell regression suite; relevant
Rust permission/inspection/egress/ACP/Claude tests; isolated probes for stale
writers, rollback, shell grammar and egress batch decisions; Desktop approval and
pending-request tests plus a cross-session UI probe. Keep test-probe sources and
output alongside this report; never execute the shell strings in the probes.

## Base-lens coverage ledger

All 205 taxonomy codes below receive one disposition. “Held” is limited to the
named traced boundary; “Not reviewed” is an explicit frontier, never a passing
result. Full suite means all 13 lenses applied to this permission scope, not all
possible repository or runtime specializations completed.

| Lens codes | Disposition | Evidence / limit |
| --- | --- | --- |
| ARC-001, ARC-002, ARC-009, ARC-014, ARC-016, ARC-017, ARC-019, ARC-020, ARC-021 | Held | Ownership inventory above: policy inspectors, PermissionManager, typed ACP transport and UI service calls; isolated core fixtures construct these without live providers. |
| ARC-003 | Finding | WFG-GSL-902: module-global approval state has weaker identity than its consumer. |
| ARC-004, ARC-007, ARC-008, ARC-025 | Finding | INV-GSL-901: hosted, ACP and legacy provider implementations disagree about persistent answers. |
| ARC-006 | Finding | WFG-GSL-901: result-erasing coordination abstraction prevents callers from knowing save outcome. |
| ARC-010 | Held (sampled) | Workspace policy is enforced in WorkingDirScopeInspector; UI mirrors the option restriction. Forged ACP option responses remain frontier. |
| ARC-011, ARC-013 | N/A in scope | No passive collector or declared-frozen internal contract is part of the permission workflow. |
| ARC-012 | N/A in scope | No optional integration load path is under audit; provider unavailability is a separately listed frontier. |
| ARC-015, ARC-022 | Held (sampled) | ToolPermissionStore has no live consumer in the repository-wide symbol search; do not count it as a second active persistence stack. |
| ARC-005, ARC-018, ARC-024 | Not reviewed | No full transitive import/config census or regenerated-schema comparison; scoped boundary reads cannot prove these globally. |
| ARC-023 | Held (sampled) | No production reflection/prototype mutation observed in the traced permission modules; test mocks are intentional. |
| CAS-001, CAS-004, CAS-005, CAS-009, CAS-013 | Finding | CON-GSL-901 / WFG-GSL-902: stale authority or display crosses into later requests. |
| CAS-006, CAS-007 | Finding | SEC-GSL-901/902: inspection omission expands shell/network authority beyond the intended gate. |
| CAS-008, CAS-015 | Finding | WFG-GSL-901: failure loses its result and logging misstates in-memory success. |
| CAS-002, CAS-003 | Held | Permission update and confirmation code has no automatic grant retry loop; repeated operator prompts are not classified as retry amplification. |
| CAS-010, CAS-014 | Held (single writer) | PermissionManager clones and restores the prior map on save failure; batch updates publish one serialized map. |
| CAS-011 | Held | ToolInspectionManager distinguishes advisory Auto downgrade from explicit workspace/egress/failure prompts. |
| CAS-012 | Not reviewed | No provider outage or optional-extension removal drill; source-only path cannot establish cascade magnitude. |
| CMP-003, CMP-007, CMP-011, CMP-012, CMP-015 | Finding | CMP-GSL-901: permission.yaml completion claim exceeds the cited config.yaml commit evidence. |
| CMP-001, CMP-002, CMP-004, CMP-005, CMP-006, CMP-008, CMP-009, CMP-010, CMP-013, CMP-014 | N/A in scope | No external compliance framework, certification, collector, multi-format compliance report or compliance release gate is being assessed. AGENTS makes Giles advisory. |
| CON-001, CON-002, CON-006, CON-007, CON-010, CON-012 | Finding | CON-GSL-901: stale independent maps, per-instance mutex and full-file overwrite. |
| CON-008, CON-009 | Finding | WFG-GSL-901: execution and persistent decision completion are separate, incorrectly reported transitions. |
| CON-017 | Finding | WFG-GSL-902: approval state survives remount into another session's request. |
| CON-003, CON-005 | Held (sampled) | Live pending-map removal and one-shot send prevent a second resolution; no automatic permission-save retry occurs. |
| CON-011 | Held (sampled) | Permission writes take persist_lock then map write lock; inspected getters do not acquire the locks in reverse order. |
| CON-013 | Held | write_file_atomic uses a unique temporary file, file sync and rename; this is atomic publication, not cross-process merge. |
| CON-014 | Held (same path/process) | for_config_dir reuses a live manager under the registry mutex; canonical path aliases and independent new() handles are not covered by that guarantee. |
| CON-004, CON-015, CON-016, CON-018 | Not reviewed | Duplicate operation replay, approval/checkpoint races, stale async bulk selection and event reentrancy need scheduler/transport probes. |
| DAT-001, DAT-004, DAT-010 | Finding | WFG-GSL-902: missing session provenance gives a stale approval status. |
| DAT-005 | Finding | CON-GSL-901: last snapshot replaces other writers' policy data. |
| DAT-006, DAT-013, DAT-015 | Finding | SEC-GSL-901: simplified parse silently promotes incomplete path knowledge into a gate decision. |
| DAT-007 | Finding | WFG-GSL-901: successful execution is reported with failed durable grant. |
| DAT-011 | Finding | CMP-GSL-901: evidence about one file is counted for a different file. |
| DAT-014 | Finding | SEC-GSL-902: earlier request destination state suppresses another request's inspection. |
| DAT-002, DAT-003, DAT-009 | Held (sampled) | Permission updates remove a principal from all levels before adding one; stale pending entries are removed; YAML/provider grant round-trip tests exist. |
| DAT-008, DAT-012 | N/A in scope | No permission schema migration or advisory compliance data export is part of the traced changes. |
| IOP-001, IOP-006, IOP-008 | Finding | SEC-GSL-901: malformed simplified segments are discarded while the real shell still receives executable input. |
| IOP-002, IOP-003 | Held (structured paths) | canonicalize_potential_path resolves existing ancestors and rejects dangling symlinks; structured path traversal/symlink tests pass. Shell gaps remain SEC-GSL-901. |
| IOP-010 | Finding | WFG-GSL-902: cached output from a previous request identity is reused. |
| IOP-011 | Finding | CON-GSL-901: atomic whole-file overwrite loses concurrent principals. |
| IOP-012 | Finding | WFG-GSL-901: durable outcome is inferred from local approval submission. |
| IOP-014 | Finding | CON-GSL-901: another process changes the policy file without reader invalidation. |
| IOP-015 | Finding | INV-GSL-901: the legacy provider consumes the same persistent answer differently. |
| IOP-004, IOP-005, IOP-007 | N/A in scope | No archive extraction, file-format conversion or spreadsheet export occurs in these permission boundaries. |
| IOP-009, IOP-013 | Not reviewed | End-to-end log redaction and worst-case command/regex input resource bounds were not established. |
| INV-001, INV-002, INV-004, INV-005, INV-011, INV-014 | Held (sampled) | Permission enums and ACP mappings explicitly distinguish one-time, tool-wide and domain choices; narrower option subsets are intentional. Dead ToolPermissionStore is not treated as authoritative. |
| INV-007 | Finding | New probes show baseline tests lacked cross-writer, cross-session and multi-call-destination invariants. |
| INV-008, INV-010, INV-012 | Finding | INV-GSL-901: AlwaysAllow is handled without its required persistent effect. |
| INV-009 | Finding | WFG-GSL-902: displayed and live request identities must match but do not. |
| INV-006, INV-013 | N/A in scope | No permission import/export or migration change is in this slice. |
| INV-003, INV-015 | Not reviewed | Generated schema round-trip and provider title/tool identity compatibility need separate protocol work; no unsupported equivalence claim. |
| NEG-001, NEG-002, NEG-003, NEG-005, NEG-013 | Finding | CON-GSL-901: single-process freshness assumption fails with independently opened managers. |
| NEG-004 | Finding | SEC-GSL-902: two individually inspected network commands compose into an uninspected upload. |
| NEG-007, NEG-009, NEG-011 | Finding | SEC-GSL-901/902: the gate can miss filesystem or outbound effects; actual irreversible effects were not executed. |
| NEG-008 | Finding | New failing probes exercise gaps absent from existing regression coverage. |
| NEG-010 | Held (sampled) | UI labels broad grants by extension/domain, puts Allow Once first, and surfaces stale/bulk-failure errors; it does not silently submit a preselected answer. |
| NEG-012 | N/A in scope | No finding relies on a hypothetical future integration; CLI and Desktop already exist. |
| NEG-015 | Held (single writer) | Permission rollback restores previous policy on write failure; no mutation rerun is automatically triggered by that rollback. |
| NEG-006, NEG-014 | Not reviewed | Filesystem/transport timing windows and actual reliance on stale governance claims as a release gate were not exercised. |
| REL-001 | Held | Corrupted permission YAML raises an explicit startup error/panic instead of loading permissive empty state; tested corruption path. |
| REL-002, REL-003, REL-009, REL-010, REL-011, REL-015 | Finding | WFG-GSL-901: failure is logged but reported to the operator as successful persistent approval. |
| REL-004 | Held | No unbounded retry loop occurs in the traced grant save/answer paths. |
| REL-006 | Held with frontier | Interactive human approval intentionally waits; headless CLI refuses. Disconnected-client cancellation timing remains unverified. |
| REL-008 | Finding | CON-GSL-901: stale policy state can overwrite newer decisions. |
| REL-014 | Held (scope only) | Workspace/restriction flags and tool-class rules are explicit; no production defaults were widened to address the screenshot. |
| REL-005, REL-007, REL-012, REL-013 | Not reviewed | No resource stress, power-loss filesystem durability or complete failure-tempfile cleanup drill; atomic rename is only a narrower held property. |
| SEC-002, SEC-004, SEC-008, SEC-012 | Finding | SEC-GSL-901: env/background/heredoc grammar gaps bypass workspace inspection when shell authority exists. |
| SEC-005 | Finding | SEC-GSL-902: deduplication of observations is incorrectly used as authority for later requests. |
| SEC-011 | Finding | CON-GSL-901: a previously granted permission can remain effective after revocation elsewhere. |
| SEC-003 | Held (structured workspace scope) | Existing sibling-session and canonical path tests exercise session-private folder isolation. This is not a full HTTP IDOR audit. |
| SEC-006 | N/A in scope | Executing an explicitly requested shell command is the tool's purpose; grammar enforcement gaps are reported under SEC-GSL-901 rather than relabeled generic command injection. |
| SEC-009, SEC-014 | N/A in scope | Deployment defaults and reverse-proxy configuration are outside this local permission pipeline. |
| SEC-001, SEC-007, SEC-010, SEC-013, SEC-015 | Not reviewed | Server authentication/sensitive routes, complete log/env secret handling, and backend validation of forged ACP option IDs need dedicated traces. |
| STT-001, STT-005, STT-006, STT-012 | Finding | WFG-GSL-902: pending request can be displayed as resolved using a different session's state. |
| STT-002, STT-008 | Finding | SEC-GSL-901/902: a mutation/upload can pass without its required inspection verdict. |
| STT-003, STT-011 | Finding | WFG-GSL-901: approval execution and durable grant disagree. |
| STT-004 | Held | Hosted tool action-required and ACP Pending status are explicitly emitted before the interactive wait. |
| STT-007, STT-010 | Held (live map) | Pending request lookup rejects unknown/stale IDs and deletes the live entry before resolving once. |
| STT-009 | Held (sampled) | UI permission mutations go through the permission service; PermissionManager owns the file update. |
| TMP-001, TMP-003, TMP-010 | Finding | CON-GSL-901: cached decisions do not observe external revocation. |
| TMP-002 | Finding | WFG-GSL-902: remembered approval survives into another session. |
| TMP-005 | Finding | WFG-GSL-901: optimistic UI resolution precedes known durable outcome. |
| TMP-008 | Finding | INV-GSL-901: a lifetime grant is reduced to one provider response. |
| TMP-009 | Finding | CMP-GSL-901: old evidence for config.yaml is presented as current permission.yaml completion. |
| TMP-006, TMP-011, TMP-012 | N/A in scope | No permission migration, delayed-job authority or standards draft is under assessment. |
| TMP-013 | Held | Active permissions are explicit levels without clock-based expiration; obsolete ToolPermissionStore expiry logic has no live consumer. |
| TMP-015 | Held (runtime scope) | No supported runtime consumer uses the alternate legacy permission store; cleanup is a maintenance recommendation, not a live grant mechanism. |
| TMP-004, TMP-007, TMP-014 | Not reviewed | Checkpoint replay, post-inspection filesystem TOCTOU and long-lived registry cleanup require deeper lifecycle/drill coverage. |
| WFG-001, WFG-002, WFG-005, WFG-008, WFG-011, WFG-013 | Finding | WFG-GSL-901: persistent success label cannot know that save rolled back. |
| WFG-003 | Finding | INV-GSL-901: legacy provider and hosted/ACP persistence semantics differ for the same UI action. |
| WFG-004 | Finding | WFG-GSL-902: cross-session stale approval display. |
| WFG-007, WFG-014 | Finding | SEC-GSL-901/902: incomplete derived inspection knowledge is treated as sufficient authority. |
| WFG-006 | Held (source/UI tests) | The buttons explicitly distinguish one-time, domain and all-extension choices, and failures have an alert region. |
| WFG-009, WFG-015 | Held (sampled) | Bulk handler posts its enumerated tool names in one permission update and shows persistence failure; it does not report partial saves as complete. |
| WFG-012 | Held (hosted path) | Approval-required requests flow through a pending confirmation before dispatch; the separate parsing omissions are already findings. |
| WFG-010 | Not reviewed | A forged or unsupported ACP response option was not driven through every backend/provider transport. |

## Standalone skill applicability

Loaded for triage does not mean an entire specialized audit was completed. The
base suite covers the requested permission workflow; the following explicit
limits prevent that from being mistaken for 41 exhaustive audits.

| Catalog skill | Disposition | Evidence / scope reason |
| --- | --- | --- |
| audit-architecture-drift | Deferred specialization | Base ARC/INV/CMP compared permission contracts and the closed ledger. Whole declared-vs-actual architecture map is outside the bounded permission run. |
| audit-security-code | Deferred specialization | Base SEC/IOP cover the traced permission sinks. No independent repository-wide taint/supply-chain pass is claimed. |
| audit-security-owasp | Not applicable | No web-app/OWASP assessment was requested; this local agent approval flow is not an ASVS verification engagement. |
| audit-contract-crossrepo | Not applicable | Both Gosling producer and consumer are in this repo. External vendor protocol compatibility is a provider frontier, not two owned repository checkouts. |
| audit-deadcode-cleanup | Deferred specialization | ToolPermissionStore has no observed runtime caller; complete public-API/export reachability and removal planning were not requested or performed. |
| audit-pipeline-externalapi | Deferred specialization | ACP/Claude permission source branches were traced. Vendor runtime/version/timeout compatibility needs fake-provider sequences or a separately authorized live drill. |
| audit-dependency-criticality | Deferred specialization | Failure propagation was covered under CAS/REL; no dependency removal or outage criticality census. |
| audit-failsafe-readiness | Deferred specialization | REL/CON/STT plus operator-signal covered known save/wait failures. Power-loss, process death and overload drills were not run. |
| audit-contract-internalapi | Deferred specialization | Typed permission enums/options and result loss were traced under INV/ARC. Full validator/schema-generation equivalence remains frontier. |
| audit-security-llm | Deferred specialization | Authority checks on model tool calls and MCP annotations were reviewed under SEC/NEG. No adversarial prompt corpus or broad LLM data-leakage campaign. |
| audit-architecture-nodejs | Not applicable to selected boundary | Policy/persistence is Rust and approval view/store is React. Electron main-process Node architecture is outside this scope. |
| audit-security-nodejs | Not applicable to selected boundary | No Node server routes, Node sandbox or dependency exploit surface is included in this permission audit. |
| audit-optimization-opportunities | Deferred specialization | Repeated approval work is explained by correctness defects; no throughput/cost optimization study beyond these findings. |
| audit-performance-profile | Deferred specialization | No performance claim, baseline benchmark or live profiling; latency/resource magnitude is unverified. |
| audit-memory-lifecycle | Deferred specialization | UI cache cap and pending-map removal were inspected. Heap/long-session registry growth requires a separate measured lifecycle pass. |
| audit-resource-lifecycle | Deferred specialization | Cancellation/router ownership was sampled. Full process-tree/FD/tempfile shutdown drills were not executed. |
| audit-operator-signal | Applied to permission failures | Failure-to-log/UI trace, actionability, detectability and safe-state analysis below. |
| audit-dataflow-pipeline-graph | Deferred specialization | Shared permission producer/consumer map is in the base report; no separately generated whole-system dependency graph. |
| audit-recovery-idempotency | Deferred specialization | Same-writer rollback and one-shot resolution were checked; crash/replay/duplicate-operation scheduling remains frontier. |
| audit-security-repo-posture | Not applicable | CI, branch protection, package publishing and repository supply-chain posture are outside the user's permission issue. |
| audit-security-repo-triage | Not applicable | This is a resolved permission scope, not first-contact repository security triage. |
| audit-security-vuln-harness | Deferred specialization | Small isolated regression probes were run under test authority; no full adversarial exploit harness or live-target proof is claimed. |
| audit-design-webapp | Deferred specialization | Button semantics/status were covered under WFG; no independent accessibility/visual design/browser matrix. |
| audit-playtest-app | Deferred specialization | Patched source is not installed and user prohibited an app build. Component/lifecycle tests run; no patched Electron playtest claim. |
| audit-mcp-server | Deferred specialization | Host annotation/tool classification seam was inspected. This does not cover every MCP server's discovery/schema/transport/authorization. |
| audit-agent-orchestration-code | Applied, permission subset | Mode/role, provider answer, pending state and failure paths mapped; detailed AOC disposition below names the unreviewed broader orchestration surface. |
| audit-repo-state-reconciliation | Applied, local permission evidence | Reconciled current commit, tree, source patch, TODO and cited lock-fix commit; no remote issue/PR status assertion. |
| audit-repo-path-consistency | Deferred specialization | Cargo and Vitest command/output paths show this checkout. Full relocation inventory, wrappers and alternate-machine probes were not performed. |

### Orchestration coverage

| AOC codes | Disposition | Evidence / limit |
| --- | --- | --- |
| 001,004 | Finding | SEC-GSL-901: model shell input does not preserve the workspace authority boundary through parsing. |
| 005 | Held, narrow | Hosted permission decisions aggregate explicit deny and inspector verdicts; model self-assessment does not override explicit denied tools. |
| 013,014 | Finding | WFG-GSL-901/902: approval/persistence completion state is misleading. |
| 023 | Finding | CON-GSL-901: independent actors overwrite shared policy snapshots. |
| 020 | Held with gaps | Existing permission, ACP and fake Claude tests run; new regression probes expose missing cases. |
| 006,017,018,026,029 | N/A in permission subset | No scout consensus or relay-summary approval is implemented in the traced answer/save path; broader orchestrator features are outside this subset. |
| 002,003,007,008,009,010,011,012,015,016,019,021,022,024,025,027,028,030 | Not reviewed | Vendor CLI-version contracts, secret/env construction, general planning/budgets/costs, discovery, diff/review baselines, full run checkpoints and process cleanup were not audited end to end. |

### Detection and operator signal

Production signals, not the existence of this audit's probes:

| Failure | Detection / surface / audience | Visibility and latency | Actual → required detectability; safe next action |
| --- | --- | --- | --- |
| Missed shell or upload gate | none in the missed inspector verdict; user/operator may notice effects later | silent; production detection time unknown | silent → obvious before side effects; repair the classifier/per-request gate |
| Lost/stale permission | successful save return; later repeated prompt or policy inspection reveals loss | inferred; next affected request or manual inspection | inferred → obvious for revocation; transactional refresh and explicit invalidation |
| Failed hosted persistent save | log with tool/domain and root error; log reader | log-only immediately; no UI delivery claim | logged → obvious at approval card; show save-only retry and whether tool already ran |
| Cross-session display reuse | conflicting “Allowed once” status; end user | UI-visible symptom without diagnosis; on second session render | inferred → obvious current pending state; correct composite identity |
| Legacy provider discards persistence | no persistent-save step/error; end user sees another prompt | inferred; next same-tool request | inferred → obvious persistent or one-time result; implement scoped save or withhold option |
| Stale completion record | source/Git comparison by maintainer | misleading until reconciliation | inferred → logged accurate evidence; reopen permission-specific item |

Highest attention goes to silent, potentially irreversible shell/network effects
and revoked authority; then misleading grant completion. No numeric risk-product
score or invented production SLA is used. WFG-GSL-901's structured log answers
what/tool/why but lies about present state and lacks a safe next action. Inspector
failure fallback is more honest: it names the failed inspector and requires a
decision. Bulk UI persistence failure is visible and leaves the request pending.

| SIG codes | Disposition |
| --- | --- |
| 001,006,008,009,010,012,013 | Finding: WFG-GSL-901 or INV-GSL-901; reason/outcome is missing or misleading at the approval surface. |
| 003 | Held in scope: action-required/Pending tells the operator what is waiting. |
| 002 | N/A: no readiness/health endpoint claim is part of the permission UI. |
| 004,005,007,011 | Frontier: disconnected-client stall watchdog, end-to-end correlation and notification/alert delivery were not measured. |

## Recommended patch order and regression strategy

1. Fix SEC-GSL-901 and SEC-GSL-902 without widening workspace or network grants.
   Promote the isolated failing grammar/batch probes into permanent regression tests.
2. Repair CON-GSL-901 with full read/modify/write coordination and reader freshness;
   add independent-process tests as well as deterministic independent-handle tests.
3. Make persistent approval completion observable (WFG-GSL-901), then align legacy
   Claude persistence (INV-GSL-901). Use a fake provider to issue repeated requests
   and simulate save failure; retries must save the grant without rerunning a tool.
4. Correct Desktop request identity (WFG-GSL-902) with remount, in-place session
   switch and request-replacement tests.
5. Reconcile the permission-specific TODO claim (CMP-GSL-901), retaining the valid
   config.yaml history. Keep WDS-GSL-001 installation verification open until a
   separately authorized app build/install and safe diagnostic replay.

Escalation/ownership: SEC/IOP → shell classification; SEC/CAS/DAT → egress
per-request decision; CON/TMP → persistence protocol; WFG/REL/SIG → approval
completion; INV/ARC/AOC → legacy provider adapter; CMP → evidence ledger. These
are bounded repair handoffs, not changes applied during this audit.

## Replicated facts and assumptions

| Fact / source of truth | Copies and required relationship | Disposition |
| --- | --- | --- |
| “Always Allow” user choice (ToolApprovalButtons labels; shared Permission enum) | Hosted handler, ACP provider and legacy Claude response must preserve lifetime intent; concrete storage may differ | Required-identical meaning; INV-GSL-901 |
| Live request identity (permissionRequests.ts:129) | Display cache must distinguish the same session/request pairs as the live pending map | Required-identical identity; WFG-GSL-902 |
| Domain vs tool scope (acp/common.rs PermissionDecision) | ACP option kind may be shared, but domain option ID remains distinct in UI/server mapping | Legitimate representation difference; existing domain tests pass |
| Auto advisory vs explicit safety result (ToolInspector contract) | Per-inspector downgrade behavior may differ deliberately | Legitimate policy difference; scope/egress/failure prompts survive |
| Durable grant state (PermissionManager) | Reader cache and committed permission file must not silently disagree about revocation | Required freshness protocol missing; CON-GSL-901 |
| Completion evidence (docs/TODO.md:42/:54) | Source file named in completion claim must match cited implementation/test evidence | Required evidence correspondence; CMP-GSL-901 |

Load-bearing assumptions challenged: only one permission writer exists (false);
request IDs are globally unique across sessions (not enforced); shell token
splitting models the executing shell (false); checking a URL once checks later
calls to it (false); resolving an approval proves persistence (false). None
requires a hypothetical future integration.

## Executed validation

Commands ran from this checkout using `source bin/activate-hermit`. Cargo output
identified `crates/gosling` and `crates/gosling-cli` under this repository;
Vitest identified `ui/desktop`. No application entrypoint was launched.

| Check | Result |
| --- | --- |
| `cargo test -p gosling --lib working_dir_scope_inspector` | **32 passed**; includes all three comment-fix regressions |
| `env -u MUNINN_MCP_BEARER_TOKEN cargo test -p gosling --lib` | **1,864 passed, 3 ignored**; 32 scope tests above are included, not additional |
| `cargo test -p gosling --test tool_inspection_manager_tests` | **3 passed** |
| `cargo test -p gosling-cli --lib non_interactive` | **3 passed**, including `session::non_interactive_confirmations_are_denied` |
| Desktop ToolApprovalButtons + permissionRequests existing suites | **16 passed** |
| Desktop chatSessionLifecycle, chatSessionStore, chatSessionController, sessionNotificationAdapter | **64 passed** |
| Desktop `pnpm typecheck` | **Passed**, including the temporary UI probe |
| New Rust audit probes (final run) | **2 passed, 6 failed**; the failures demonstrate open defects |
| New React cross-session audit probe | **1 failed**, reproducing WFG-GSL-902 |
| Formatting, artifact/source consistency, links and final tree checks | Recorded in the companion results artifact |

The first CLI command selected the binary target and ran **zero tests**; it is
not counted as validation. The corrected library-target command above ran the
actual tests. The initial seven-probe Rust run was expanded to eight to add a
real-thread writer test and to check both GET-first and literal-first egress
cases; only final probe counts are reported here.

The library run removed `MUNINN_MCP_BEARER_TOKEN` from that test process's
environment because an existing environment-isolation test treats inherited
values as test input. No credential value was printed and no saved configuration
was changed. The three ignored library tests are manually opted-in snippet,
session-listing and dispatch benchmarks.

### Probe evidence and isolation

- [Rust probe source](2026-09-07-permissions-audit-probes.rs): actual
  PermissionManager, ToolInspectionManager, WorkingDirScopeInspector and
  EgressInspector APIs; a fresh TempDir for each test and isolated SessionManager
  SQLite directory. No global live permission manager or live session database.
- [React probe source](2026-09-07-permissions-audit-ui-probe.tsx): real
  ToolApprovalButtons component in jsdom, fresh test-module state, mocked live
  request resolution so both fixture sessions are valid; unmount/remount with a
  reused tool-call ID. It proves component state leakage, not backend execution.
- [Commands and observed output](2026-09-07-permissions-audit-results.md).
- Shell and network strings are passed only into inspectors. No launchctl, touch,
  curl or other target command in a probe is executed; `audit.invalid` is fixture
  text. Forced persistence failure makes permission.yaml a directory only inside
  the test's temporary config directory.
- The normal-return save test is a **characterization of the defect**, not proof
  of correct persistence. The benign apostrophe-comment test is a held control.
- Probes were compiled at `crates/gosling/tests/permission_audit_20260907.rs` and
  `ui/desktop/src/components/PermissionAudit20260907.test.tsx`. Their sources are
  preserved above, then the temporary runner files removed. To reproduce, copy
  them back to those locations only when the destination is absent, and run the
  recorded targeted commands. They intentionally assert the desired invariants
  and currently fail; they were not made green by accepting the broken behavior.

## Final confidence and next action

High confidence in the reproduced inspector, storage and component defects.
WFG-GSL-901's backend rollback is reproduced and its misleading UI consequence is
source-traced; INV-GSL-901 remains a deterministic source finding without a
two-request legacy-provider reproduction. CMP-GSL-901 is verified against source
and Git history. Production incident frequency, scheduler/power-loss behavior,
vendor mode compatibility and patched installed-app behavior remain unverified.

The next repair slice should address shell classification, per-call egress and
permission persistence, preserving the probe assertions. The screenshot-specific
source fix is test-verified; the installed application still requires its own
later build/install and safe verification. This audit does not close the seven
new findings or assert the permission experience is fully fixed.
