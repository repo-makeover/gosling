use super::{Session, SessionManager, SessionStorage, SessionType};
use crate::conversation::message::MessageMetadata;
use crate::permission::working_dir_scope_inspector::{is_mutating_tool_call, mutation_paths};
use crate::session::artifacts::{
    discover_from_successful_tool, DiscoveredArtifact, SessionArtifactProvenance,
    SessionArtifactRelation,
};
use crate::session::output_revisions::{
    annotated_snapshot, canonical_output_path, digest, is_output_document, markdown_body,
    output_roots, output_roots_with_warnings, prepare_replacement, read_snapshot,
    replace_if_unchanged, scan_output_roots, OutputRevisionError, OutputSnapshot,
    MAX_CAPTURE_BYTES, MAX_OUTPUT_REVISION_BYTES,
};
use anyhow::{ensure, Result};
use base64::Engine;
use chrono::{DateTime, Utc};
use gosling_sdk_types::custom_requests::*;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use sqlx::Sqlite;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub struct OutputCapture {
    started_at: DateTime<Utc>,
    parent_session_id: Option<String>,
    session: Session,
    contributor: OutputContributor,
    call: CallToolRequestParams,
    before: BTreeMap<PathBuf, OutputSnapshot>,
    candidates: BTreeSet<PathBuf>,
    unobserved: BTreeSet<PathBuf>,
    scan_complete: bool,
    warnings: Vec<String>,
}

