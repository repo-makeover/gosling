//! The durable tool-operation ledger: dispatch-once/replay-on-redispatch
//! bookkeeping (`begin`/`complete`/`persist`), crash recovery for
//! interrupted tool calls (`recover_tool_operations`, live-peer-aware via
//! `owner_pid`), and cancellation of siblings left undispatched when a turn
//! ends.
//!
//! Extracted from `crate::session::session_manager` in a behavior-preserving
//! modularization (see `docs/logs/session/2026-08-22-modularize-session-manager.md`).
//! `ToolOperationStart` moves here with its methods; the facade re-exports it
//! at the same `pub(crate) use tool_operations::ToolOperationStart` path so
//! `crate::session::session_manager::ToolOperationStart` (used by
//! `session::mod.rs`'s own re-export) keeps resolving unchanged.

use super::SessionStorage;
use crate::conversation::message::{Message, MessageContent};
use crate::mcp_utils::ToolResult;
use anyhow::Result;
use rmcp::model::{CallToolRequestParams, CallToolResult, ErrorCode, ErrorData};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const TOOL_OPERATION_SCHEMA_VERSION: i32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ToolOperationStart {
    Execute { operation_id: String },
    Replay { result: ToolResult<CallToolResult> },
    InDoubt { operation_id: String },
}

impl SessionStorage {
    pub(super) async fn begin_tool_operation(
        &self,
        session_id: &str,
        tool_request_id: &str,
        tool_call: &CallToolRequestParams,
        conversation_bound: bool,
    ) -> Result<ToolOperationStart> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let request_digest = tool_operation_request_digest(tool_call)?;
        let existing = sqlx::query_as::<_, (String, String, String, bool, String, Option<String>)>(
            r#"
            SELECT operation_id, tool_name, request_digest, conversation_bound, state, result_json
            FROM tool_operations
            WHERE session_id = ? AND tool_request_id = ?
            "#,
        )
        .bind(session_id)
        .bind(tool_request_id)
        .fetch_optional(&mut *tx)
        .await?;

        let outcome = if let Some((
            operation_id,
            stored_name,
            stored_digest,
            stored_conversation_bound,
            state,
            result_json,
        )) = existing
        {
            if stored_name != tool_call.name.as_ref()
                || stored_digest != request_digest
                || stored_conversation_bound != conversation_bound
            {
                anyhow::bail!(
                    "tool request id {tool_request_id} was already used with a different tool payload"
                );
            }
            match state.as_str() {
                "completed" => ToolOperationStart::Replay {
                    result: deserialize_tool_operation_result(result_json.as_deref().ok_or_else(
                        || anyhow::anyhow!("completed tool operation has no terminal result"),
                    )?)?,
                },
                "started" | "in_doubt" => ToolOperationStart::InDoubt { operation_id },
                other => anyhow::bail!("tool operation has invalid state {other}"),
            }
        } else {
            if conversation_bound {
                let checkpointed_content = sqlx::query_scalar::<_, String>(
                    r#"
                    SELECT tool_request.value
                    FROM messages, json_each(messages.content_json) AS tool_request
                    WHERE messages.session_id = ?
                      AND json_extract(tool_request.value, '$.type') = 'toolRequest'
                      AND json_extract(tool_request.value, '$.id') = ?
                    ORDER BY messages.id DESC
                    LIMIT 1
                    "#,
                )
                .bind(session_id)
                .bind(tool_request_id)
                .fetch_optional(&mut *tx)
                .await?;
                let checkpointed_content = checkpointed_content.ok_or_else(|| {
                    anyhow::anyhow!(
                        "tool request {tool_request_id} must be durably checkpointed before dispatch"
                    )
                })?;
                let MessageContent::ToolRequest(request) =
                    serde_json::from_str(&checkpointed_content)?
                else {
                    anyhow::bail!("checkpointed tool request is malformed");
                };
                let persisted_call = request.tool_call.map_err(|error| {
                    anyhow::anyhow!(
                        "checkpointed tool request {tool_request_id} is invalid: {error}"
                    )
                })?;
                if persisted_call.name != tool_call.name
                    || persisted_call.arguments != tool_call.arguments
                {
                    anyhow::bail!(
                        "checkpointed tool request {tool_request_id} has a different tool payload"
                    );
                }
            }
            let operation_id = format!("toolop_{}", uuid::Uuid::new_v4());
            sqlx::query(
                r#"
                INSERT INTO tool_operations (
                    operation_id, schema_version, session_id, tool_request_id, tool_name, request_digest,
                    conversation_bound, owner_id, owner_pid, state
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'started')
                "#,
            )
            .bind(&operation_id)
            .bind(TOOL_OPERATION_SCHEMA_VERSION)
            .bind(session_id)
            .bind(tool_request_id)
            .bind(tool_call.name.as_ref())
            .bind(request_digest)
            .bind(conversation_bound)
            .bind(&self.owner_id)
            .bind(std::process::id() as i64)
            .execute(&mut *tx)
            .await?;
            ToolOperationStart::Execute { operation_id }
        };

