//! Message read/write/paging/search and conversation replace/truncate: the
//! session transcript's storage layer, independent of session metadata,
//! tool operations, artifacts, or the library/summary side stores.
//!
//! Extracted from `crate::session::session_manager` in a behavior-preserving
//! modularization (see `docs/logs/session/2026-08-22-modularize-session-manager.md`).
//! Kept as one file despite exceeding the ~400-line seam-size guideline:
//! `add_message`/`upsert_message_in_tx` share near-identical bodies that must
//! not be pulled apart into separate files (that would manufacture copy-drift
//! risk for the next maintainer, per the hazard catalog). `pub(super)` on
//! most methods matches their pre-extraction (private, same-module)
//! visibility; `replace_conversation` stays `pub` (external callers reach it
//! through `SessionManager::replace_conversation`, itself unchanged).

use super::{
    role_to_string, SessionMessagePage, SessionMessageSearchMatch, SessionMessageSearchResults,
    SessionStorage, MAX_SESSION_MESSAGE_PAGE_LIMIT,
};
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use anyhow::Result;
use gosling_providers::conversation::token_usage::Usage;
use rmcp::model::Role;
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;

impl SessionStorage {
    pub(super) async fn get_conversation(&self, session_id: &str) -> Result<Conversation> {
        let pool = self.pool().await?;
        let rows = sqlx::query_as::<_, (String, String, i64, Option<String>, Option<String>)>(
            // Order by created_timestamp, then by id to break ties. created_timestamp is in seconds,
            // so messages created in the same second (e.g., tool request and response) need to
            // maintain their insertion order via the auto-increment id.
            "SELECT role, content_json, created_timestamp, metadata_json, message_id FROM messages WHERE session_id = ? ORDER BY created_timestamp, id",
        )
            .bind(session_id)
            .fetch_all(pool)
            .await?;

        let mut messages = Vec::new();
        for (role_str, content_json, created_timestamp, metadata_json, message_id) in
            rows.into_iter()
        {
            let role = match role_str.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => continue,
            };

            let content = serde_json::from_str(&content_json)?;
            let metadata = metadata_json
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();

            let mut message = Message::new(role, created_timestamp, content);
            message.metadata = metadata;
            if let Some(id) = message_id {
                message = message.with_id(id);
            }
            messages.push(message);
        }

