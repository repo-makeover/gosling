//! Bounded ACP message, tool-result, status, artifact, and replay projection.
//!
//! Maintainers: project durable state without mutating or trusting client-supplied metadata.
//! Clients: standard and Gosling notification payloads retain their existing shapes and bounds.

use super::*;

pub(super) fn outcome_to_confirmation(
    outcome: &RequestPermissionOutcome,
) -> PermissionConfirmation {
    PermissionConfirmation {
        principal_type: PrincipalType::Tool,
        permission: Permission::from(PermissionDecision::from(outcome)),
    }
}

pub(super) fn prompt_error_from_message(message: &Message) -> Option<agent_client_protocol::Error> {
    message
        .content
        .iter()
        .find_map(prompt_error_from_message_content)
        .or_else(|| {
            message
                .metadata
                .terminal_error
                .as_ref()
                .map(|reason| agent_client_protocol::Error::new(-32603, reason.clone()))
        })
}

pub(super) fn prompt_error_from_message_content(
    content_item: &MessageContent,
) -> Option<agent_client_protocol::Error> {
    match content_item {
        MessageContent::SystemNotification(notification)
            if notification.notification_type == SystemNotificationType::CreditsExhausted =>
        {
            Some(credits_exhausted_prompt_error(notification))
        }
        _ => None,
    }
}

fn credits_exhausted_prompt_error(
    notification: &SystemNotificationContent,
) -> agent_client_protocol::Error {
    let mut data = serde_json::Map::new();
    data.insert(
        "reason".to_string(),
        serde_json::Value::String("credits_exhausted".to_string()),
    );

    if let Some(url) = notification
        .data
        .as_ref()
        .and_then(|data| data.get("top_up_url"))
        .and_then(|url| url.as_str())
    {
        data.insert(
            "url".to_string(),
            serde_json::Value::String(url.to_string()),
        );
    }

    agent_client_protocol::Error::new(-32603, notification.msg.clone())
        .data(serde_json::Value::Object(data))
}

pub(super) fn send_status_message_update(
    cx: &ConnectionTo<Client>,
    supports_gosling_custom_notifications: bool,
    session_id: &str,
    notification: &SystemNotificationContent,
) -> Result<(), agent_client_protocol::Error> {
    if let Some(status) = status_message_from_system_notification(notification) {
        if supports_gosling_custom_notifications {
            cx.send_notification(GoslingSessionNotification {
                session_id: session_id.to_string(),
                update: GoslingSessionUpdate::StatusMessage(StatusMessageUpdate { status }),
            })?;
        }
    }
    Ok(())
}

pub(super) fn session_artifact_dto(artifact: SessionArtifact) -> SessionArtifactDto {
    SessionArtifactDto {
        session_id: artifact.session_id,
        display_path: artifact.display_path,
        resolved_path: artifact.resolved_path,
        base_working_dir: artifact.base_working_dir,
        workspace_id: artifact.workspace_id,
        mime_type: artifact.mime_type,
        relation: match artifact.relation {
            SessionArtifactRelation::Created => SessionArtifactRelationDto::Created,
            SessionArtifactRelation::Modified => SessionArtifactRelationDto::Modified,
            SessionArtifactRelation::Referenced => SessionArtifactRelationDto::Referenced,
        },
        provenance: match artifact.provenance {
            SessionArtifactProvenance::BuiltInTool => SessionArtifactProvenanceDto::BuiltInTool,
            SessionArtifactProvenance::McpResourceLink => {
                SessionArtifactProvenanceDto::McpResourceLink
            }
            SessionArtifactProvenance::ToolMetadata => SessionArtifactProvenanceDto::ToolMetadata,
            SessionArtifactProvenance::ToolArgument => SessionArtifactProvenanceDto::ToolArgument,
            SessionArtifactProvenance::AssistantMessage => {
                SessionArtifactProvenanceDto::AssistantMessage
            }
            SessionArtifactProvenance::CompatibilityInference => {
                SessionArtifactProvenanceDto::CompatibilityInference
            }
        },
        source_id: artifact.source_id,
        first_seen_at: artifact.first_seen_at.to_rfc3339(),
        last_seen_at: artifact.last_seen_at.to_rfc3339(),
    }
}

fn status_message_from_system_notification(
    notification: &SystemNotificationContent,
) -> Option<StatusMessage> {
    match notification.notification_type {
        SystemNotificationType::InlineMessage => Some(StatusMessage::Notice {
            message: presentation::project_live_text(&notification.msg, "Status message"),
        }),
        SystemNotificationType::ThinkingMessage => Some(StatusMessage::Progress {
            message: presentation::project_live_text(&notification.msg, "Status message"),
        }),
        SystemNotificationType::CreditsExhausted => None,
    }
}