        let active_operation_id = match &outcome {
            ToolOperationStart::Execute { operation_id } => Some(operation_id.clone()),
            _ => None,
        };
        if let Some(operation_id) = &active_operation_id {
            self.active_tool_operations
                .lock()
                .expect("active tool operations mutex poisoned")
                .insert(operation_id.clone());
        }
        if let Err(error) = tx.commit().await {
            if let Some(operation_id) = &active_operation_id {
                self.release_tool_operation(operation_id);
            }
            return Err(error.into());
        }
        Ok(outcome)
    }

    pub(super) fn release_tool_operation(&self, operation_id: &str) {
        self.active_tool_operations
            .lock()
            .expect("active tool operations mutex poisoned")
            .remove(operation_id);
    }

    pub(super) async fn mark_tool_operation_in_doubt(&self, operation_id: &str) -> Result<()> {
        self.release_tool_operation(operation_id);
        let result = Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            "Tool execution ended without a durable terminal result. Its execution status is in doubt and it must not be retried automatically.".to_string(),
            Some(serde_json::json!({
                "tool_operation_id": operation_id,
                "status": "in_doubt",
                "retryable": false
            })),
        ));
        let result_json = serialize_tool_operation_result(&result)?;
        sqlx::query(
            r#"
            UPDATE tool_operations
            SET state = 'in_doubt', result_json = ?, updated_at = CURRENT_TIMESTAMP
            WHERE operation_id = ? AND state = 'started'
            "#,
        )
        .bind(result_json)
        .bind(operation_id)
        .execute(self.pool().await?)
        .await?;
        Ok(())
    }

    pub(super) async fn complete_tool_operation(
        &self,
        operation_id: &str,
        result: &ToolResult<CallToolResult>,
    ) -> Result<()> {
        let result_json = serialize_tool_operation_result(result)?;
        let pool = self.pool().await?;
        let updated = sqlx::query(
            r#"
            UPDATE tool_operations
            SET state = 'completed', result_json = ?, updated_at = CURRENT_TIMESTAMP
            WHERE operation_id = ? AND state = 'started'
            "#,
        )
        .bind(&result_json)
        .bind(operation_id)
        .execute(pool)
        .await?;

        if updated.rows_affected() == 0 {
            let existing = sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT state, result_json FROM tool_operations WHERE operation_id = ?",
            )
            .bind(operation_id)
            .fetch_optional(pool)
            .await?;
            match existing {
                Some((state, Some(stored))) if state == "completed" && stored == result_json => {}
                Some((state, _)) => {
                    anyhow::bail!("cannot complete tool operation in state {state}")
                }
                None => anyhow::bail!("tool operation {operation_id} does not exist"),
            }
        }
        Ok(())
    }

    pub(super) async fn persist_tool_operation_response(
        &self,
        session_id: &str,
        tool_request_id: &str,
        message: &Message,
    ) -> Result<()> {
        let response_message_id = message
            .id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("tool response message must have a stable id"))?;
        let has_response = message.content.iter().any(
            |content| matches!(content, MessageContent::ToolResponse(response) if response.id == tool_request_id),
        );
        if !has_response {
            anyhow::bail!("tool response message does not answer request {tool_request_id}");
        }

        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let state = sqlx::query_scalar::<_, String>(
            "SELECT state FROM tool_operations WHERE session_id = ? AND tool_request_id = ?",
        )
        .bind(session_id)
        .bind(tool_request_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow::anyhow!("tool operation does not exist"))?;
        if state != "completed" {
            anyhow::bail!("cannot persist response for tool operation in state {state}");
        }

        Self::upsert_message_in_tx(&mut tx, session_id, message).await?;
        sqlx::query(
            r#"
            UPDATE tool_operations
            SET response_message_id = ?, updated_at = CURRENT_TIMESTAMP
            WHERE session_id = ? AND tool_request_id = ?
            "#,
        )
        .bind(response_message_id)
        .bind(session_id)
        .bind(tool_request_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn recover_tool_operations(&self, session_id: &str) -> Result<usize> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let operations = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                Option<i64>,
                String,
                Option<String>,
            ),
        >(
            r#"
            SELECT operation_id, tool_request_id, tool_name, owner_id, owner_pid, state, result_json
            FROM tool_operations
            WHERE session_id = ?
              AND conversation_bound = TRUE
              AND response_message_id IS NULL
              AND state IN ('started', 'completed', 'in_doubt')
            ORDER BY created_at, operation_id
            "#,
        )
        .bind(session_id)
        .fetch_all(&mut *tx)
        .await?;
        if operations.is_empty() {
            tx.commit().await?;
            return Ok(0);
        }

        let stored_messages = sqlx::query_as::<_, (String, String)>(
            "SELECT message_id, content_json FROM messages WHERE session_id = ? ORDER BY id",
        )
        .bind(session_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut request_metadata_by_id = HashMap::new();
        let mut response_message_id_by_request = HashMap::new();
        for (message_id, content_json) in stored_messages {
            let contents: Vec<MessageContent> = serde_json::from_str(&content_json)?;
            for content in contents {
                match content {
                    MessageContent::ToolRequest(request) => {
                        request_metadata_by_id.insert(request.id, request.metadata);
                    }
                    MessageContent::ToolResponse(response) => {
                        response_message_id_by_request.insert(response.id, message_id.clone());
                    }
                    _ => {}
                }
            }
        }
        let mut recovered = 0;
        let active_operations = self
            .active_tool_operations
            .lock()
            .expect("active tool operations mutex poisoned")
            .clone();
        // REL-GSL-006: who, if anyone, is running a turn on this session right
        // now. Read once, inside the same transaction as the operations, so
        // every row in this pass is judged against one consistent answer.
        let live_turn_owner = self.live_turn_owner(&mut tx, session_id).await?;

        for (operation_id, request_id, tool_name, owner_id, owner_pid, state, stored_result) in
            operations
        {
            if owner_id == self.owner_id {
                if active_operations.contains(&operation_id) {
                    continue;
                }
            } else if state == "started" {
                // CON-GSL-001: a foreign `owner_id` alone doesn't mean the
                // owner is dead -- it's a per-instance UUID, not a liveness
                // signal, so recovering unconditionally could mark a live
                // peer's in-flight tool `in_doubt` mid-execution. Only a
                // `started` row is still "in flight"; `completed`/`in_doubt`
                // rows below are already terminal and safe to finalize
                // regardless of who dispatched them. Probe the owner's OS
                // process directly rather than guessing from `updated_at`
                // recency, which only moves on state transitions and would
                // either stomp a slow live tool or delay a genuine crash's
                // `in_doubt` signal.
                //
                // REL-GSL-006: a live owner process is not a running turn.
                // Requiring only a live process left operations `started`
                // indefinitely whenever the owner survived but its turn did
                // not -- the 2026-09-05 write-gate deadlock stranded three
                // that way for hours, and only an app restart cleared them.
                // A tool call can only still be executing inside a turn, and a
                // turn is exactly what the session turn lease names, so the
                // second half of the test is that this operation's owner is
                // the one currently holding that lease. If it is not, its turn
                // is over and nothing is running that recovery could interrupt.
                // Recovery surfaces the row as `in_doubt` and never as
                // retryable, so an operation whose side effects did land is
                // still never repeated automatically.
                let owner_process_is_live = match owner_pid {
                    Some(pid) => match u32::try_from(pid) {
                        Ok(pid) => crate::subprocess::process_is_alive(pid).await,
                        Err(_) => false,
                    },
                    None => false,
                };
                if owner_process_is_live && live_turn_owner.as_deref() == Some(owner_id.as_str()) {
                    continue;
                }
            }

            let request_exists = request_metadata_by_id.contains_key(&request_id);
            let request_metadata = request_metadata_by_id.remove(&request_id).flatten();
            let existing_response_message_id = response_message_id_by_request.remove(&request_id);

            if let Some(message_id) = existing_response_message_id {
                sqlx::query(
                    "UPDATE tool_operations SET response_message_id = ?, updated_at = CURRENT_TIMESTAMP WHERE operation_id = ?",
                )
                .bind(message_id)
                .bind(&operation_id)
                .execute(&mut *tx)
                .await?;
                recovered += 1;
                continue;
            }

            if !request_exists {
                let request = Message::assistant().with_generated_id().with_tool_request(
                    request_id.clone(),
                    Ok(CallToolRequestParams::new(tool_name)),
                );
                Self::upsert_message_in_tx(&mut tx, session_id, &request).await?;
            }

            let result = match state.as_str() {
                "completed" => deserialize_tool_operation_result(
                    stored_result.as_deref().ok_or_else(|| {
                        anyhow::anyhow!("completed tool operation has no terminal result")
                    })?,
                )?,
                "started" | "in_doubt" => Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Tool execution was interrupted after durable dispatch began. Its execution status is in doubt and it must not be retried automatically. Verify the external state before deciding how to proceed.".to_string(),
                    Some(serde_json::json!({
                        "tool_operation_id": operation_id,
                        "status": "in_doubt",
                        "retryable": false
                    })),
                )),
                other => anyhow::bail!("tool operation has invalid state {other}"),
            };
            let result_json = serialize_tool_operation_result(&result)?;
            let mut response = Message::user().with_generated_id();
            response.add_tool_response_with_metadata(
                request_id.clone(),
                result,
                request_metadata.as_ref(),
            );
            let response_message_id = response.id.clone().expect("generated message id");
            Self::upsert_message_in_tx(&mut tx, session_id, &response).await?;
            sqlx::query(
                r#"
                UPDATE tool_operations
                SET state = CASE WHEN state = 'completed' THEN state ELSE 'in_doubt' END,
                    result_json = ?, response_message_id = ?, updated_at = CURRENT_TIMESTAMP
                WHERE operation_id = ?
                "#,
            )
            .bind(result_json)
            .bind(response_message_id)
            .bind(operation_id)
            .execute(&mut *tx)
            .await?;
            recovered += 1;
        }

        tx.commit().await?;
        Ok(recovered)
    }

    pub(super) async fn cancel_undispatched_tool_requests(
        &self,
        session_id: &str,
        cancelled_request_id: &str,
    ) -> Result<usize> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let operation_request_ids = sqlx::query_scalar::<_, String>(
            "SELECT tool_request_id FROM tool_operations WHERE session_id = ? AND conversation_bound = TRUE",
        )
        .bind(session_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect::<HashSet<_>>();
        let stored_messages = sqlx::query_scalar::<_, String>(
            "SELECT content_json FROM messages WHERE session_id = ? ORDER BY id",
        )
        .bind(session_id)
        .fetch_all(&mut *tx)
        .await?;

        let mut sibling_requests = HashMap::new();
        let mut answered_request_ids = HashSet::new();
        for content_json in stored_messages {
            let contents: Vec<MessageContent> = serde_json::from_str(&content_json)?;
            let contains_cancelled_request = contents.iter().any(
                |content| matches!(content, MessageContent::ToolRequest(request) if request.id == cancelled_request_id),
            );
            for content in contents {
                match content {
                    MessageContent::ToolRequest(request) if contains_cancelled_request => {
                        sibling_requests.insert(request.id, request.metadata);
                    }
                    MessageContent::ToolResponse(response) => {
                        answered_request_ids.insert(response.id);
                    }
                    _ => {}
                }
            }
        }

        let mut cancelled = 0;
        for (request_id, request_metadata) in sibling_requests {
            if answered_request_ids.contains(&request_id)
                || operation_request_ids.contains(&request_id)
            {
                continue;
            }
            let mut response = Message::user().with_generated_id();
            response.add_tool_response_with_metadata(
                request_id,
                Err(ErrorData::new(
                    ErrorCode::INVALID_REQUEST,
                    "Tool execution was cancelled before it started because the prior turn ended. It will not be retried automatically.".to_string(),
                    None,
                )),
                request_metadata.as_ref(),
            );
            Self::upsert_message_in_tx(&mut tx, session_id, &response).await?;
            cancelled += 1;
        }

        tx.commit().await?;
        Ok(cancelled)
    }
}

