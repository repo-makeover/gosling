//! Session-store schema DDL: table/index/trigger definitions for the
//! `sessions.db` SQLite store, plus the one-time legacy-artifact backfill
//! run at migration v26.
//!
//! Extracted from `crate::session::session_manager` in a behavior-preserving
//! modularization (see `docs/logs/session/2026-08-22-modularize-session-manager.md`).
//! These are `impl SessionStorage` associated functions physically relocated
//! here; the facade re-exports nothing separately for this module — callers
//! keep calling `SessionStorage::create_schema(...)` etc. exactly as before,
//! since Rust resolves inherent `impl` blocks by type, not by file.

use super::SessionStorage;
use crate::conversation::message::MessageContent;
use crate::session::artifacts::{
    assistant_reference_bases, discover_from_assistant_markdown, discover_from_successful_tool,
    SessionArtifactProvenance,
};
use anyhow::Result;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::path::Path;

use super::CURRENT_SCHEMA_VERSION;

impl SessionStorage {
    pub(super) async fn create_schema(pool: &Pool<Sqlite>) -> Result<()> {
        // Run schema creation under `BEGIN IMMEDIATE` so SQLite serializes
        // writers across processes. Combined with `IF NOT EXISTS` on every
        // DDL statement and `INSERT OR IGNORE` on the bootstrap version
        // row, this makes init safe under concurrent first-run startup —
        // the previous flow:
        //
        //   SELECT EXISTS('schema_version') → false
        //   CREATE TABLE schema_version (...)
        //
        // raced when two processes both saw "doesn't exist" and the
        // second one's CREATE TABLE failed with `table already exists`,
        // which surfaced to callers as "Could not create session".
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("INSERT OR IGNORE INTO schema_version (version) VALUES (?)")
            .bind(CURRENT_SCHEMA_VERSION)
            .execute(&mut *tx)
            .await?;

        // Left unmarked here (see `legacy_import_completed`/`mark_legacy_import_complete`)
        // so the caller in `pool()` knows to actually run `import_legacy` for a
        // fresh database, and can retry it if the process is interrupted
        // before the import finishes.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS legacy_import_status (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                completed_at TIMESTAMP
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                user_set_name BOOLEAN DEFAULT FALSE,
                session_type TEXT NOT NULL DEFAULT 'user',
                working_dir TEXT NOT NULL,
                additional_working_dirs_json TEXT NOT NULL DEFAULT '[]',
                restrict_tools_to_working_dirs BOOLEAN NOT NULL DEFAULT FALSE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                extension_data TEXT DEFAULT '{}',
                total_tokens INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                cache_read_tokens INTEGER,
                cache_write_tokens INTEGER,
                accumulated_total_tokens INTEGER,
                accumulated_input_tokens INTEGER,
                accumulated_output_tokens INTEGER,
                accumulated_cache_read_tokens INTEGER,
                accumulated_cache_write_tokens INTEGER,
                accumulated_cost REAL,
                schedule_id TEXT,
                recipe_json TEXT,
                user_recipe_values_json TEXT,
                provider_name TEXT,
                model_config_json TEXT,
                gosling_mode TEXT NOT NULL DEFAULT 'auto',
                archived_at TIMESTAMP,
                project_id TEXT
                ,workspace_id TEXT
                ,workspace_name TEXT
                ,credential_profile_id TEXT
                ,credential_profile_name TEXT
                ,credential_binding_id TEXT
                ,workspace_context_json TEXT
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id TEXT,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                role TEXT NOT NULL,
                content_json TEXT NOT NULL,
                created_timestamp INTEGER NOT NULL,
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                tokens INTEGER,
                metadata_json TEXT
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_message_id ON messages(message_id)")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_messages_session_row_desc ON messages(session_id, id DESC)",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_messages_session_time_asc ON messages(session_id, created_timestamp, id)",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_type ON sessions(session_type)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_workspace ON sessions(workspace_id)")
            .execute(&mut *tx)
            .await?;

        Self::create_message_search_schema(&mut tx).await?;

        Self::create_tool_operations_schema(&mut tx).await?;

        Self::create_session_artifacts_schema(&mut tx).await?;
        Self::create_output_revisions_schema(&mut tx).await?;

        Self::create_session_library_schema(&mut tx).await?;

