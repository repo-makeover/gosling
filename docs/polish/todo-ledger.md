# Source TODO ledger

Date: 2026-08-27

This ledger covers actionable `TODO` markers in Rust source and tests. The
repository backlog remains [`docs/TODO.md`](../TODO.md); this file provides
stable ownership for source-local markers.

| ID | File | Line/Area | Type | Status | Owner/Context | Disposition |
|---|---|---|---|---|---|---|
| POLISH-20260827-001 | `crates/gosling-server/src/commands/agent.rs` | startup bridge | migration | resolved (2026-09-09) | Desktop / ACP migration | Verified nothing in the workspace depends on `gosling-server`, no CI workflow builds it, and `justfile`'s packaging steps already strip any `goslingd` binary before shipping Desktop — confirming Desktop already launches `gosling serve` directly. Removed the `gosling-server` crate (including this bridge) and its dead `openapi.json`/doc references. |
| POLISH-20260827-002 | `crates/gosling-test-support/src/session.rs`; `crates/gosling/tests/acp_common_tests/mod.rs` | OpenAI fixtures | test coverage | open | Provider test fixtures | Add Responses API SSE fixtures before routing these tests through Responses-only models. |
| POLISH-20260827-003 | `crates/gosling-providers/src/canonical.rs` | recommended models | product design | deferred | Provider discovery | Decide whether recommended-model discovery should reconcile the bundled registry with live provider APIs. |
| POLISH-20260827-004 | `crates/gosling/tests/acp_fixtures/mod.rs` | test data roots | architecture | blocked | Runtime path ownership | Scope process-global path access before making the ACP fixture data root fully isolated. |
| POLISH-20260827-005 | `crates/gosling/tests/acp_provider_test.rs` | four ignored tests | implementation | open | ACP provider | Implement ACP provider session loading, then enable the four ignored conformance tests. |
| POLISH-20260827-006 | `crates/gosling/src/acp/server.rs` | agent construction | architecture | blocked | Runtime path ownership | Move request logging and remaining path reads from global `Paths` state to `RuntimePaths`. |
| POLISH-20260827-007 | `crates/gosling/src/agents/platform_extensions/orchestrator.rs` | start-agent schema | product design | deferred | Orchestrator contract | Define model-tier semantics before expanding the orchestrator tool contract. |
| POLISH-20260827-008 | `crates/gosling/src/providers/formats/gcpvertexai.rs` | MaaS request format | provider compatibility | blocked | GCP publisher evidence | Select MaaS wire formats by publisher after supported publisher behavior is known. |
| POLISH-20260827-009 | `crates/gosling/src/otel/otlp.rs` | metric temporality | upstream workaround | blocked | OpenTelemetry release | Remove the temporality workaround after OpenTelemetry Rust PR 3351 is available in the selected release. |

The SQL rollback helper deliberately emits a `-- TODO:` line into generated
rollback output when it cannot safely invert a statement. Product strings and
test data for the built-in todo extension are likewise not source-debt markers.