fn serialize_tool_operation_result(result: &ToolResult<CallToolResult>) -> Result<String> {
    serde_json::to_string(&MessageContent::tool_response("ledger", result.clone()))
        .map_err(Into::into)
}

fn tool_operation_request_digest(tool_call: &CallToolRequestParams) -> Result<String> {
    let digest = Sha256::digest(serde_json::to_vec(tool_call)?);
    Ok(crate::utils::bytes_to_hex(digest))
}

fn deserialize_tool_operation_result(value: &str) -> Result<ToolResult<CallToolResult>> {
    match serde_json::from_str::<MessageContent>(value)? {
        MessageContent::ToolResponse(response) => Ok(response.tool_result),
        _ => anyhow::bail!("tool operation result is malformed"),
    }
}

#[cfg(test)]
mod tests {
    use super::ToolOperationStart;
    use crate::config::GoslingMode;
    use crate::conversation::message::Message;
    use crate::session::session_manager::{SessionManager, SessionType};
    use rmcp::model::CallToolRequestParams;
    use std::path::PathBuf;
    use tempfile::TempDir;

    async fn tool_session(sm: &SessionManager) -> String {
        sm.create_session(
            PathBuf::from("/tmp/tool-operations"),
            "Tool operations".to_string(),
            SessionType::User,
            GoslingMode::default(),
        )
        .await
        .unwrap()
        .id
    }