impl SessionStorage {
    pub(super) async fn create_output_revisions_schema(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS output_revisions (
            path TEXT NOT NULL, version INTEGER NOT NULL, event_id TEXT NOT NULL,
            metadata_json TEXT NOT NULL, content BLOB NOT NULL,
            PRIMARY KEY(path, version), UNIQUE(path, event_id)
        )",
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

impl SessionManager {
    pub async fn prepare_output_capture(
        &self,
        session: &Session,
        call: &CallToolRequestParams,
        request_id: &str,
    ) -> Result<Option<OutputCapture>> {
        let operation = call.name.rsplit("__").next().unwrap_or(&call.name);
        if !is_mutating_tool_call(call)
            || matches!(operation, "delegate" | "orchestrate" | "summon")
        {
            return Ok(None);
        }
        let metadata: Option<String> = sqlx::query_scalar(
            "SELECT messages.metadata_json FROM messages, json_each(messages.content_json) AS item
            WHERE messages.session_id = ? AND json_extract(item.value, '$.type') = 'toolRequest'
            AND json_extract(item.value, '$.id') = ? ORDER BY messages.id DESC LIMIT 1",
        )
        .bind(&session.id)
        .bind(request_id)
        .fetch_optional(self.storage.pool().await?)
        .await?
        .flatten();
        let metadata = metadata
            .map(|json| serde_json::from_str::<MessageMetadata>(&json))
            .transpose()?;
        if metadata
            .as_ref()
            .is_some_and(|metadata| metadata.imported_untrusted)
        {
            return Ok(None);
        }
        let inference = metadata.and_then(|metadata| metadata.inference);
        let identity = session
            .extension_data
            .get_extension_state("output_agent", "v1");
        let parent_session_id = if session.session_type == SessionType::SubAgent {
            identity
                .and_then(|value| value.get("parentSessionId"))
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        } else {
            None
        };
        let contributor = OutputContributor {
            agent: if session.session_type == SessionType::SubAgent {
                identity
                    .and_then(|value| value.get("name"))
                    .and_then(|value| value.as_str())
                    .unwrap_or(&session.name)
                    .to_owned()
            } else {
                "gosling".into()
            },
            session_id: session.id.clone(),
            session_name: session.name.clone(),
            source_id: request_id.into(),
            provider: inference.as_ref().map(|value| value.provider.clone()),
            selected_model: inference
                .as_ref()
                .map(|value| value.requested_model.clone()),
            resolved_model: inference.and_then(|value| value.resolved_model),
        };
        let started_at = Utc::now();
        let mut session = session.clone();
        if let Some(parent_id) = &parent_session_id {
            let parent = self.get_session(parent_id, false).await?;
            session.workspace_context = parent.workspace_context;
            session.workspace_id = parent.workspace_id;
            session.additional_working_dirs = parent.additional_working_dirs;
        }
        let call = call.clone();
        tokio::task::spawn_blocking(move || {
            let candidates: BTreeSet<_> = mutation_paths(&call, &session.working_dir)
                .into_iter()
                .filter(|path| is_output_document(path))
                .collect();
            let (roots, mut warnings) = output_roots_with_warnings(&session);
            let (observed, scan_warnings) = scan_output_roots(&roots);
            warnings.extend(scan_warnings);
            let mut scan_complete = warnings.is_empty();
            let candidates = candidates
                .iter()
                .chain(observed.iter().filter(|path| !candidates.contains(*path)));
            let mut before = BTreeMap::new();
            let mut authorized = BTreeSet::new();
            let mut unobserved = BTreeSet::new();
            let mut total = 0;
            for candidate in candidates {
                let path = match canonical_output_path(&session, candidate, true) {
                    Ok(path) => path,
                    Err(error)
                        if error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
                    {
                        continue;
                    }
                    Err(error) => {
                        if error.downcast_ref::<std::io::Error>().is_some() {
                            scan_complete = false;
                            warnings.push(format!("{}: {error}", candidate.display()));
                        }
                        continue;
                    }
                };
                match read_snapshot(&path) {
                    Ok(Some(snapshot)) if total + snapshot.bytes.len() <= MAX_CAPTURE_BYTES => {
                        total += snapshot.bytes.len();
                        before.insert(path.clone(), snapshot);
                    }
                    Ok(None) => {}
                    Ok(Some(_)) => {
                        warnings.push(format!(
                            "{}: Output observation exceeded 32 MiB",
                            path.display()
                        ));
                        unobserved.insert(path);
                        continue;
                    }
                    Err(error) => {
                        warnings.push(format!("{}: {error}", path.display()));
                        unobserved.insert(path);
                        continue;
                    }
                }
                authorized.insert(path);
            }
            Ok(Some(OutputCapture {
                started_at,
                parent_session_id,
                session,
                contributor,
                call,
                before,
                candidates: authorized,
                unobserved,
                scan_complete,
                warnings,
            }))
        })
        .await?
    }

    pub async fn finish_output_capture(
        &self,
        capture: OutputCapture,
        result: &CallToolResult,
    ) -> Result<()> {
        if result.is_error == Some(true) {
            return Ok(());
        }
        let discovered = discover_from_successful_tool(
            &capture.call,
            result,
            &capture.session.working_dir,
            capture.session.workspace_id.as_deref(),
            Some(&capture.contributor.source_id),
        );
        let direct: BTreeSet<_> = discovered
            .iter()
            .filter(|artifact| {
                artifact.provenance == SessionArtifactProvenance::BuiltInTool
                    && artifact.relation != SessionArtifactRelation::Referenced
            })
            .map(|artifact| PathBuf::from(&artifact.resolved_path))
            .collect();
        let observed_session = capture.session.clone();
        let mut candidates = capture.candidates.clone();
        candidates.extend(
            discovered
                .iter()
                .filter(|artifact| artifact.relation != SessionArtifactRelation::Referenced)
                .map(|artifact| PathBuf::from(&artifact.resolved_path))
                .filter(|path| is_output_document(path)),
        );
        let preferred = direct.clone();
        let after = tokio::task::spawn_blocking(move || -> Result<_> {
            let (observed, mut warnings) = scan_output_roots(&output_roots(&observed_session));
            candidates.extend(observed);
            let candidates = preferred
                .iter()
                .chain(candidates.iter().filter(|path| !preferred.contains(*path)));
            let mut after = BTreeMap::new();
            let mut total = 0;
            for path in candidates {
                let Ok(path) = canonical_output_path(&observed_session, path, true) else {
                    continue;
                };
                match read_snapshot(&path) {
                    Ok(Some(snapshot)) if total + snapshot.bytes.len() <= MAX_CAPTURE_BYTES => {
                        total += snapshot.bytes.len();
                        after.insert(path, snapshot);
                    }
                    Ok(None) => {}
                    Ok(Some(_)) => warnings.push(format!(
                        "{}: Output observation exceeded 32 MiB",
                        path.display()
                    )),
                    Err(error) => warnings.push(format!("{}: {error}", path.display())),
                }
            }
            Ok((after, warnings))
        })
        .await??;
        let (after, mut warnings) = after;
        warnings.extend(capture.warnings.clone());
        for (path, after) in after {
            // Incomplete observation cannot establish that a file was created or changed.
            if capture.unobserved.contains(&path)
                || (!capture.scan_complete && !capture.candidates.contains(&path))
            {
                continue;
            }
            let result: Result<()> = async {
                let before = capture.before.get(&path);
                let unchanged = before.is_some_and(|before| before.body == after.body);
                if unchanged && before.is_some_and(|before| before.bytes == after.bytes) {
                    return Ok(());
                }
                let _guard = self.storage.acquire_write_guard().await;
                let pool = self.storage.pool().await?;
                let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
                let history = history_in_tx(&mut tx, &path).await?;
                let latest = history.last();
                if latest.is_some_and(|latest| latest.content_hash == digest(&after.body)) {
                    if output_roots(&capture.session)
                        .iter()
                        .any(|root| path.starts_with(root))
                    {
                        let bytes = annotated_snapshot(&path, &after.body, &history);
                        if bytes != after.bytes {
                            let session = capture.session.clone();
                            let path = path.clone();
                            let hash = after.hash;
                            tokio::task::spawn_blocking(move || {
                                replace_if_unchanged(&session, &path, &hash, &bytes)
                            })
                            .await??;
                        }
                    }
                    return Ok(());
                }
                if unchanged {
                    return Ok(());
                }
                let concurrent = latest.is_some_and(|latest| {
                    DateTime::parse_from_rfc3339(&latest.recorded_at)
                        .is_ok_and(|recorded| recorded > capture.started_at)
                        && before.is_none_or(|before| latest.content_hash != digest(&before.body))
                });
                if let Some(before) = before {
                    if !history
                        .iter()
                        .any(|revision| revision.content_hash == digest(&before.body))
                    {
                        let baseline = revision(
                            &history,
                            &before.body,
                            unknown_contributor(&capture.contributor),
                            OutputRevisionAction::Baseline,
                            OutputAttributionKind::Unknown,
                            None,
                        );
                        insert_revision(&mut tx, &path, &baseline, &before.bytes).await?;
                    }
                }
                tx.commit().await?;
                let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
                let history = history_in_tx(&mut tx, &path).await?;
                let kind = if concurrent {
                    OutputAttributionKind::Unknown
                } else if direct.contains(&path) {
                    OutputAttributionKind::Tool
                } else {
                    OutputAttributionKind::Observed
                };
                let contributor = if concurrent {
                    unknown_contributor(&capture.contributor)
                } else {
                    capture.contributor.clone()
                };
                let action = if before.is_none() && history.is_empty() {
                    OutputRevisionAction::Created
                } else {
                    OutputRevisionAction::Modified
                };
                let next = revision(&history, &after.body, contributor, action, kind, None);
                let mut history = history;
                history.push(next.clone());
                let annotate = output_roots(&capture.session)
                    .iter()
                    .any(|root| path.starts_with(root));
                let bytes = if annotate {
                    annotated_snapshot(&path, &after.body, &history)
                } else {
                    after.bytes.clone()
                };
                ensure!(
                    bytes.len() <= MAX_OUTPUT_REVISION_BYTES,
                    "Output with attribution exceeds the 8 MiB revision limit"
                );
                insert_revision(&mut tx, &path, &next, &bytes).await?;
                let artifact = DiscoveredArtifact {
                    display_path: path.to_string_lossy().into_owned(),
                    resolved_path: path.to_string_lossy().into_owned(),
                    base_working_dir: capture.session.working_dir.to_string_lossy().into_owned(),
                    workspace_id: capture.session.workspace_id.clone(),
                    mime_type: None,
                    relation: if action == OutputRevisionAction::Created {
                        SessionArtifactRelation::Created
                    } else {
                        SessionArtifactRelation::Modified
                    },
                    provenance: if direct.contains(&path) {
                        SessionArtifactProvenance::BuiltInTool
                    } else {
                        SessionArtifactProvenance::ToolArgument
                    },
                    source_id: Some(capture.contributor.source_id.clone()),
                };
                let artifacts = [artifact];
                SessionStorage::upsert_artifacts_in_tx(&mut tx, &capture.session.id, &artifacts)
                    .await?;
                if let Some(parent_id) = &capture.parent_session_id {
                    SessionStorage::upsert_artifacts_in_tx(&mut tx, parent_id, &artifacts).await?;
                }
                tx.commit().await?;
                if bytes != after.bytes {
                    let session = capture.session.clone();
                    let path = path.clone();
                    let hash = after.hash;
                    let bytes = bytes.clone();
                    tokio::task::spawn_blocking(move || {
                        replace_if_unchanged(&session, &path, &hash, &bytes)
                    })
                    .await??;
                }
                Ok(())
            }
            .await;
            if let Err(error) = result {
                warnings.push(format!("{}: {error}", path.display()));
            }
        }
        ensure!(
            warnings.is_empty(),
            "Output history partially recorded ({} warnings): {}",
            warnings.len(),
            warnings
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
        Ok(())
    }

    async fn authorized_output(
        &self,
        session_id: &str,
        path: &str,
        write: bool,
    ) -> Result<(Session, PathBuf)> {
        let session = self.get_session(session_id, false).await?;
        let requested = path.to_string();
        let scope = session.clone();
        let resolved = tokio::task::spawn_blocking(move || {
            canonical_output_path(&scope, Path::new(&requested), write).map_err(|error| {
                if error.downcast_ref::<std::io::Error>().is_some() {
                    error
                } else {
                    OutputRevisionError::Validation(error.to_string()).into()
                }
            })
        })
        .await??;
        let registered: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM session_artifacts WHERE session_id = ? AND (resolved_path = ? OR resolved_path = ?))")
            .bind(session_id).bind(path).bind(resolved.to_string_lossy().as_ref()).fetch_one(self.storage.pool().await?).await?;
        ensure!(
            registered,
            OutputRevisionError::NotFound("File is not an output of this session".into())
        );
        Ok((session, resolved))
    }

    pub async fn list_output_revisions(
        &self,
        request: ListOutputRevisionsRequest,
    ) -> Result<ListOutputRevisionsResponse> {
        let (_, path) = self
            .authorized_output(&request.session_id, &request.path, false)
            .await?;
        let limit = request.limit.unwrap_or(50).clamp(1, 100);
        let rows: Vec<String> = sqlx::query_scalar("SELECT metadata_json FROM output_revisions WHERE path = ? AND version < ? ORDER BY version DESC LIMIT ?")
            .bind(path.to_string_lossy().as_ref()).bind(request.before_version.unwrap_or(i64::MAX)).bind((limit + 1) as i64)
            .fetch_all(self.storage.pool().await?).await?;
        let mut revisions = rows
            .into_iter()
            .map(|json| serde_json::from_str::<OutputRevisionDto>(&json))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let more = revisions.len() > limit;
        revisions.truncate(limit);
        Ok(ListOutputRevisionsResponse {
            next_before_version: if more {
                revisions.last().map(|revision| revision.version)
            } else {
                None
            },
            revisions,
        })
    }

    pub async fn get_output_revision(
        &self,
        request: GetOutputRevisionRequest,
    ) -> Result<GetOutputRevisionResponse> {
        let (_, path) = self
            .authorized_output(&request.session_id, &request.path, false)
            .await?;
        let (metadata, bytes): (String, Vec<u8>) = sqlx::query_as(
            "SELECT metadata_json, content FROM output_revisions WHERE path = ? AND version = ?",
        )
        .bind(path.to_string_lossy().as_ref())
        .bind(request.version)
        .fetch_one(self.storage.pool().await?)
        .await?;
        // Saved bytes remain exportable when the live file cannot be safely snapshotted.
        let current_hash = tokio::task::spawn_blocking(move || read_snapshot(&path))
            .await?
            .ok()
            .flatten()
            .map(|snapshot| snapshot.hash);
        Ok(GetOutputRevisionResponse {
            revision: serde_json::from_str(&metadata)?,
            content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            current_hash,
        })
    }

    pub async fn restore_output_revision(
        &self,
        request: RestoreOutputRevisionRequest,
    ) -> Result<RestoreOutputRevisionResponse> {
        let (session, path) = self
            .authorized_output(&request.session_id, &request.path, true)
            .await?;
        let _guard = self.storage.acquire_write_guard().await;
        let pool = self.storage.pool().await?;
        let snapshot_path = path.clone();
        let current = tokio::task::spawn_blocking(move || read_snapshot(&snapshot_path))
            .await??
            .ok_or_else(|| {
                OutputRevisionError::NotFound(
                    "Output no longer exists; export the saved revision instead".into(),
                )
            })?;
        ensure!(
            current.hash == request.expected_current_hash,
            OutputRevisionError::Conflict(
                "Output changed; refresh its history before restoring".into()
            )
        );
        let content: Vec<u8> = sqlx::query_scalar(
            "SELECT content FROM output_revisions WHERE path = ? AND version = ?",
        )
        .bind(path.to_string_lossy().as_ref())
        .bind(request.version)
        .fetch_one(pool)
        .await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let history = history_in_tx(&mut tx, &path).await?;
        let contributor = OutputContributor {
            agent: "User".into(),
            session_id: session.id.clone(),
            session_name: session.name.clone(),
            source_id: uuid::Uuid::new_v4().to_string(),
            ..Default::default()
        };
        if history
            .last()
            .is_none_or(|last| last.content_hash != digest(&current.body))
        {
            let baseline = revision(
                &history,
                &current.body,
                unknown_contributor(&contributor),
                OutputRevisionAction::Baseline,
                OutputAttributionKind::Unknown,
                None,
            );
            insert_revision(&mut tx, &path, &baseline, &current.bytes).await?;
        }
        // Stage the file before committing; durable snapshots must precede replacement.
        let mut history = history_in_tx(&mut tx, &path).await?;
        let body = markdown_body(&path, &content);
        let next = revision(
            &history,
            &body,
            contributor,
            OutputRevisionAction::Restored,
            OutputAttributionKind::User,
            Some(request.version),
        );
        history.push(next.clone());
        let bytes = if output_roots(&session)
            .iter()
            .any(|root| path.starts_with(root))
        {
            annotated_snapshot(&path, &body, &history)
        } else {
            content
        };
        ensure!(
            bytes.len() <= MAX_OUTPUT_REVISION_BYTES,
            OutputRevisionError::Limit(
                "Output with attribution exceeds the 8 MiB revision limit".into()
            )
        );
        insert_revision(&mut tx, &path, &next, &bytes).await?;
        let expected = request.expected_current_hash;
        let replacement = tokio::task::spawn_blocking(move || {
            prepare_replacement(&session, &path, &expected, &bytes)
        })
        .await??;
        tx.commit().await?;
        tokio::task::spawn_blocking(move || replacement.persist()).await?
            .map_err(|error| error.context("Restore snapshots saved, but file replacement failed; refresh history before retrying"))?;
        Ok(RestoreOutputRevisionResponse { revision: next })
    }
}

async fn history_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    path: &Path,
) -> Result<Vec<OutputRevisionDto>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT metadata_json FROM output_revisions WHERE path = ? ORDER BY version LIMIT 1001",
    )
    .bind(path.to_string_lossy().as_ref())
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= 1000,
        OutputRevisionError::Limit("Output has reached the 1000-revision capture limit".into())
    );
    Ok(rows
        .into_iter()
        .map(|json| serde_json::from_str(&json))
        .collect::<std::result::Result<_, _>>()?)
}

