use crate::conversation::message::{Message, MessageContent, ToolRequest};
use crate::conversation::Conversation;
use crate::prompt_template::render_template;
use crate::providers::base::Provider;
use chrono::Utc;
use indoc::indoc;
use rmcp::model::{Tool, ToolAnnotations};
use rmcp::object;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

const READ_ONLY_JUDGE_TOOL_NAME: &str = "platform__tool_by_tool_permission";

// Session settings keep the judge aligned with the active conversation; global
// settings are the fallback when the session's model configuration is unavailable.
async fn resolve_model_config(
    session_manager: &crate::session::SessionManager,
    session_id: &str,
) -> anyhow::Result<gosling_providers::model::ModelConfig> {
    if !session_id.is_empty() {
        if let Ok(session) = session_manager.get_session(session_id, false).await {
            if let Some(model_config) = session.model_config {
                return Ok(model_config);
            }
        }
    }

    let config = crate::config::Config::global();
    let provider_name = config
        .get_gosling_provider()
        .map_err(|_| anyhow::anyhow!("missing provider"))?;
    let model_name = config
        .get_gosling_model()
        .map_err(|_| anyhow::anyhow!("missing model"))?;
    crate::model_config::model_config_from_user_config(&provider_name, &model_name)
}

#[derive(Serialize)]
struct PermissionJudgeContext {
    // Keep an object-shaped template context even while it has no variables.
}

fn create_read_only_judge_tool() -> Tool {
    Tool::new(
        READ_ONLY_JUDGE_TOOL_NAME.to_string(),
        indoc! {r#"
            Analyze the tool requests and determine which ones perform read-only operations.

            What constitutes a read-only operation:
            - A read-only operation retrieves information without modifying any data or state.
            - Examples include:
                - Reading a file without writing to it.
                - Querying a database without making updates.
                - Retrieving information from APIs without performing POST, PUT, or DELETE operations.

            Examples of read vs. write operations:
            - Read Operations:
                - `SELECT` query in SQL.
                - Reading file metadata or content.
                - Listing directory contents.
            - Write Operations:
                - `INSERT`, `UPDATE`, or `DELETE` in SQL.
                - Writing or appending to a file.
                - Modifying system configurations.
                - Sending messages to Slack channel.

            How to analyze tool requests:
            - Inspect each tool request to identify its purpose based on its name and arguments.
            - Categorize the operation as read-only if it does not involve any state or data modification.
            - Return a list of tool names that are strictly read-only. If you cannot make the decision, then it is not read-only.

            Use this analysis to generate the list of tools performing read-only operations from the provided tool requests.
        "#}
        .to_string(),
        object!({
            "type": "object",
            "properties": {
                "read_only_tools": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional list of tool names which has read-only operations."
                }
            },
            "required": []
        })
    ).annotate(ToolAnnotations::with_title("Check tool operation".to_string()).read_only(true).destructive(false).idempotent(false).open_world(false))
}

/// Builds the message to be sent to the LLM for detecting read-only operations.
/// Includes each request's arguments, not just its tool name: a tool whose
/// read/write behavior depends on how it's called (e.g. a generic
/// `run_command` tool) cannot be classified correctly from its name alone.
fn create_check_messages(tool_requests: Vec<&ToolRequest>) -> Conversation {
    let tool_calls: Vec<String> = tool_requests
        .iter()
        .filter_map(|request| {
            let tool_call = request.tool_call.as_ref().ok()?;
            let arguments = tool_call
                .arguments
                .as_ref()
                .map(|arguments| serde_json::to_string(arguments).unwrap_or_default())
                .unwrap_or_default();
            Some(format!("{}({})", tool_call.name, arguments))
        })
        .collect();
    let mut check_messages = vec![];
    check_messages.push(Message::new(
        rmcp::model::Role::User,
        Utc::now().timestamp(),
        vec![MessageContent::text(format!(
                "Here are the tool requests, with the arguments they were actually called with: {:?}\n\nAnalyze the tool requests and list the tools that perform read-only operations. \
                \n\nGuidelines for Read-Only Operations: \
                \n- Read-only operations do not modify any data or state. \
                \n- Examples include file reading, SELECT queries in SQL, and directory listing. \
                \n- Write operations include INSERT, UPDATE, DELETE, and file writing. \
                \n- Base your judgment on the arguments shown, not just the tool name: a generic tool (e.g. a shell/command runner) can be read-only for one set of arguments and destructive for another. \
                \n\nPlease provide a list of tool names that qualify as read-only:",
                tool_calls.join(", "),
            ))],
    ));
    Conversation::new_unvalidated(check_messages)
}