        Ok(Conversation::new_unvalidated(messages))
    }

    fn row_to_message(
        role_str: String,
        content_json: String,
        created_timestamp: i64,
        metadata_json: Option<String>,
        message_id: Option<String>,
    ) -> Result<Option<Message>> {
        let role = match role_str.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => return Ok(None),
        };
        let content = serde_json::from_str(&content_json)?;
        let metadata = metadata_json
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let mut message = Message::new(role, created_timestamp, content);
        message.metadata = metadata;
        if let Some(id) = message_id {
            message = message.with_id(id);
        }
        Ok(Some(message))
    }

    async fn get_message_page_rows(
        &self,
        session_id: &str,
        before_row_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<(i64, Message)>> {
        let pool = self.pool().await?;
        let page_limit = limit.clamp(1, MAX_SESSION_MESSAGE_PAGE_LIMIT);
        let rows = if let Some(before_row_id) = before_row_id {
            sqlx::query_as::<_, (i64, String, String, i64, Option<String>, Option<String>)>(
                    "SELECT id, role, content_json, created_timestamp, metadata_json, message_id FROM messages WHERE session_id = ? AND id < ? ORDER BY id DESC LIMIT ?",
                )
                .bind(session_id)
                .bind(before_row_id)
                .bind((page_limit + 1) as i64)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query_as::<_, (i64, String, String, i64, Option<String>, Option<String>)>(
                    "SELECT id, role, content_json, created_timestamp, metadata_json, message_id FROM messages WHERE session_id = ? ORDER BY id DESC LIMIT ?",
                )
                .bind(session_id)
                .bind((page_limit + 1) as i64)
                .fetch_all(pool)
                .await?
        };

        let mut messages = Vec::new();
        for (row_id, role, content_json, created, metadata_json, message_id) in rows {
            if let Some(message) =
                Self::row_to_message(role, content_json, created, metadata_json, message_id)?
            {
                messages.push((row_id, message));
            }
        }
        Ok(messages)
    }

    pub(super) async fn get_session_message_page(
        &self,
        session_id: &str,
        before_cursor: Option<&str>,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        let before_row_id = before_cursor
            .map(|cursor| cursor.parse::<i64>())
            .transpose()
            .map_err(|_| anyhow::anyhow!("Invalid before cursor"))?;
        let page_limit = limit.clamp(1, MAX_SESSION_MESSAGE_PAGE_LIMIT);
        let total_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE session_id = ?")
                .bind(session_id)
                .fetch_one(self.pool().await?)
                .await?;
        let mut rows = self
            .get_message_page_rows(session_id, before_row_id, page_limit)
            .await?;
        let has_more = rows.len() > page_limit;
        if has_more {
            rows.truncate(page_limit);
        }
        rows.reverse();

        let oldest_row_id = rows.first().map(|(row_id, _)| *row_id);
        let newest_row_id = rows.last().map(|(row_id, _)| *row_id);
        let next_before_cursor = if has_more {
            oldest_row_id.map(|row_id| row_id.to_string())
        } else {
            None
        };
        Ok(SessionMessagePage {
            messages: rows.into_iter().map(|(_, message)| message).collect(),
            next_before_cursor,
            total_count: total_count as usize,
            oldest_row_id,
            newest_row_id,
        })
    }

    pub(super) async fn get_session_tail_page(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        let mut page_limit = limit.clamp(1, MAX_SESSION_MESSAGE_PAGE_LIMIT);
        loop {
            let page = self
                .get_session_message_page(session_id, None, page_limit)
                .await?;
            if !has_orphaned_tool_responses(&page.messages)
                || page_limit >= MAX_SESSION_MESSAGE_PAGE_LIMIT
                || page.messages.len() >= page.total_count
            {
                return Ok(page);
            }
            page_limit = (page_limit * 2).min(MAX_SESSION_MESSAGE_PAGE_LIMIT);
        }
    }

    pub(super) async fn get_session_message_rows_between(
        &self,
        session_id: &str,
        after_row_id: i64,
        before_row_id: i64,
        limit: usize,
    ) -> Result<Vec<(i64, Message)>> {
        let page_limit = limit.clamp(1, MAX_SESSION_MESSAGE_PAGE_LIMIT);
        let rows = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, Option<String>)>(
            "SELECT id, role, content_json, created_timestamp, metadata_json, message_id FROM messages WHERE session_id = ? AND id > ? AND id < ? ORDER BY id ASC LIMIT ?",
        )
        .bind(session_id)
        .bind(after_row_id)
        .bind(before_row_id)
        .bind(page_limit as i64)
        .fetch_all(self.pool().await?)
        .await?;

        let mut messages = Vec::new();
        for (row_id, role, content_json, created, metadata_json, message_id) in rows {
            if let Some(message) =
                Self::row_to_message(role, content_json, created, metadata_json, message_id)?
            {
                messages.push((row_id, message));
            }
        }
        Ok(messages)
    }

    pub(super) async fn get_session_message_window(
        &self,
        session_id: &str,
        message_id: &str,
        before: usize,
        after: usize,
    ) -> Result<Vec<Message>> {
        let pool = self.pool().await?;
        let Some((center_row_id,)) = sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM messages WHERE session_id = ? AND message_id = ? ORDER BY id LIMIT 1",
        )
        .bind(session_id)
        .bind(message_id)
        .fetch_optional(pool)
        .await?
        else {
            return Ok(Vec::new());
        };

        let before = before.min(MAX_SESSION_MESSAGE_PAGE_LIMIT.saturating_sub(1));
        let after = after.min(MAX_SESSION_MESSAGE_PAGE_LIMIT.saturating_sub(1));
        let mut leading =
            sqlx::query_as::<_, (i64, String, String, i64, Option<String>, Option<String>)>(
                "SELECT id, role, content_json, created_timestamp, metadata_json, message_id \
             FROM messages WHERE session_id = ? AND id <= ? ORDER BY id DESC LIMIT ?",
            )
            .bind(session_id)
            .bind(center_row_id)
            .bind((before + 1) as i64)
            .fetch_all(pool)
            .await?;
        leading.reverse();
        let trailing =
            sqlx::query_as::<_, (i64, String, String, i64, Option<String>, Option<String>)>(
                "SELECT id, role, content_json, created_timestamp, metadata_json, message_id \
             FROM messages WHERE session_id = ? AND id > ? ORDER BY id ASC LIMIT ?",
            )
            .bind(session_id)
            .bind(center_row_id)
            .bind(after as i64)
            .fetch_all(pool)
            .await?;
        leading.extend(trailing);

        let mut messages = Vec::with_capacity(leading.len());
        for (_row_id, role, content_json, created, metadata_json, stored_message_id) in leading {
            if let Some(message) = Self::row_to_message(
                role,
                content_json,
                created,
                metadata_json,
                stored_message_id,
            )? {
                messages.push(message);
            }
        }
        Ok(messages)
    }

    pub(super) async fn search_session_messages(
        &self,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<SessionMessageSearchResults> {
        let terms: Vec<String> = query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .map(|term| format!("%{}%", term.to_lowercase()))
            .collect();
        if terms.is_empty() {
            return Ok(SessionMessageSearchResults {
                matches: Vec::new(),
                total_matches: 0,
            });
        }

        let mut sql = String::from(
            r#"
            SELECT id, message_id, role, content_json, created_timestamp
            FROM messages
            WHERE session_id = ?
              AND EXISTS (
                SELECT 1
                FROM json_each(content_json)
                WHERE json_extract(value, '$.type') = 'text'
                  AND (
            "#,
        );
        for idx in 0..terms.len() {
            if idx > 0 {
                sql.push_str(" OR ");
            }
            sql.push_str("LOWER(json_extract(value, '$.text')) LIKE ?");
        }
        sql.push_str(")) ORDER BY id DESC LIMIT ?");

        let mut q =
            sqlx::query_as::<_, (i64, Option<String>, String, String, i64)>(&sql).bind(session_id);
        for term in &terms {
            q = q.bind(term);
        }
        q = q.bind(limit.clamp(1, MAX_SESSION_MESSAGE_PAGE_LIMIT) as i64);
        let rows = q.fetch_all(self.pool().await?).await?;

        let mut matches = Vec::new();
        for (row_id, message_id, role, content_json, created) in rows {
            let content: Vec<MessageContent> = serde_json::from_str(&content_json)?;
            let snippet = content
                .iter()
                .filter_map(|content| content.as_text())
                .collect::<Vec<_>>()
                .join("\n");
            matches.push(SessionMessageSearchMatch {
                row_id,
                message_id,
                role,
                snippet: snippet.chars().take(500).collect(),
                created,
                before_cursor: Some((row_id + 1).to_string()),
            });
        }
        let total_matches = matches.len();
        Ok(SessionMessageSearchResults {
            matches,
            total_matches,
        })
    }

    pub(super) async fn add_message(&self, session_id: &str, message: &Message) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let metadata_json = serde_json::to_string(&message.metadata)?;

        let message_id = message
            .id
            .clone()
            .unwrap_or_else(|| format!("msg_{}_{}", session_id, uuid::Uuid::new_v4()));

        sqlx::query(
            r#"
            INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp, metadata_json)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind(message_id)
        .bind(session_id)
        .bind(role_to_string(&message.role))
        .bind(serde_json::to_string(&message.content)?)
        .bind(message.created)
        .bind(metadata_json)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE sessions SET updated_at = datetime('now') WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("UPDATE session_summaries SET status = 'stale', updated_at = datetime('now') WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        Self::discover_message_artifacts_in_tx(&mut tx, session_id, message).await?;

        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn upsert_message(&self, session_id: &str, message: &Message) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        Self::upsert_message_in_tx(&mut tx, session_id, message).await?;

        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn upsert_message_in_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
        message: &Message,
    ) -> Result<()> {
        let role = role_to_string(&message.role);
        let content_json = serde_json::to_string(&message.content)?;
        let metadata_json = serde_json::to_string(&message.metadata)?;
        let mut updated = false;

        if let Some(message_id) = message.id.as_deref() {
            let result = sqlx::query(
                r#"
                UPDATE messages
                SET role = ?, content_json = ?, created_timestamp = ?, metadata_json = ?
                WHERE session_id = ? AND message_id = ?
            "#,
            )
            .bind(role)
            .bind(&content_json)
            .bind(message.created)
            .bind(&metadata_json)
            .bind(session_id)
            .bind(message_id)
            .execute(&mut **tx)
            .await?;
            updated = result.rows_affected() > 0;
        }

        if !updated {
            let message_id = message
                .id
                .clone()
                .unwrap_or_else(|| format!("msg_{}_{}", session_id, uuid::Uuid::new_v4()));
            sqlx::query(
                r#"
                INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp, metadata_json)
                VALUES (?, ?, ?, ?, ?, ?)
            "#,
            )
            .bind(message_id)
            .bind(session_id)
            .bind(role)
            .bind(content_json)
            .bind(message.created)
            .bind(metadata_json)
            .execute(&mut **tx)
            .await?;
        }

        sqlx::query("UPDATE sessions SET updated_at = datetime('now') WHERE id = ?")
            .bind(session_id)
            .execute(&mut **tx)
            .await?;

        sqlx::query("UPDATE session_summaries SET status = 'stale', updated_at = datetime('now') WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut **tx)
            .await?;

        Self::discover_message_artifacts_in_tx(tx, session_id, message).await?;

        Ok(())
    }

    pub(super) async fn replace_conversation_in_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
        conversation: &Conversation,
    ) -> Result<()> {
        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM session_summary_facts WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM session_summaries WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut **tx)
            .await?;

        for message in conversation.messages() {
            let metadata_json = serde_json::to_string(&message.metadata)?;

            let message_id = message
                .id
                .clone()
                .unwrap_or_else(|| format!("msg_{}_{}", session_id, uuid::Uuid::new_v4()));

            sqlx::query(
                r#"
            INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp, metadata_json)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
            )
            .bind(message_id)
            .bind(session_id)
            .bind(role_to_string(&message.role))
            .bind(serde_json::to_string(&message.content)?)
            .bind(message.created)
            .bind(metadata_json)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    async fn replace_conversation_inner(
        pool: &Pool<Sqlite>,
        session_id: &str,
        conversation: &Conversation,
    ) -> Result<()> {
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        Self::replace_conversation_in_tx(&mut tx, session_id, conversation).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_conversation(
        &self,
        session_id: &str,
        conversation: &Conversation,
    ) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        Self::replace_conversation_inner(pool, session_id, conversation).await
    }

    /// Like `replace_conversation`, but also applies a `sessions` usage
    /// update in the same transaction. Compaction call sites use this
    /// instead of a separate `replace_conversation` + `record_usage` pair so
    /// a crash between the two can't leave the messages table already
    /// reflecting the compacted conversation while `sessions.total_tokens`
    /// still reflects the pre-compaction one (see `record_usage_in_tx`).
    pub async fn replace_conversation_and_record_usage(
        &self,
        session_id: &str,
        conversation: &Conversation,
        current_usage: Usage,
        accumulated_delta: Usage,
        cost_delta: Option<f64>,
    ) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        Self::replace_conversation_in_tx(&mut tx, session_id, conversation).await?;
        Self::record_usage_in_tx(
            &mut tx,
            session_id,
            current_usage,
            accumulated_delta,
            cost_delta,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn truncate_conversation(
        &self,
        session_id: &str,
        timestamp: i64,
    ) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM messages WHERE session_id = ? AND created_timestamp >= ?")
            .bind(session_id)
            .bind(timestamp)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM session_summary_facts WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM session_summaries WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn truncate_conversation_from_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let boundary = sqlx::query_as::<_, (i64, i64)>(
            "SELECT id, created_timestamp FROM messages WHERE session_id = ? AND message_id = ? ORDER BY created_timestamp, id LIMIT 1",
        )
        .bind(session_id)
        .bind(message_id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((boundary_id, boundary_timestamp)) = boundary {
            sqlx::query(
                "DELETE FROM messages WHERE session_id = ? AND (created_timestamp > ? OR (created_timestamp = ? AND id >= ?))",
            )
            .bind(session_id)
            .bind(boundary_timestamp)
            .bind(boundary_timestamp)
            .bind(boundary_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM session_summary_facts WHERE session_id = ?")
                .bind(session_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM session_summaries WHERE session_id = ?")
                .bind(session_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn update_message_metadata<F>(
        &self,
        session_id: &str,
        message_id: &str,
        f: F,
    ) -> Result<()>
    where
        F: FnOnce(
            crate::conversation::message::MessageMetadata,
        ) -> crate::conversation::message::MessageMetadata,
    {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        // No-op when the row is gone: another writer (session truncation or
        // deletion from the desktop app) may race the background tasks that
        // patch metadata, and losing that race must not abort an agent turn.
        let Some(current_metadata_json) = sqlx::query_scalar::<_, String>(
            "SELECT metadata_json FROM messages WHERE message_id = ? AND session_id = ?",
        )
        .bind(message_id)
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(());
        };

        let current_metadata: crate::conversation::message::MessageMetadata =
            serde_json::from_str(&current_metadata_json)?;

        let new_metadata = f(current_metadata);
        let metadata_json = serde_json::to_string(&new_metadata)?;

        sqlx::query(
            "UPDATE messages SET metadata_json = ? WHERE message_id = ? AND session_id = ?",
        )
        .bind(metadata_json)
        .bind(message_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Patch `tool_meta` on a specific `ToolRequest` within a stored message's
    /// `content_json`. Finds the row(s) with matching `message_id`, scans each
    /// row's content for a `ToolRequest` with the given `tool_call_id`, and
    /// merges `patch` into its `tool_meta`. Uses `BEGIN IMMEDIATE` so
    /// concurrent writers serialize correctly.
    pub(super) async fn update_tool_request_meta(
        &self,
        session_id: &str,
        message_id: &str,
        tool_call_id: &str,
        patch: serde_json::Value,
    ) -> Result<()> {
        use crate::conversation::message::MessageContent;

        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, content_json FROM messages \
             WHERE session_id = ? AND message_id = ? \
             ORDER BY id ASC",
        )
        .bind(session_id)
        .bind(message_id)
        .fetch_all(&mut *tx)
        .await?;

        for (row_id, content_json) in rows {
            let mut content: Vec<MessageContent> = serde_json::from_str(&content_json)?;
            let mut found = false;
            for block in &mut content {
                if let MessageContent::ToolRequest(tr) = block {
                    if tr.id == tool_call_id {
                        tr.tool_meta = Some(merge_tool_meta(tr.tool_meta.take(), &patch));
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                continue;
            }

            let updated_json = serde_json::to_string(&content)?;
            sqlx::query("UPDATE messages SET content_json = ? WHERE id = ?")
                .bind(updated_json)
                .bind(row_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(());
        }

        tx.commit().await?;
        Ok(())
    }
}

fn has_orphaned_tool_responses(messages: &[Message]) -> bool {
    let request_ids = messages
        .iter()
        .flat_map(|message| message.content.iter())
        .filter_map(|content| match content {
            MessageContent::ToolRequest(request) => Some(request.id.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();

    messages
        .iter()
        .flat_map(|message| message.content.iter())
        .any(|content| match content {
            MessageContent::ToolResponse(response) => !request_ids.contains(response.id.as_str()),
            _ => false,
        })
}

/// Merge a JSON object `patch` into an existing optional object value,
/// preserving keys not present in the patch.
fn merge_tool_meta(
    existing: Option<serde_json::Value>,
    patch: &serde_json::Value,
) -> serde_json::Value {
    let mut base = match existing {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    if let serde_json::Value::Object(patch_map) = patch {
        for (k, v) in patch_map {
            base.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(base)
}