fn unknown_contributor(original: &OutputContributor) -> OutputContributor {
    OutputContributor {
        agent: "Unknown".into(),
        session_id: original.session_id.clone(),
        session_name: original.session_name.clone(),
        source_id: format!("{}:baseline", original.source_id),
        ..Default::default()
    }
}

fn revision(
    history: &[OutputRevisionDto],
    body: &[u8],
    contributor: OutputContributor,
    action: OutputRevisionAction,
    attribution: OutputAttributionKind,
    restored_from: Option<i64>,
) -> OutputRevisionDto {
    OutputRevisionDto {
        version: history.last().map_or(1, |last| last.version + 1),
        recorded_at: Utc::now().to_rfc3339(),
        content_hash: digest(body),
        size_bytes: body.len(),
        action,
        attribution,
        contributor,
        restored_from,
    }
}

async fn insert_revision(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    path: &Path,
    revision: &OutputRevisionDto,
    bytes: &[u8],
) -> Result<()> {
    ensure!(
        revision.version <= 1000,
        OutputRevisionError::Limit("Output has reached the 1000-revision capture limit".into())
    );
    sqlx::query("INSERT INTO output_revisions(path, version, event_id, metadata_json, content) VALUES (?, ?, ?, ?, ?)")
        .bind(path.to_string_lossy().as_ref()).bind(revision.version)
        .bind(format!("{}:{}:{:?}", revision.contributor.session_id, revision.contributor.source_id, revision.action))
        .bind(serde_json::to_string(revision)?).bind(bytes).execute(&mut **tx).await?;
    Ok(())
}
