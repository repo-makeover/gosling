//! The full `sessions.db` schema migration ladder (v1..CURRENT_SCHEMA_VERSION)
//! plus the version-tracking helpers that drive it.
//!
//! Extracted from `crate::session::session_manager` in a behavior-preserving
//! modularization (see `docs/logs/session/2026-08-22-modularize-session-manager.md`).
//! Kept as one file rather than split further: `apply_migration` is a single
//! ordered match over schema versions, and splitting a version history
//! across files would make it harder, not easier, to audit — a deliberate
//! exception to this run's ~400-line seam-size guideline. `run_migrations`
//! is `pub(super)` because the facade's pool-init path (`SessionStorage::pool`)
//! calls it directly; the rest are private to this module.

use super::{SessionStorage, CURRENT_SCHEMA_VERSION};
use anyhow::Result;
use sqlx::{Pool, Sqlite};
use tracing::info;

impl SessionStorage {
    pub(super) async fn run_migrations(pool: &Pool<Sqlite>) -> Result<()> {
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let current_version = Self::get_schema_version(&mut tx).await?;

        if current_version < CURRENT_SCHEMA_VERSION {
            info!(
                "Running database migrations from v{} to v{}...",
                current_version, CURRENT_SCHEMA_VERSION
            );

            for version in (current_version + 1)..=CURRENT_SCHEMA_VERSION {
                info!("  Applying migration v{}...", version);
                Self::apply_migration(&mut tx, version).await?;
                Self::update_schema_version(&mut tx, version).await?;
                info!("  ✓ Migration v{} complete", version);
            }

            info!("All migrations complete");
        }

        tx.commit().await?;
        Ok(())
    }

    async fn get_schema_version(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<i32> {
        let table_exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT name FROM sqlite_master
                WHERE type='table' AND name='schema_version'
            )
        "#,
        )
        .fetch_one(&mut **tx)
        .await?;

        if !table_exists {
            return Ok(0);
        }

        let version = sqlx::query_scalar::<_, i32>("SELECT MAX(version) FROM schema_version")
            .fetch_one(&mut **tx)
            .await?;