    fn shell_call(command: &str) -> CallToolRequestParams {
        CallToolRequestParams::new("developer__shell")
            .with_arguments(rmcp::object!({ "command": command }))
    }

    async fn add_tool_round(sm: &SessionManager, session_id: &str, request_id: &str, output: &str) {
        let call = shell_call(&format!("echo {request_id}"));
        sm.add_message(
            session_id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request(request_id, Ok(call)),
        )
        .await
        .unwrap();
        sm.add_message(
            session_id,
            &Message::user().with_generated_id().with_tool_response(
                request_id,
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(output),
                ])),
            ),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_begin_tool_operation_checks_the_newest_checkpoint_of_a_request_id() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session_id = tool_session(&sm).await;
        for index in 0..20 {
            add_tool_round(&sm, &session_id, &format!("earlier-{index}"), "done").await;
        }
        let superseded = shell_call("echo superseded");
        sm.add_message(
            &session_id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("repeated", Ok(superseded.clone())),
        )
        .await
        .unwrap();
        let current = shell_call("echo current");
        sm.add_message(
            &session_id,
            &Message::assistant()
                .with_generated_id()
                .with_tool_request("repeated", Ok(current.clone())),
        )
        .await
        .unwrap();

        let mismatch = sm
            .begin_tool_operation(&session_id, "repeated", &superseded, true)
            .await
            .unwrap_err();
        assert!(mismatch.to_string().contains("different tool payload"));
        assert!(matches!(
            sm.begin_tool_operation(&session_id, "repeated", &current, true)
                .await
                .unwrap(),
            ToolOperationStart::Execute { .. }
        ));

        let older = shell_call("echo earlier-3");
        assert!(matches!(
            sm.begin_tool_operation(&session_id, "earlier-3", &older, true)
                .await
                .unwrap(),
            ToolOperationStart::Execute { .. }
        ));
        let missing = sm
            .begin_tool_operation(&session_id, "never-checkpointed", &older, true)
            .await
            .unwrap_err();
        assert!(missing.to_string().contains("must be durably checkpointed"));
    }

    #[tokio::test]
    #[ignore = "manual same-workload tool dispatch benchmark; run with --ignored --nocapture"]
    async fn benchmark_begin_tool_operation() {
        use std::time::Instant;

        fn report(label: &str, mut samples: Vec<f64>) {
            samples.sort_by(f64::total_cmp);
            println!(
                "{label}: median={:.2}us p95={:.2}us min={:.2}us max={:.2}us n={}",
                samples[samples.len() / 2],
                samples[(samples.len() * 95).div_ceil(100) - 1],
                samples[0],
                samples[samples.len() - 1],
                samples.len(),
            );
        }

        const ROUNDS: usize = 300;
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session_id = tool_session(&sm).await;
        let output = "x".repeat(20_000);
        for index in 0..ROUNDS {
            add_tool_round(&sm, &session_id, &format!("round-{index}"), &output).await;
        }
        let newest = format!("round-{}", ROUNDS - 1);
        let call = shell_call(&format!("echo {newest}"));
        let pool = sm.storage().pool().await.unwrap();
        let reset = |request_id: &str| {
            sqlx::query("DELETE FROM tool_operations WHERE session_id = ? AND tool_request_id = ?")
                .bind(session_id.clone())
                .bind(request_id.to_string())
                .execute(pool)
        };

        for _ in 0..5 {
            sm.begin_tool_operation(&session_id, &newest, &call, true)
                .await
                .unwrap();
            reset(&newest).await.unwrap();
        }
        let mut samples = Vec::new();
        for _ in 0..30 {
            let start = Instant::now();
            let started = sm
                .begin_tool_operation(&session_id, &newest, &call, true)
                .await
                .unwrap();
            samples.push(start.elapsed().as_secs_f64() * 1_000_000.0);
            assert!(matches!(started, ToolOperationStart::Execute { .. }));
            reset(&newest).await.unwrap();
        }
        report(
            &format!("begin_tool_operation, newest request after {ROUNDS} tool rounds"),
            samples,
        );
    }
}
