# 17 — ACP Server and Protocol Boundaries

Use a small version-pinned ACP fixture client that records raw frames,
timestamps, close codes, HTTP status, and process state. Never expose the test
shared secret in reports.

---

### AP-01 — Authenticated serve startup requirement
- Goal: `gosling serve` refuses accidental unauthenticated startup.
- Category: authorization / lifecycle
- Preconditions: free loopback port; secret absent from environment and config.
- Steps: start normally; start with empty secret; start with a generated secret; separately start with `--dangerously-unauthenticated` on loopback only.
- Expected: absent/empty secret fails before binding and names `GOSLING_SERVER__SECRET_KEY`; valid secret binds; dangerous mode emits a prominent warning; no failed attempt leaves a listener.
- Observe: dangerous mode with non-loopback host should be blocked or produce a stronger warning.

### AP-02 — Missing, wrong, and correct shared secret
- Goal: every ACP request consistently enforces `X-Secret-Key`.
- Category: authorization / invalid input
- Preconditions: authenticated serve and fixture clients with missing, wrong, whitespace-modified, and correct secrets.
- Steps: attempt handshake/request with each; establish correct socket then change server secret by restart; retry old and new credentials.
- Expected: bad credentials receive 401/defined protocol rejection without method data; correct credential works; comparison is exact; restart invalidates old connections/credentials; secret never appears in body, URL, or logs.
- Observe: rate limiting for repeated failures without locking out valid local client.

### AP-03 — Origin allowlist replacement semantics
- Goal: WebSocket/HTTP Origin checks match default and explicit allowlists.
- Category: authorization / boundary
- Preconditions: authenticated serve; clients sending loopback, `null`, absent, lookalike, and custom origins.
- Steps: test defaults; restart with one `--allowed-origin`; test old defaults and exact custom value; restart with multiple allowed origins.
- Expected: default permits only documented loopback origins; explicit values replace defaults; matching is exact by scheme/host/port; lookalikes fail before ACP messages; non-browser documented no-Origin clients behave consistently.
- Observe: rejected Origin is safe to log while query/credential data is not.

### AP-04 — TLS certificate/key validation
- Goal: TLS mode validates configuration before serving and presents the intended certificate.
- Category: files / recovery
- Preconditions: generated test CA/server certificate plus mismatched, expired, malformed, and permission-denied fixtures.
- Steps: run `serve --tls` with missing paths, cert-only, key-only, mismatch, malformed files, then valid pair; connect with trusted and untrusted clients.
- Expected: invalid pairs fail before bind with the offending path/type; no private-key content in errors; valid endpoint negotiates TLS and serves the expected cert; untrusted client fails closed; clean shutdown releases port.
- Observe: config values versus CLI path precedence.

### AP-05 — Stdio framing and stdout cleanliness
- Goal: `gosling acp` reserves stdout for protocol frames.
- Category: files / protocol
- Preconditions: fixture client capturing stdout and stderr separately; configured and unconfigured roots.
- Steps: initialize, create session, send prompt, and shutdown; repeat with verbose/logging env and a provider error; parse every stdout frame.
- Expected: every stdout item is valid ACP framing/JSON with no banners, ANSI, or logs; diagnostics use stderr; EOF and shutdown terminate within deadline; provider failure remains a structured protocol event.
- Observe: partial writes and multibyte content boundaries.

### AP-06 — Invalid ACP messages preserve the connection
- Goal: malformed client input receives bounded protocol errors without corrupting later valid requests.
- Category: invalid input / recovery
- Preconditions: initialized fixture client and documented protocol version.
- Steps: send invalid JSON, missing method/id, unknown method, wrong parameter types, duplicate request ID, oversized but safe message, then a valid request after each where connection policy permits.
- Expected: each error matches protocol close/response rules; server does not panic or disclose stack traces; valid follow-up succeeds when error is recoverable; unrecoverable close has a defined code and process remains available to new clients.
- Observe: memory/CPU remain stable after 100 small invalid frames.

### AP-07 — Concurrent ACP session isolation
- Goal: one server supports concurrent clients without event or state leakage.
- Category: concurrency / persistence
- Preconditions: two authenticated clients, separate cwd fixtures, unique markers, and delayed provider responses.
- Steps: initialize both; create sessions concurrently; send overlapping prompts/tool calls; list/resume each; disconnect one while the other streams.
- Expected: request IDs/events route only to owning client; cwd, approvals, and history remain isolated; disconnect A does not cancel B; persisted IDs are unique; reconnection follows documented ownership rules.
- Observe: per-client backpressure when one client stops reading.

### AP-08 — ACP cancellation reaches a terminal state
- Goal: protocol cancellation stops provider and tool activity exactly once.
- Category: interruption / protocol
- Preconditions: delayed provider and cancellable MCP tool fixtures with observable side-effect checkpoints.
- Steps: cancel before first token, mid-stream, during tool wait, after completion, and twice with the same request ID; send a clean follow-up.
- Expected: active cases produce one terminal cancelled state within 10 seconds; no post-cancel side effect crosses its checkpoint; late/duplicate cancel is idempotent or a defined error; follow-up succeeds.
- Observe: cancellation propagation to child processes and approval prompts.
- Variations: revoke the turn's lease independently of the client's own cancel call (e.g. a second client taking over the session) and confirm the reported terminal state names lease loss distinctly from a caller-initiated cancellation, per `ensure_turn_not_revoked`; also confirm compaction in flight when cancellation arrives does not save a replacement summary over the intact prior history.

### AP-09 — Server termination during an active request
- Goal: clients and persisted sessions recover honestly after graceful and abrupt server loss.
- Category: recovery / interruption
- Preconditions: active delayed request; process and port observation; disposable persisted root.
- Steps: send SIGTERM during stream; restart and inspect; repeat with SIGKILL; reconnect client and resume/create session after each.
- Expected: SIGTERM closes with a meaningful event/code and releases port; SIGKILL is detected by client without false success; restart needs no manual lock cleanup; interrupted history is terminal, not forever running.
- Observe: difference between acknowledged and unacknowledged chunks at persistence boundary.

### AP-10 — Protocol version and capability negotiation
- Goal: clients can determine supported ACP behavior without guessing.
- Category: boundary / protocol
- Preconditions: fixture clients representing current, older supported, newer/unknown, and malformed protocol versions/capabilities.
- Steps: initialize each; request capabilities; invoke one advertised optional method; invoke one unadvertised method; compare stdio and serve responses.
- Expected: compatible version negotiates explicitly; incompatible version fails before session creation with supported range; advertised capabilities work; absent capability is not callable; transports report the same server contract.
- Observe: Desktop embedded backend and release CLI version skew.
