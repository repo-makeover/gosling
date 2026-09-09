//! Core session lifecycle: create, read, the dynamic-column update builder,
//! usage accounting, delete, insights, and the per-key extension-data merge.
//!
//! Extracted from `crate::session::session_manager` in a behavior-preserving
//! modularization (see `docs/logs/session/2026-08-22-modularize-session-manager.md`).
//! `pub(super)` matches these methods' pre-extraction (private, same-module)
//! visibility — the facade's `impl SessionManager` delegates to all of them,
//! and `create_session_in_tx`/`apply_update_in_tx` are also called from the
//! session_transfer submodule's `import_session`/`copy_session` (both are
//! `session_manager` descendants, so `pub(super)` covers those calls too).

use super::{
    message_timestamp_to_datetime, normalized_message_timestamp_sql, Session, SessionInsights,
    SessionStorage, SessionType, SessionUpdateBuilder,
};
use crate::config::GoslingMode;
use crate::session::extension_data::ExtensionData;
use anyhow::Result;
use gosling_providers::conversation::token_usage::Usage;
use sqlx::Sqlite;
use std::path::PathBuf;

impl SessionStorage {
    pub(super) async fn create_session(
        &self,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
        gosling_mode: GoslingMode,
    ) -> Result<Session> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let session =
            Self::create_session_in_tx(&mut tx, working_dir, name, session_type, gosling_mode)
                .await?;
        tx.commit().await?;
        #[cfg(feature = "telemetry")]
        crate::posthog::emit_session_started();
        Ok(session)
    }

    /// Same insert as `create_session`, against a caller-owned transaction so
    /// multi-step operations (import, copy) can commit session creation
    /// together with their follow-up writes instead of each being its own
    /// independently committed transaction.
    pub(super) async fn create_session_in_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
        gosling_mode: GoslingMode,
    ) -> Result<Session> {
        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        let session = sqlx::query_as(
            r#"
                INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data, gosling_mode)
                VALUES (
                    ? || '_' || CAST(COALESCE((
                        SELECT MAX(CAST(SUBSTR(id, 10) AS INTEGER))
                        FROM sessions
                        WHERE id LIKE ? || '_%'
                    ), 0) + 1 AS TEXT),
                    ?,
                    FALSE,
                    ?,
                    ?,
                    '{}',
                    ?
                )
                RETURNING *
                "#,
        )
            .bind(&today)
            .bind(&today)
            .bind(&name)
            .bind(session_type.to_string())
            .bind(&*working_dir.to_string_lossy())
            .bind(gosling_mode.to_string())
            .fetch_one(&mut **tx)
            .await?;

        Ok(session)
    }

    pub(super) async fn get_session(&self, id: &str, include_messages: bool) -> Result<Session> {
        let pool = self.pool().await?;
        let mut session = sqlx::query_as::<_, Session>(
            r#"
        SELECT id, working_dir, additional_working_dirs_json, restrict_tools_to_working_dirs, name, description, user_set_name, session_type, created_at, updated_at, extension_data,
               total_tokens, input_tokens, output_tokens,
               cache_read_tokens, cache_write_tokens,
               accumulated_total_tokens, accumulated_input_tokens, accumulated_output_tokens,
               accumulated_cache_read_tokens, accumulated_cache_write_tokens,
               accumulated_cost,
               provider_name, model_config_json, gosling_mode,
               archived_at, project_id, workspace_id, workspace_name,
               credential_profile_id, credential_profile_name, credential_binding_id,
               workspace_context_json
        FROM sessions
        WHERE id = ?
    "#,
        )
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

        if include_messages {
            let conv = self.get_conversation(&session.id).await?;
            session.message_count = conv.messages().len();
            session.last_message_at = conv
                .messages()
                .iter()
                .filter_map(|message| message_timestamp_to_datetime(message.created))
                .max();
            session.conversation = Some(conv);
        } else {
            let sql = format!(
                "SELECT COUNT(*), MAX({}) FROM messages WHERE session_id = ?",
                normalized_message_timestamp_sql("created_timestamp")
            );
            let (count, last_message_timestamp): (i64, Option<i64>) = sqlx::query_as(&sql)
                .bind(&session.id)
                .fetch_one(pool)
                .await?;
            session.message_count = count as usize;
            session.last_message_at =
                last_message_timestamp.and_then(message_timestamp_to_datetime);
        }

        Ok(session)
    }

    pub(super) async fn apply_update(&self, builder: SessionUpdateBuilder<'_>) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        Self::apply_update_in_tx(&mut tx, builder).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Same guarded UPDATE as `apply_update`, against a caller-owned
    /// transaction so `import_session`/`copy_session` can commit it together
    /// with session creation and conversation replacement instead of each
    /// being its own independently committed transaction. Never commits or
    /// rolls back itself — that's the caller's decision.
    #[allow(clippy::too_many_lines)]
    pub(super) async fn apply_update_in_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        builder: SessionUpdateBuilder<'_>,
    ) -> Result<()> {
        let mut updates = Vec::new();
        let mut query = String::from("UPDATE sessions SET ");

        macro_rules! add_update {
            ($field:expr, $name:expr) => {
                if $field.is_some() {
                    if !updates.is_empty() {
                        query.push_str(", ");
                    }
                    updates.push($name);
                    query.push_str($name);
                    query.push_str(" = ?");
                }
            };
        }

        add_update!(builder.name, "name");
        add_update!(builder.user_set_name, "user_set_name");
        add_update!(builder.session_type, "session_type");
        add_update!(builder.working_dir, "working_dir");
        add_update!(
            builder.additional_working_dirs,
            "additional_working_dirs_json"
        );
        add_update!(
            builder.restrict_tools_to_working_dirs,
            "restrict_tools_to_working_dirs"
        );
        add_update!(builder.extension_data, "extension_data");
        add_update!(builder.usage, "total_tokens");
        add_update!(builder.usage, "input_tokens");
        add_update!(builder.usage, "output_tokens");
        add_update!(builder.usage, "cache_read_tokens");
        add_update!(builder.usage, "cache_write_tokens");
        add_update!(builder.accumulated_usage, "accumulated_total_tokens");
        add_update!(builder.accumulated_usage, "accumulated_input_tokens");
        add_update!(builder.accumulated_usage, "accumulated_output_tokens");
        add_update!(builder.accumulated_usage, "accumulated_cache_read_tokens");
        add_update!(builder.accumulated_usage, "accumulated_cache_write_tokens");
        add_update!(builder.accumulated_cost, "accumulated_cost");
        add_update!(builder.provider_name, "provider_name");
        add_update!(builder.model_config, "model_config_json");
        add_update!(builder.workspace_id, "workspace_id");
        add_update!(builder.workspace_name, "workspace_name");
        add_update!(builder.credential_profile_id, "credential_profile_id");
        add_update!(builder.credential_profile_name, "credential_profile_name");
        add_update!(builder.credential_binding_id, "credential_binding_id");
        add_update!(builder.workspace_context, "workspace_context_json");
        add_update!(builder.gosling_mode, "gosling_mode");
        add_update!(builder.archived_at, "archived_at");

        add_update!(builder.project_id, "project_id");

        if updates.is_empty() {
            return Ok(());
        }

        let guard_on_user_set_name = builder.only_if_not_user_named;
        query.push_str(", ");
        query.push_str("updated_at = datetime('now') WHERE id = ?");
        if guard_on_user_set_name {
            query.push_str(" AND user_set_name = 0");
        }

        let mut q = sqlx::query(&query);

        if let Some(name) = builder.name {
            q = q.bind(name);
        }
        if let Some(user_set_name) = builder.user_set_name {
            q = q.bind(user_set_name);
        }
        if let Some(session_type) = builder.session_type {
            q = q.bind(session_type.to_string());
        }
        if let Some(wd) = builder.working_dir {
            q = q.bind(wd.to_string_lossy().to_string());
        }
        if let Some(additional_working_dirs) = builder.additional_working_dirs {
            q = q.bind(serde_json::to_string(&additional_working_dirs)?);
        }
        if let Some(restrict) = builder.restrict_tools_to_working_dirs {
            q = q.bind(restrict);
        }
        if let Some(ed) = builder.extension_data {
            q = q.bind(serde_json::to_string(&ed)?);
        }
        if let Some(u) = builder.usage {
            q = q
                .bind(u.total_tokens)
                .bind(u.input_tokens)
                .bind(u.output_tokens)
                .bind(u.cache_read_input_tokens)
                .bind(u.cache_write_input_tokens);
        }
        if let Some(u) = builder.accumulated_usage {
            q = q
                .bind(u.total_tokens)
                .bind(u.input_tokens)
                .bind(u.output_tokens)
                .bind(u.cache_read_input_tokens)
                .bind(u.cache_write_input_tokens);
        }
        if let Some(ac) = builder.accumulated_cost {
            q = q.bind(ac);
        }
        if let Some(provider_name) = builder.provider_name {
            q = q.bind(provider_name);
        }
        if let Some(model_config) = builder.model_config {
            let model_config_json = model_config
                .map(|mc| serde_json::to_string(&mc))
                .transpose()?;
            q = q.bind(model_config_json);
        }
        if let Some(workspace_id) = builder.workspace_id {
            q = q.bind(workspace_id);
        }
        if let Some(workspace_name) = builder.workspace_name {
            q = q.bind(workspace_name);
        }
        if let Some(profile_id) = builder.credential_profile_id {
            q = q.bind(profile_id);
        }
        if let Some(profile_name) = builder.credential_profile_name {
            q = q.bind(profile_name);
        }
        if let Some(binding_id) = builder.credential_binding_id {
            q = q.bind(binding_id);
        }
        if let Some(context) = builder.workspace_context {
            q = q.bind(
                context
                    .map(|value| serde_json::to_string(&value))
                    .transpose()?,
            );
        }
        if let Some(gosling_mode) = builder.gosling_mode {
            q = q.bind(gosling_mode.to_string());
        }
        if let Some(ref archived_at) = builder.archived_at {
            q = q.bind(archived_at.as_ref());
        }

        if let Some(ref project_id) = builder.project_id {
            q = q.bind(project_id.as_ref());
        }

        q = q.bind(&builder.session_id);
        let result = q.execute(&mut **tx).await?;

        if result.rows_affected() == 0 {
            if guard_on_user_set_name {
                // The guarded UPDATE matched zero rows because either the
                // session doesn't exist, or it does but user_set_name was
                // already true - i.e. the user renamed it while this
                // background auto-naming write was in flight. Only the
                // first case is a real error; distinguish them so a lost
                // race silently drops the stale write instead of clobbering
                // (or being reported as failing to touch) the user's rename.
                let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM sessions WHERE id = ?")
                    .bind(&builder.session_id)
                    .fetch_optional(&mut **tx)
                    .await?;
                if exists.is_some() {
                    return Ok(());
                }
                return Err(anyhow::anyhow!("Session not found: {}", builder.session_id));
            }
            return Err(anyhow::anyhow!("Session not found: {}", builder.session_id));
        }

        Ok(())
    }

    pub(super) async fn record_usage(
        &self,
        session_id: &str,
        current_usage: Usage,
        accumulated_delta: Usage,
        cost_delta: Option<f64>,
    ) -> Result<()> {
        let pool = self.pool().await?;
        let result = sqlx::query(
            r#"
            UPDATE sessions
            SET total_tokens = ?,
                input_tokens = ?,
                output_tokens = ?,
                cache_read_tokens = ?,
                cache_write_tokens = ?,
                accumulated_total_tokens = CASE WHEN ? IS NULL THEN accumulated_total_tokens ELSE COALESCE(accumulated_total_tokens, 0) + ? END,
                accumulated_input_tokens = CASE WHEN ? IS NULL THEN accumulated_input_tokens ELSE COALESCE(accumulated_input_tokens, 0) + ? END,
                accumulated_output_tokens = CASE WHEN ? IS NULL THEN accumulated_output_tokens ELSE COALESCE(accumulated_output_tokens, 0) + ? END,
                accumulated_cache_read_tokens = CASE WHEN ? IS NULL THEN accumulated_cache_read_tokens ELSE COALESCE(accumulated_cache_read_tokens, 0) + ? END,
                accumulated_cache_write_tokens = CASE WHEN ? IS NULL THEN accumulated_cache_write_tokens ELSE COALESCE(accumulated_cache_write_tokens, 0) + ? END,
                accumulated_cost = CASE WHEN ? IS NULL THEN accumulated_cost ELSE COALESCE(accumulated_cost, 0) + ? END,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(current_usage.total_tokens)
        .bind(current_usage.input_tokens)
        .bind(current_usage.output_tokens)
        .bind(current_usage.cache_read_input_tokens)
        .bind(current_usage.cache_write_input_tokens)
        .bind(accumulated_delta.total_tokens)
        .bind(accumulated_delta.total_tokens)
        .bind(accumulated_delta.input_tokens)
        .bind(accumulated_delta.input_tokens)
        .bind(accumulated_delta.output_tokens)
        .bind(accumulated_delta.output_tokens)
        .bind(accumulated_delta.cache_read_input_tokens)
        .bind(accumulated_delta.cache_read_input_tokens)
        .bind(accumulated_delta.cache_write_input_tokens)
        .bind(accumulated_delta.cache_write_input_tokens)
        .bind(cost_delta)
        .bind(cost_delta)
        .bind(session_id)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("Session not found: {session_id}");
        }
        Ok(())
    }

    /// Same statement as `record_usage`, run against an existing transaction
    /// instead of `pool` directly. Used by `MessageStorage::replace_conversation_and_record_usage`
    /// so a compaction's message replacement and usage update commit or roll
    /// back together — a crash between two separate commits used to leave
    /// `sessions.total_tokens` reflecting the pre-compaction conversation
    /// while the messages table already held the post-compaction one, which
    /// could spuriously re-trigger auto-compaction on the next turn (the
    /// stale-high stored value wins `resolve_context_usage`'s
    /// `max(stored, estimated)` undercounting guard). Keep this SQL in sync
    /// with `record_usage` above.
    pub(super) async fn record_usage_in_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
        current_usage: Usage,
        accumulated_delta: Usage,
        cost_delta: Option<f64>,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE sessions
            SET total_tokens = ?,
                input_tokens = ?,
                output_tokens = ?,
                cache_read_tokens = ?,
                cache_write_tokens = ?,
                accumulated_total_tokens = CASE WHEN ? IS NULL THEN accumulated_total_tokens ELSE COALESCE(accumulated_total_tokens, 0) + ? END,
                accumulated_input_tokens = CASE WHEN ? IS NULL THEN accumulated_input_tokens ELSE COALESCE(accumulated_input_tokens, 0) + ? END,
                accumulated_output_tokens = CASE WHEN ? IS NULL THEN accumulated_output_tokens ELSE COALESCE(accumulated_output_tokens, 0) + ? END,
                accumulated_cache_read_tokens = CASE WHEN ? IS NULL THEN accumulated_cache_read_tokens ELSE COALESCE(accumulated_cache_read_tokens, 0) + ? END,
                accumulated_cache_write_tokens = CASE WHEN ? IS NULL THEN accumulated_cache_write_tokens ELSE COALESCE(accumulated_cache_write_tokens, 0) + ? END,
                accumulated_cost = CASE WHEN ? IS NULL THEN accumulated_cost ELSE COALESCE(accumulated_cost, 0) + ? END,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(current_usage.total_tokens)
        .bind(current_usage.input_tokens)
        .bind(current_usage.output_tokens)
        .bind(current_usage.cache_read_input_tokens)
        .bind(current_usage.cache_write_input_tokens)
        .bind(accumulated_delta.total_tokens)
        .bind(accumulated_delta.total_tokens)
        .bind(accumulated_delta.input_tokens)
        .bind(accumulated_delta.input_tokens)
        .bind(accumulated_delta.output_tokens)
        .bind(accumulated_delta.output_tokens)
        .bind(accumulated_delta.cache_read_input_tokens)
        .bind(accumulated_delta.cache_read_input_tokens)
        .bind(accumulated_delta.cache_write_input_tokens)
        .bind(accumulated_delta.cache_write_input_tokens)
        .bind(cost_delta)
        .bind(cost_delta)
        .bind(session_id)
        .execute(&mut **tx)
        .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("Session not found: {session_id}");
        }
        Ok(())
    }

    pub(super) async fn delete_session(&self, session_id: &str) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?)")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;

        if !exists {
            return Err(anyhow::anyhow!("Session not found"));
        }

        sqlx::query("DELETE FROM session_summary_facts WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM session_summaries WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM session_library_items WHERE scope = 'session' AND scope_key = ?")
            .bind(format!("session:{session_id}"))
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn get_insights(&self, types: &[SessionType]) -> Result<SessionInsights> {
        if types.is_empty() {
            return Ok(SessionInsights {
                total_sessions: 0,
                total_tokens: 0,
            });
        }

        let placeholders: String = types.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let query = format!(
            r#"
            SELECT COUNT(*) as total_sessions,
                   COALESCE(SUM(COALESCE(accumulated_total_tokens, total_tokens, 0)), 0) as total_tokens
            FROM sessions
            WHERE session_type IN ({})
            "#,
            placeholders
        );

        let pool = self.pool().await?;
        let mut q = sqlx::query_as::<_, (i64, Option<i64>)>(&query);
        for t in types {
            q = q.bind(t.to_string());
        }

        let row = q.fetch_one(pool).await?;

        Ok(SessionInsights {
            total_sessions: row.0 as usize,
            total_tokens: row.1.unwrap_or(0),
        })
    }

    /// Read-modify-write `extension_data` for one key inside a single
    /// `BEGIN IMMEDIATE` transaction. `BEGIN IMMEDIATE` takes SQLite's write
    /// lock before the read, so a second concurrent caller of this method
    /// (or of any other writer using the same transaction pattern) blocks
    /// until this one commits, then reads the up-to-date row — the two
    /// writes can never interleave and silently drop one side's key.
    pub(super) async fn merge_extension_state(
        &self,
        session_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let row: Option<(String,)> =
            sqlx::query_as("SELECT extension_data FROM sessions WHERE id = ?")
                .bind(session_id)
                .fetch_optional(&mut *tx)
                .await?;

        let Some((extension_data_json,)) = row else {
            tx.commit().await?;
            return Err(anyhow::anyhow!("Session not found: {}", session_id));
        };

        let mut extension_data: ExtensionData =
            serde_json::from_str(&extension_data_json).unwrap_or_default();
        extension_data
            .extension_states
            .insert(key.to_string(), value);

        sqlx::query(
            "UPDATE sessions SET extension_data = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(serde_json::to_string(&extension_data)?)
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}
