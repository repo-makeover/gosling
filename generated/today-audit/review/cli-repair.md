# CLI cancellation repair stage

Finding: REVIEW-REL-002, P2 reliability, low complexity. Source: independent repair report; parent authorized CLI consumer patch.
Baseline: a48108750945e42509164980e49ad452c3e12e79 with other agents' disjoint working-tree changes; session/mod.rs unchanged before this stage.
Touch set: process_agent_response cancellation/EOF exit and same module regression tests. Existing contracts: CLI machine output is completed only without terminal error; existing handle_interrupted_messages preserves human interrupt behavior and durable noninteractive cancellation notice. No schema/API/config changes.
Plan: route cancelled EOF and cancellation-select exits through one shared existing interruption-cleanup block. Preserve errors, tool denial/cancel and elicitation exits. Add real CliSession pre-cancelled reply test covering both JSON modes repeatedly, plus uncancelled slash-command control. Parent independently reviews this patch; reviewer does not self-approve.
Before/after: baseline source permits EOF -> completed when token already cancelled. Regression execution pending before production patch.

## Execution and closure

Baseline regression executed through actual CliSession::process_agent_response and failed: `cancelled EOF must not report completion: ()`. Log: cli-cancel-baseline.log. This promotes REVIEW-REL-002 from Likely to Confirmed/test-reproduced for simultaneous ready cancellation and empty stream.

Patch: EOF samples original token and selected cancellation sets the same interrupted flag. Both leave the stream before the shared existing handle_interrupted_messages call and unchanged noninteractive `Run cancelled by user` error assignment. Explicit agent error/tool/elicitation exits retain previous handling. Ordinary EOF and /clear command success remain unchanged.

Validation:
- `cargo fmt`: passed.
- `cargo test -p gosling-cli --lib reply_`: 2 passed, 254 filtered (actual cancelled replies repeated 16 times each in JSON/stream-JSON; exactly one durable notice, plus successful /clear control).
- `cargo test -p gosling-cli --lib session::tests`: 18 passed, two unrelated model-switch tests failed because ambient persisted thinking effort was High. Their code paths do not call changed response method.
- Fresh process using existing documented `GOSLING_PATH_ROOT` override, created with mktemp; exact newly compiled binary `target/debug/deps/gosling_cli-d9d7142ef1dc0a37 session::tests`: all 20 passed. Real user configuration untouched. Both logs preserved.
- Parent independently reviewed source, adjacent output/error consumers and tests; no blocker. Reviewer does not count own CLI implementation as independent review.

Contract delta: no new drift. Machine completed/error output semantics, interruption text, and exit Result behavior preserved except corrected false success on cancelled EOF. No public schema, configuration, dependency, or storage-format change. No source TODO claimed this edge. Current source record REVIEW-REL-002 is closed by this addendum; parent owns aggregated repo session log and final Clippy evidence.

Status: completed_verified for local defect and session regression surface; no installed CLI subprocess or real Ctrl-C signal drill claimed.