/// Uses the first judge call with an array verdict; prose and unrelated calls are ignored.
fn extract_read_only_tool_names(response: &Message) -> Option<Vec<String>> {
    for content in &response.content {
        if let MessageContent::ToolRequest(tool_request) = content {
            if let Ok(tool_call) = &tool_request.tool_call {
                if tool_call.name == READ_ONLY_JUDGE_TOOL_NAME {
                    if let Some(arguments) = &tool_call.arguments {
                        if let Some(Value::Array(read_only_tools)) =
                            arguments.get("read_only_tools")
                        {
                            return Some(
                                read_only_tools
                                    .iter()
                                    .filter_map(|tool| tool.as_str().map(String::from))
                                    .collect(),
                            );
                        }
                    }
                }
            }
        }
    }
    None
}

/// Returns the model's suggested read-only tool names; callers still enforce permission policy.
/// Configuration resolution errors, completion errors or an unusable verdict yield an empty list.
pub async fn detect_read_only_tools(
    provider: Arc<dyn Provider>,
    session_manager: &crate::session::SessionManager,
    session_id: &str,
    tool_requests: Vec<&ToolRequest>,
) -> Vec<String> {
    if tool_requests.is_empty() {
        return vec![];
    }
    let judge_tool = create_read_only_judge_tool();
    let judge_messages = create_check_messages(tool_requests);

    let context = PermissionJudgeContext {};
    let system_prompt = render_template("permission_judge.md", &context)
        .unwrap_or_else(|_| "You are a good analyst and can detect operations whether they have read-only operations.".to_string());

    let model_config = match resolve_model_config(session_manager, session_id).await {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!("Could not resolve model config for permission judge: {e}");
            return vec![];
        }
    };
    let judge_response = crate::session_context::with_session_id(
        Some(session_id.to_string()),
        provider.complete(
            &model_config,
            &system_prompt,
            judge_messages.messages(),
            std::slice::from_ref(&judge_tool),
        ),
    )
    .await;

    if let Ok((message, _usage)) = judge_response {
        extract_read_only_tool_names(&message).unwrap_or_default()
    } else {
        vec![]
    }
}

/// Result of permission checking for tool requests
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionCheckResult {
    pub approved: Vec<ToolRequest>,
    pub needs_approval: Vec<ToolRequest>,
    pub denied: Vec<ToolRequest>,
}

impl PermissionCheckResult {
    pub fn approve_all(tool_requests: &[ToolRequest]) -> Self {
        Self {
            approved: tool_requests.to_vec(),
            needs_approval: Vec::new(),
            denied: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolRequestParams;

    #[test]
    fn create_check_messages_includes_call_arguments_not_just_tool_names() {
        let request = ToolRequest {
            id: "request-1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("run_command")
                .with_arguments(rmcp::object!({"command": "rm -rf /important-data"}))),
            metadata: None,
            tool_meta: None,
        };

        let conversation = create_check_messages(vec![&request]);

        let text = conversation.messages()[0]
            .content
            .iter()
            .filter_map(|c| match c {
                MessageContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert!(
            text.contains("run_command"),
            "message must name the tool: {text}"
        );
        assert!(
            text.contains("rm -rf /important-data"),
            "message must include the arguments the tool was actually called with, \
            not just its name, so the classifier can't be fooled by a generic tool \
            name into ignoring destructive arguments: {text}"
        );
    }

    #[test]
    fn approve_all_never_requests_or_denies_approval() {
        let requests = vec![ToolRequest {
            id: "request-1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("developer__write_file")),
            metadata: None,
            tool_meta: None,
        }];

        let result = PermissionCheckResult::approve_all(&requests);

        assert_eq!(result.approved, requests);
        assert!(result.needs_approval.is_empty());
        assert!(result.denied.is_empty());
    }
}