        Self::create_session_turn_lease_schema(&mut tx).await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_summaries (
                session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
                summary TEXT NOT NULL DEFAULT '',
                covered_through_row_id INTEGER NOT NULL DEFAULT 0,
                covered_through_timestamp INTEGER NOT NULL DEFAULT 0,
                covered_message_count INTEGER NOT NULL DEFAULT 0,
                source_hash TEXT NOT NULL DEFAULT '',
                summarizer_model TEXT,
                status TEXT NOT NULL DEFAULT 'stale',
                error TEXT,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_summary_facts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                project_id TEXT,
                working_dir TEXT NOT NULL,
                scope TEXT NOT NULL DEFAULT 'session',
                fact_type TEXT NOT NULL,
                content TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0,
                source_start_row_id INTEGER,
                source_end_row_id INTEGER,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_session_summary_facts_session ON session_summary_facts(session_id)",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_session_summary_facts_project ON session_summary_facts(project_id, scope)",
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // The inventory tables already use `CREATE TABLE IF NOT EXISTS`
        // and run on the shared pool, so they don't need to be inside
        // the same transaction.
        crate::providers::inventory::create_tables(pool).await?;

        Ok(())
    }
    /// A rebuildable full-text projection of text-only message content.
    ///
    /// It is intentionally independent from the conversation source of truth:
    /// `messages` remains durable, while this index can always be recreated
    /// from `content_json`. The triggers make writes and edits visible to
    /// recall immediately, without paying the JSON scan cost on every search.
    pub(super) async fn create_message_search_schema(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS message_search USING fts5(
                text,
                session_id UNINDEXED,
                message_id UNINDEXED,
                role UNINDEXED,
                tokenize = 'unicode61 remove_diacritics 2'
            )
            "#,
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS messages_search_after_insert
            AFTER INSERT ON messages BEGIN
                INSERT INTO message_search(rowid, text, session_id, message_id, role)
                SELECT
                    NEW.id,
                    COALESCE((
                        SELECT group_concat(json_extract(value, '$.text'), ' ')
                        FROM json_each(NEW.content_json)
                        WHERE json_extract(value, '$.type') = 'text'
                    ), ''),
                    NEW.session_id,
                    COALESCE(NEW.message_id, ''),
                    NEW.role;
            END
            "#,
        )
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS messages_search_after_delete
            AFTER DELETE ON messages BEGIN
                DELETE FROM message_search WHERE rowid = OLD.id;
            END
            "#,
        )
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS messages_search_after_update
            AFTER UPDATE OF content_json, session_id, message_id, role ON messages BEGIN
                DELETE FROM message_search WHERE rowid = OLD.id;
                INSERT INTO message_search(rowid, text, session_id, message_id, role)
                SELECT
                    NEW.id,
                    COALESCE((
                        SELECT group_concat(json_extract(value, '$.text'), ' ')
                        FROM json_each(NEW.content_json)
                        WHERE json_extract(value, '$.type') = 'text'
                    ), ''),
                    NEW.session_id,
                    COALESCE(NEW.message_id, ''),
                    NEW.role;
            END
            "#,
        )
        .execute(&mut **tx)
        .await?;

        // `INSERT OR IGNORE` keeps this idempotent for freshly-created
        // databases, where the insert trigger has already indexed any rows.
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO message_search(rowid, text, session_id, message_id, role)
            SELECT
                m.id,
                COALESCE((
                    SELECT group_concat(json_extract(value, '$.text'), ' ')
                    FROM json_each(m.content_json)
                    WHERE json_extract(value, '$.type') = 'text'
                ), ''),
                m.session_id,
                COALESCE(m.message_id, ''),
                m.role
            FROM messages m
            "#,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
    pub(super) async fn create_tool_operations_schema(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tool_operations (
                operation_id TEXT PRIMARY KEY,
                schema_version INTEGER NOT NULL DEFAULT 1 CHECK(schema_version = 1),
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                tool_request_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                request_digest TEXT NOT NULL,
                conversation_bound BOOLEAN NOT NULL DEFAULT FALSE,
                owner_id TEXT NOT NULL,
                owner_pid INTEGER,
                state TEXT NOT NULL CHECK(state IN ('started', 'completed', 'in_doubt')),
                result_json TEXT,
                response_message_id TEXT,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(session_id, tool_request_id)
            )
            "#,
        )
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tool_operations_session_state ON tool_operations(session_id, state)",
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
    pub(super) async fn create_session_artifacts_schema(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_artifacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                display_path TEXT NOT NULL,
                resolved_path TEXT NOT NULL,
                base_working_dir TEXT NOT NULL,
                workspace_id TEXT,
                mime_type TEXT,
                relation TEXT NOT NULL CHECK(relation IN ('created', 'modified', 'referenced')),
                provenance TEXT NOT NULL CHECK(provenance IN ('built_in_tool', 'mcp_resource_link', 'tool_metadata', 'tool_argument', 'assistant_message', 'compatibility_inference')),
                source_id TEXT,
                first_seen_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                last_seen_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(session_id, resolved_path)
            )
            "#,
        )
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_session_artifacts_session_seen ON session_artifacts(session_id, last_seen_at DESC, id DESC)",
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
    pub(super) async fn create_session_library_schema(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_library_items (
                id TEXT PRIMARY KEY,
                scope TEXT NOT NULL CHECK(scope IN ('project', 'session')),
                scope_key TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('text', 'image', 'file')),
                mime_type TEXT NOT NULL,
                size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
                text_content TEXT,
                image_data TEXT,
                file_path TEXT,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                CHECK(
                    (kind = 'text' AND text_content IS NOT NULL AND image_data IS NULL AND file_path IS NULL)
                    OR (kind = 'image' AND text_content IS NULL AND image_data IS NOT NULL AND file_path IS NULL)
                    OR (kind = 'file' AND text_content IS NULL AND image_data IS NULL AND file_path IS NOT NULL)
                )
            )
            "#,
        )
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_session_library_scope ON session_library_items(scope, scope_key, created_at DESC, id DESC)",
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub(super) async fn create_session_turn_lease_schema(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_turn_leases (
                session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
                lease_id TEXT NOT NULL UNIQUE,
                owner_id TEXT NOT NULL,
                owner_pid INTEGER NOT NULL,
                acquired_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
    pub(super) async fn backfill_session_artifacts(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> Result<()> {
        let sessions = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
            "SELECT id, working_dir, additional_working_dirs_json, extension_data, workspace_id FROM sessions",
        )
        .fetch_all(&mut **tx)
        .await?;
        for (session_id, working_dir, additional_dirs_json, extension_data_json, workspace_id) in
            sessions
        {
            let additional_dirs = assistant_reference_bases(
                serde_json::from_str(&additional_dirs_json).unwrap_or_default(),
                &serde_json::from_str(&extension_data_json).unwrap_or_default(),
            );
            let messages = sqlx::query_as::<_, (Option<String>, String, String, Option<String>)>(
                "SELECT message_id, role, content_json, metadata_json FROM messages WHERE session_id = ? ORDER BY id",
            )
            .bind(&session_id)
            .fetch_all(&mut **tx)
            .await?;
            let mut requests = HashMap::new();
            for (message_id, role, content_json, metadata_json) in messages {
                let metadata: crate::conversation::message::MessageMetadata = metadata_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()?
                    .unwrap_or_default();
                let contents: Vec<MessageContent> = serde_json::from_str(&content_json)?;
                for content in contents {
                    match content {
                        MessageContent::ToolRequest(request) => {
                            requests.insert(request.id.clone(), request);
                        }
                        MessageContent::ToolResponse(response) => {
                            if let (Some(request), Ok(result)) =
                                (requests.get(&response.id), response.tool_result.as_ref())
                            {
                                if let Ok(tool_call) = request.tool_call.as_ref() {
                                    let artifacts = discover_from_successful_tool(
                                        tool_call,
                                        result,
                                        Path::new(&working_dir),
                                        workspace_id.as_deref(),
                                        message_id.as_deref().or(Some(response.id.as_str())),
                                    );
                                    Self::upsert_artifacts_in_tx(tx, &session_id, &artifacts)
                                        .await?;
                                }
                            }
                        }
                        MessageContent::Text(text)
                            if role == "assistant" && !metadata.imported_untrusted =>
                        {
                            let artifacts = discover_from_assistant_markdown(
                                &text.text,
                                Path::new(&working_dir),
                                &additional_dirs,
                                workspace_id.as_deref(),
                                message_id.as_deref(),
                                SessionArtifactProvenance::CompatibilityInference,
                            );
                            Self::upsert_artifacts_in_tx(tx, &session_id, &artifacts).await?;
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}