        Ok(version)
    }

    async fn update_schema_version(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        version: i32,
    ) -> Result<()> {
        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(version)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub(super) async fn apply_migration(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        version: i32,
    ) -> Result<()> {
        match version {
            1 => {
                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS schema_version (
                        version INTEGER PRIMARY KEY,
                        applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                    )
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            2 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN user_recipe_values_json TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            3 => {
                sqlx::query(
                    r#"
                    ALTER TABLE messages ADD COLUMN metadata_json TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            4 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN name TEXT DEFAULT ''
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN user_set_name BOOLEAN DEFAULT FALSE
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            5 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN session_type TEXT NOT NULL DEFAULT 'user'
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query("CREATE INDEX idx_sessions_type ON sessions(session_type)")
                    .execute(&mut **tx)
                    .await?;
            }
            6 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN provider_name TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN model_config_json TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            7 => {
                sqlx::query(
                    r#"
                    ALTER TABLE messages ADD COLUMN message_id TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    r#"
                    UPDATE messages
                    SET message_id = 'msg_' || session_id || '_' || id
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query("CREATE INDEX idx_messages_message_id ON messages(message_id)")
                    .execute(&mut **tx)
                    .await?;
            }
            8 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN gosling_mode TEXT NOT NULL DEFAULT 'auto'
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            9 => {
                sqlx::query(
                    r#"
                    UPDATE sessions
                    SET session_type = 'acp'
                    WHERE session_type = 'user'
                      AND name = 'ACP Session'
                      AND user_set_name = FALSE
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            10 => {
                // Check if thread_id column already exists (e.g. fresh schema)
                let has_thread_id = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'thread_id'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if !has_thread_id {
                    sqlx::query("ALTER TABLE sessions ADD COLUMN thread_id TEXT")
                        .execute(&mut **tx)
                        .await?;
                }
                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS idx_sessions_thread ON sessions(thread_id)",
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS threads (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL DEFAULT 'New Chat',
                        user_set_name BOOLEAN DEFAULT FALSE,
                        working_dir TEXT,
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                        archived_at TIMESTAMP,
                        metadata_json TEXT DEFAULT '{}'
                    )",
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS thread_messages (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        thread_id TEXT NOT NULL REFERENCES threads(id),
                        session_id TEXT,
                        message_id TEXT,
                        role TEXT NOT NULL,
                        content_json TEXT NOT NULL,
                        created_timestamp INTEGER NOT NULL,
                        metadata_json TEXT DEFAULT '{}'
                    )",
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query("CREATE INDEX IF NOT EXISTS idx_thread_messages_thread ON thread_messages(thread_id)")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("CREATE INDEX IF NOT EXISTS idx_thread_messages_message_id ON thread_messages(message_id)")
                    .execute(&mut **tx)
                    .await?;
            }
            11 => {
                crate::providers::inventory::create_tables_in_tx(tx).await?;
            }
            12 => {
                // Add archived_at, project_id columns to sessions.
                let has_archived_at = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'archived_at'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if !has_archived_at {
                    sqlx::query("ALTER TABLE sessions ADD COLUMN archived_at TIMESTAMP")
                        .execute(&mut **tx)
                        .await?;
                }

                let has_project_id = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'project_id'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if !has_project_id {
                    sqlx::query("ALTER TABLE sessions ADD COLUMN project_id TEXT")
                        .execute(&mut **tx)
                        .await?;
                }
            }
            13 => {
                let has_accumulated_cost = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'accumulated_cost'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if !has_accumulated_cost {
                    sqlx::query("ALTER TABLE sessions ADD COLUMN accumulated_cost REAL")
                        .execute(&mut **tx)
                        .await?;
                }
            }
            14 => {
                for column in [
                    "cache_read_tokens",
                    "cache_write_tokens",
                    "accumulated_cache_read_tokens",
                    "accumulated_cache_write_tokens",
                ] {
                    let has_column = sqlx::query_scalar::<_, i32>(
                        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = ?",
                    )
                    .bind(column)
                    .fetch_one(&mut **tx)
                    .await?
                        > 0;
                    if !has_column {
                        sqlx::query(&format!("ALTER TABLE sessions ADD COLUMN {column} INTEGER"))
                            .execute(&mut **tx)
                            .await?;
                    }
                }
            }
            15 => {
                let has_goose_mode = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'goose_mode'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if has_goose_mode {
                    sqlx::query("ALTER TABLE sessions RENAME COLUMN goose_mode TO gosling_mode")
                        .execute(&mut **tx)
                        .await?;
                }
            }
            16 => {
                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS idx_messages_session_row_desc ON messages(session_id, id DESC)",
                )
                .execute(&mut **tx)
                .await?;

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
                .execute(&mut **tx)
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
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS idx_session_summary_facts_session ON session_summary_facts(session_id)",
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS idx_session_summary_facts_project ON session_summary_facts(project_id, scope)",
                )
                .execute(&mut **tx)
                .await?;
            }
            17 => {
                let has_additional_working_dirs = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'additional_working_dirs_json'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if !has_additional_working_dirs {
                    sqlx::query(
                        "ALTER TABLE sessions ADD COLUMN additional_working_dirs_json TEXT NOT NULL DEFAULT '[]'",
                    )
                    .execute(&mut **tx)
                    .await?;
                }
            }
            18 => {
                let has_restrict_flag = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'restrict_tools_to_working_dirs'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if !has_restrict_flag {
                    sqlx::query(
                        "ALTER TABLE sessions ADD COLUMN restrict_tools_to_working_dirs BOOLEAN NOT NULL DEFAULT FALSE",
                    )
                    .execute(&mut **tx)
                    .await?;
                }
            }
            19 => {
                let has_workflow_kind = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'workflow_kind'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if !has_workflow_kind {
                    sqlx::query(
                        "ALTER TABLE sessions ADD COLUMN workflow_kind TEXT NOT NULL DEFAULT 'standard'",
                    )
                    .execute(&mut **tx)
                    .await?;
                }
                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS tagteam_run_bindings (
                        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                        launch_generation INTEGER NOT NULL,
                        schema_version INTEGER NOT NULL,
                        launch_spec_json TEXT NOT NULL,
                        action_digest TEXT NOT NULL,
                        producer_run_id TEXT,
                        run_dir TEXT,
                        state_root TEXT,
                        last_sequence INTEGER NOT NULL DEFAULT 0,
                        snapshot_json TEXT NOT NULL,
                        terminal_class TEXT,
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                        PRIMARY KEY (session_id, launch_generation),
                        UNIQUE (session_id, producer_run_id)
                    )
                    "#,
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS idx_tagteam_bindings_session_updated ON tagteam_run_bindings(session_id, updated_at DESC)",
                )
                .execute(&mut **tx)
                .await?;
            }
            20 => {
                let producer_run_ids = sqlx::query_scalar::<_, String>(
                    "SELECT producer_run_id FROM tagteam_run_bindings WHERE producer_run_id IS NOT NULL",
                )
                .fetch_all(&mut **tx)
                .await?;
                for run_id in &producer_run_ids {
                    if run_id.is_empty()
                        || run_id.trim() != run_id
                        || run_id.len() > 256
                        || run_id.chars().any(char::is_control)
                    {
                        anyhow::bail!("tagteam binding contains an invalid producer run id");
                    }
                }

                let duplicate_run_id = sqlx::query_scalar::<_, String>(
                    "SELECT producer_run_id FROM tagteam_run_bindings WHERE producer_run_id IS NOT NULL GROUP BY producer_run_id HAVING COUNT(*) > 1 LIMIT 1",
                )
                .fetch_optional(&mut **tx)
                .await?;
                if let Some(run_id) = duplicate_run_id {
                    anyhow::bail!("producer run id is attached to multiple sessions: {run_id}");
                }

                let invalid_generation = sqlx::query_scalar::<_, i64>(
                    "SELECT launch_generation FROM tagteam_run_bindings WHERE launch_generation < 1 LIMIT 1",
                )
                .fetch_optional(&mut **tx)
                .await?;
                if let Some(generation) = invalid_generation {
                    anyhow::bail!("tagteam binding has invalid launch generation: {generation}");
                }

                sqlx::query("DROP INDEX IF EXISTS idx_tagteam_bindings_session_updated")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("DROP INDEX IF EXISTS idx_tagteam_bindings_producer_run_id")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("DROP INDEX IF EXISTS idx_tagteam_bindings_launch_nonce")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("ALTER TABLE tagteam_run_bindings RENAME TO tagteam_run_bindings_v19")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query(
                    r#"
                    CREATE TABLE tagteam_run_bindings (
                        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                        launch_generation INTEGER NOT NULL CHECK(launch_generation >= 1),
                        schema_version INTEGER NOT NULL,
                        launch_spec_json TEXT NOT NULL,
                        action_digest TEXT NOT NULL,
                        launch_nonce TEXT NOT NULL UNIQUE,
                        producer_run_id TEXT,
                        run_dir TEXT,
                        state_root TEXT,
                        last_sequence INTEGER NOT NULL DEFAULT 0,
                        snapshot_json TEXT NOT NULL,
                        terminal_class TEXT,
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                        PRIMARY KEY (session_id, launch_generation)
                    )
                    "#,
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    r#"
                    INSERT INTO tagteam_run_bindings (
                        session_id, launch_generation, schema_version, launch_spec_json,
                        action_digest, launch_nonce, producer_run_id, run_dir, state_root,
                        last_sequence, snapshot_json, terminal_class, created_at, updated_at
                    )
                    SELECT session_id, launch_generation, schema_version, launch_spec_json,
                           action_digest, lower(hex(randomblob(16))), producer_run_id, run_dir,
                           state_root, last_sequence,
                           CASE
                               WHEN last_sequence > 0 THEN json_set(
                                   snapshot_json,
                                   '$.last_observation_digest',
                                   printf('%064d', 0)
                               )
                               ELSE snapshot_json
                           END,
                           terminal_class,
                           created_at, updated_at
                    FROM tagteam_run_bindings_v19
                    "#,
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query("DROP TABLE tagteam_run_bindings_v19")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query(
                    "CREATE UNIQUE INDEX idx_tagteam_bindings_producer_run_id ON tagteam_run_bindings(producer_run_id) WHERE producer_run_id IS NOT NULL",
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    "CREATE INDEX idx_tagteam_bindings_session_updated ON tagteam_run_bindings(session_id, updated_at DESC)",
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS tagteam_launch_identities (
                        launch_nonce TEXT PRIMARY KEY,
                        session_id TEXT NOT NULL,
                        launch_generation INTEGER NOT NULL CHECK(launch_generation >= 1),
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                    )
                    "#,
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    r#"
                    INSERT INTO tagteam_launch_identities(
                        launch_nonce, session_id, launch_generation, created_at
                    )
                    SELECT launch_nonce, session_id, launch_generation, created_at
                    FROM tagteam_run_bindings
                    "#,
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS tagteam_producer_run_ids (
                        producer_run_id TEXT PRIMARY KEY,
                        session_id TEXT NOT NULL,
                        launch_nonce TEXT NOT NULL,
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                    )
                    "#,
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    r#"
                    INSERT INTO tagteam_producer_run_ids(
                        producer_run_id, session_id, launch_nonce, created_at
                    )
                    SELECT producer_run_id, session_id, launch_nonce, created_at
                    FROM tagteam_run_bindings
                    WHERE producer_run_id IS NOT NULL
                    "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS tagteam_launch_counters (
                        session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
                        last_generation INTEGER NOT NULL CHECK(last_generation >= 1)
                    )
                    "#,
                )
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    r#"
                    INSERT INTO tagteam_launch_counters(session_id, last_generation)
                    SELECT session_id, MAX(launch_generation)
                    FROM tagteam_run_bindings
                    GROUP BY session_id
                    ON CONFLICT(session_id) DO UPDATE SET
                        last_generation = MAX(last_generation, excluded.last_generation)
                    "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            21 => {
                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS legacy_import_status (
                        id INTEGER PRIMARY KEY CHECK (id = 1),
                        completed_at TIMESTAMP
                    )
                    "#,
                )
                .execute(&mut **tx)
                .await?;

                // Databases reaching this migration already existed before the
                // marker was introduced, so whatever legacy `.jsonl` import they
                // were ever going to run has already happened (or never
                // applied because they predate the legacy on-disk format).
                // Backfill it as complete so upgrading installs don't get
                // silently re-imported and have since-accumulated session
                // history overwritten by the original legacy snapshot.
                sqlx::query(
                    "INSERT OR IGNORE INTO legacy_import_status (id, completed_at) VALUES (1, CURRENT_TIMESTAMP)",
                )
                .execute(&mut **tx)
                .await?;
            }
            22 => {
                for column in [
                    "workspace_id",
                    "workspace_name",
                    "credential_profile_id",
                    "credential_profile_name",
                    "credential_binding_id",
                    "workspace_context_json",
                ] {
                    let exists = sqlx::query_scalar::<_, i32>(
                        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = ?",
                    )
                    .bind(column)
                    .fetch_one(&mut **tx)
                    .await?
                        > 0;
                    if !exists {
                        sqlx::query(&format!("ALTER TABLE sessions ADD COLUMN {column} TEXT"))
                            .execute(&mut **tx)
                            .await?;
                    }
                }
                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS idx_sessions_workspace ON sessions(workspace_id)",
                )
                .execute(&mut **tx)
                .await?;
            }
            23 => Self::create_tool_operations_schema(tx).await?,
            24 => {
                // The restriction flag became opt-in (default off). Builds
                // before this version force-seeded it on for every workspace
                // session, so clear it exactly there. Non-workspace sessions
                // keep their value: any `true` on those was a deliberate
                // per-chat opt-in or an untrusted import.
                sqlx::query(
                    "UPDATE sessions SET restrict_tools_to_working_dirs = FALSE WHERE workspace_id IS NOT NULL",
                )
                .execute(&mut **tx)
                .await?;
            }
            25 => Self::create_message_search_schema(tx).await?,
            26 => {
                Self::create_session_artifacts_schema(tx).await?;
                Self::backfill_session_artifacts(tx).await?;
            }
            27 => {
                // CON-GSL-001: recover previously had no way to tell a live
                // peer's in-flight tool call from a crashed owner's, because
                // `owner_id` is a per-instance UUID with no liveness signal.
                // Recording the dispatching OS process id lets recovery probe
                // it with `subprocess::process_is_alive` instead of guessing
                // from `updated_at` recency. A brand-new database already has
                // this column (`create_tool_operations_schema` declares it
                // directly, e.g. fresh schema); this ALTER only reaches
                // databases that created `tool_operations` before this
                // migration existed.
                let has_owner_pid = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('tool_operations') WHERE name = 'owner_pid'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if !has_owner_pid {
                    sqlx::query("ALTER TABLE tool_operations ADD COLUMN owner_pid INTEGER")
                        .execute(&mut **tx)
                        .await?;
                }
            }
            28 => Self::create_session_library_schema(tx).await?,
            29 => Self::create_session_turn_lease_schema(tx).await?,
            30 => {
                // Tagteam support was removed in 602ae43c (2026-08-27). This
                // migration is destructive and irreversible by design: it
                // discards persisted Tagteam launch identities, producer run
                // ids, run bindings, and counters. That data was accepted as
                // unrecoverable when the feature was removed; there is no
                // archive and no down migration. Sessions themselves are
                // preserved and become ordinary sessions.
                //
                // Migrations 19 and 20 must stay in the ladder even though
                // fresh databases no longer create Tagteam state, because an
                // older database still upgrades through them to reach this
                // point. See the regression guard
                // `test_removed_tagteam_schema_is_cleaned_up`.
                sqlx::query("DROP TABLE IF EXISTS tagteam_launch_identities")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("DROP TABLE IF EXISTS tagteam_producer_run_ids")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("DROP TABLE IF EXISTS tagteam_run_bindings")
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("DROP TABLE IF EXISTS tagteam_launch_counters")
                    .execute(&mut **tx)
                    .await?;
                let has_workflow_kind = sqlx::query_scalar::<_, i32>(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'workflow_kind'",
                )
                .fetch_one(&mut **tx)
                .await?
                    > 0;
                if has_workflow_kind {
                    sqlx::query("ALTER TABLE sessions DROP COLUMN workflow_kind")
                        .execute(&mut **tx)
                        .await?;
                }
            }
            31 => {
                sqlx::query(
                    "CREATE INDEX IF NOT EXISTS idx_messages_session_time_asc ON messages(session_id, created_timestamp, id)",
                )
                .execute(&mut **tx)
                .await?;
            }
            32 => Self::create_output_revisions_schema(tx).await?,
            _ => {
                anyhow::bail!("Unknown migration version: {}", version);
            }
        }

        Ok(())
    }
}