pub(super) fn message_update_meta(message_id: Option<&str>, created: i64, steer: bool) -> Meta {
    let mut gosling = serde_json::Map::new();
    gosling.insert("created".to_string(), serde_json::json!(created));
    if let Some(id) = message_id {
        gosling.insert(
            "messageId".to_string(),
            serde_json::json!(presentation::project_identifier(id)),
        );
    }
    if steer {
        gosling.insert("steer".to_string(), serde_json::json!(true));
    }

    let mut meta = serde_json::Map::new();
    meta.insert("gosling".to_string(), serde_json::Value::Object(gosling));
    meta
}

pub(super) fn extract_tool_call_update_meta(
    tool_response: &crate::conversation::message::ToolResponse,
) -> Option<Meta> {
    let tool_result = tool_response.tool_result.as_ref().ok()?;
    let gosling_meta = tool_result
        .meta
        .as_ref()?
        .0
        .get(TRUSTED_TOOL_UPDATE_META_KEY)?
        .clone();
    let mut meta_map = serde_json::Map::new();
    meta_map.insert("gosling".to_string(), gosling_meta);
    Some(meta_map)
}

pub(super) fn replay_message_meta(message: &Message) -> Meta {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "gosling".to_string(),
        serde_json::Value::Object(replay_message_gosling_meta(message)),
    );
    meta
}

fn replay_message_gosling_meta(message: &Message) -> serde_json::Map<String, serde_json::Value> {
    let mut gosling = serde_json::Map::new();
    gosling.insert("created".to_string(), serde_json::json!(message.created));
    if let Some(id) = &message.id {
        gosling.insert(
            "messageId".to_string(),
            serde_json::json!(presentation::project_identifier(id)),
        );
    }
    if message.metadata.steer {
        gosling.insert("steer".to_string(), serde_json::json!(true));
    }
    if message.metadata.imported_untrusted {
        gosling.insert("importedUntrusted".to_string(), serde_json::json!(true));
    }
    gosling
}

pub(super) fn merge_replay_message_meta(meta: Option<Meta>, message: &Message) -> Meta {
    let replay_gosling = replay_message_gosling_meta(message);
    let mut meta = meta.unwrap_or_default();
    let gosling_value = meta
        .entry("gosling".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    if let serde_json::Value::Object(gosling) = gosling_value {
        for (key, value) in replay_gosling {
            gosling.insert(key, value);
        }
    } else {
        *gosling_value = serde_json::Value::Object(replay_gosling);
    }

    meta
}

pub(super) fn build_tool_call_content(
    tool_result: &ToolResult<CallToolResult>,
) -> Vec<ToolCallContent> {
    match tool_result {
        Ok(result) => result
            .content
            .iter()
            .filter_map(|content| match &content.raw {
                RawContent::Text(val) => Some(ToolCallContent::Content(Content::new(
                    ContentBlock::Text(TextContent::new(val.text.clone())),
                ))),
                RawContent::Image(val) => Some(ToolCallContent::Content(Content::new(
                    ContentBlock::Image(ImageContent::new(val.data.clone(), val.mime_type.clone())),
                ))),
                RawContent::Resource(val) => {
                    let resource = match &val.resource {
                        ResourceContents::TextResourceContents {
                            mime_type,
                            text,
                            uri,
                            ..
                        } => EmbeddedResourceResource::TextResourceContents(
                            TextResourceContents::new(text.clone(), uri.clone())
                                .mime_type(mime_type.clone()),
                        ),
                        ResourceContents::BlobResourceContents {
                            mime_type,
                            blob,
                            uri,
                            ..
                        } => EmbeddedResourceResource::BlobResourceContents(
                            BlobResourceContents::new(blob.clone(), uri.clone())
                                .mime_type(mime_type.clone()),
                        ),
                    };
                    Some(ToolCallContent::Content(Content::new(
                        ContentBlock::Resource(EmbeddedResource::new(resource)),
                    )))
                }
                RawContent::Audio(_) | RawContent::ResourceLink(_) => None,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub(super) fn extract_tool_raw_output(
    tool_result: &ToolResult<CallToolResult>,
) -> Option<serde_json::Value> {
    tool_result
        .as_ref()
        .ok()
        .and_then(|result| result.structured_content.clone())
}
