use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

use crate::config::GoslingMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::permission_judge::PermissionCheckResult;
use crate::security::egress_inspector::EgressInspector;

/// Result of inspecting a tool call
#[derive(Debug, Clone)]
pub struct InspectionResult {
    pub tool_request_id: String,
    pub action: InspectionAction,
    pub reason: String,
    pub confidence: f32,
    pub inspector_name: String,
    pub finding_id: Option<String>,
    /// Inspector-specific structured data an approval-response handler may
    /// need after the user answers a `RequireApproval` prompt (e.g. the
    /// egress inspector's flagged domains, so "always allow this domain"
    /// can be persisted without adding an egress-specific field every
    /// inspector's result type would otherwise carry).
    pub metadata: Option<serde_json::Value>,
}

/// Action to take based on inspection result
#[derive(Debug, Clone, PartialEq)]
pub enum InspectionAction {
    /// Allow the tool to execute without user intervention
    Allow,
    /// Deny the tool execution completely
    Deny,
    /// Require user approval before execution (with optional warning message)
    RequireApproval(Option<String>),
}

/// Trait for all tool inspectors
#[async_trait]
pub trait ToolInspector: Send + Sync {
    /// Name of this inspector (for logging/debugging)
    fn name(&self) -> &'static str;

    /// Inspect tool requests and return results
    async fn inspect(
        &self,
        session_id: &str,
        tool_requests: &[ToolRequest],
        messages: &[Message],
        gosling_mode: GoslingMode,
    ) -> Result<Vec<InspectionResult>>;

    /// Whether this inspector is enabled
    fn is_enabled(&self) -> bool {
        true
    }

    /// Whether Auto mode may treat this inspector's approval requests as advisory.
    /// Workspace and security gates override this to keep their prompts mandatory.
    fn auto_downgrades_require_approval(&self) -> bool {
        true
    }

    /// Allow downcasting to concrete types
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Manages all tool inspectors and coordinates their results
pub struct ToolInspectionManager {
    inspectors: Vec<Box<dyn ToolInspector>>,
}

impl ToolInspectionManager {
    pub fn new() -> Self {
        Self {
            inspectors: Vec::new(),
        }
    }

    /// Add an inspector to the manager
    /// Inspectors run in the order they are added
    pub fn add_inspector(&mut self, inspector: Box<dyn ToolInspector>) {
        self.inspectors.push(inspector);
    }

    /// Runs enabled inspectors in order. Failures become mandatory approval results
    /// for every request in the batch, including in Auto mode.
    pub async fn inspect_tools(
        &self,
        session_id: &str,
        tool_requests: &[ToolRequest],
        messages: &[Message],
        gosling_mode: GoslingMode,
    ) -> Result<Vec<InspectionResult>> {
        let mut all_results = Vec::new();

        for inspector in &self.inspectors {
            if !inspector.is_enabled() {
                continue;
            }

            tracing::debug!(
                inspector_name = inspector.name(),
                tool_count = tool_requests.len(),
                "Running tool inspector"
            );

            match inspector
                .inspect(session_id, tool_requests, messages, gosling_mode)
                .await
            {
                Ok(mut results) => {
                    tracing::debug!(
                        inspector_name = inspector.name(),
                        result_count = results.len(),
                        "Tool inspector completed"
                    );
                    // Only opted-in advisory prompts are downgraded. Hard denials,
                    // mandatory prompts and inspector failures retain their restrictions.
                    if gosling_mode == GoslingMode::Auto
                        && inspector.auto_downgrades_require_approval()
                    {
                        for result in &mut results {
                            if matches!(result.action, InspectionAction::RequireApproval(_)) {
                                tracing::info!(
                                    security.event_type = "inspection_result",
                                    security.action = "ALLOW",
                                    inspector.name = result.inspector_name.as_str(),
                                    inspector.reason = %result.reason,
                                    "auto mode: approval requirement downgraded to allow"
                                );
                                result.action = InspectionAction::Allow;
                            }
                        }
                    }
                    all_results.extend(results);
                }
                Err(e) => {
                    tracing::error!(
                        inspector_name = inspector.name(),
                        error = %e,
                        "Tool inspector failed; failing closed by requiring approval for this batch"
                    );
                    // Auto's permission baseline may allow the entire batch. Emit
                    // a restriction for each request so failure cannot erase a gate.
                    for request in tool_requests {
                        all_results.push(InspectionResult {
                            tool_request_id: request.id.clone(),
                            action: InspectionAction::RequireApproval(Some(format!(
                                "Inspector '{}' failed to run; approval required as a safety fallback",
                                inspector.name()
                            ))),
                            reason: format!("inspector '{}' error: {e}", inspector.name()),
                            confidence: 1.0,
                            inspector_name: inspector.name().to_string(),
                            finding_id: None,
                            metadata: None,
                        });
                    }
                }
            }
        }

        Ok(all_results)
    }

    /// Get list of registered inspector names
    pub fn inspector_names(&self) -> Vec<&'static str> {
        self.inspectors
            .iter()
            .map(|inspector| inspector.name())
            .collect()
    }

    fn get_permission_inspector(&self) -> Option<&PermissionInspector> {
        self.inspectors
            .iter()
            .find(|inspector| inspector.name() == "permission")
            .and_then(|inspector| inspector.as_any().downcast_ref::<PermissionInspector>())
    }

    fn get_egress_inspector(&self) -> Option<&EgressInspector> {
        self.inspectors
            .iter()
            .find(|inspector| inspector.name() == "egress")
            .and_then(|inspector| inspector.as_any().downcast_ref::<EgressInspector>())
    }

    pub fn apply_tool_annotations(&self, tools: &[rmcp::model::Tool]) {
        if let Some(inspector) = self.get_permission_inspector() {
            inspector.apply_tool_annotations(tools);
        }
    }

    /// Persists a tool decision; callers must handle failure before dispatching the tool.
    pub async fn update_permission_manager(
        &self,
        tool_name: &str,
        permission_level: crate::config::permission::PermissionLevel,
    ) -> Result<()> {
        self.get_permission_inspector()
            .ok_or_else(|| anyhow::anyhow!("Permission inspector is unavailable"))?
            .permission_manager
            .update_user_permission(tool_name, permission_level)
            .map_err(anyhow::Error::msg)
    }

    /// Persists a domain decision without granting authority to the whole tool.
    pub async fn update_egress_domain_permission(
        &self,
        domain: &str,
        permission_level: crate::config::permission::PermissionLevel,
    ) -> Result<()> {
        self.get_egress_inspector()
            .ok_or_else(|| anyhow::anyhow!("Egress inspector is unavailable"))?
            .permission_manager
            .update_egress_domain_permission(domain, permission_level)
            .map_err(anyhow::Error::msg)
    }

    pub fn process_inspection_results_with_permission_inspector(
        &self,
        remaining_requests: &[ToolRequest],
        inspection_results: &[InspectionResult],
    ) -> Option<PermissionCheckResult> {
        self.get_permission_inspector().map(|inspector| {
            inspector.process_inspection_results(remaining_requests, inspection_results)
        })
    }
}

impl Default for ToolInspectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Tightens the baseline: deny outranks approval, and allow cannot undo either.
/// Inspection results are applied in order; an inspector can move a request
/// only toward a more restrictive outcome.
pub fn apply_inspection_results_to_permissions(
    mut permission_result: PermissionCheckResult,
    inspection_results: &[InspectionResult],
) -> PermissionCheckResult {
    if inspection_results.is_empty() {
        return permission_result;
    }

    let mut requests_by_id: HashMap<String, ToolRequest> = HashMap::new();

    for req in &permission_result.approved {
        requests_by_id.insert(req.id.clone(), req.clone());
    }
    for req in &permission_result.needs_approval {
        requests_by_id.insert(req.id.clone(), req.clone());
    }
    for req in &permission_result.denied {
        requests_by_id.insert(req.id.clone(), req.clone());
    }

    for result in inspection_results {
        let request_id = &result.tool_request_id;

        let security_action = match &result.action {
            InspectionAction::Deny => "BLOCK",
            InspectionAction::RequireApproval(_) => "ALERT",
            InspectionAction::Allow => "ALLOW",
        };

        tracing::info!(
            security.event_type = "inspection_result",
            security.action = security_action,
            security.confidence = result.confidence,
            security.finding_id = ?result.finding_id,
            tool.request_id = %request_id,
            inspector.name = result.inspector_name,
            inspector.reason = %result.reason,
            "inspection result applied"
        );

        match result.action {
            InspectionAction::Deny => {
                permission_result
                    .approved
                    .retain(|req| req.id != *request_id);
                permission_result
                    .needs_approval
                    .retain(|req| req.id != *request_id);

                if let Some(request) = requests_by_id.get(request_id) {
                    if !permission_result
                        .denied
                        .iter()
                        .any(|req| req.id == *request_id)
                    {
                        permission_result.denied.push(request.clone());
                    }
                }
            }
            InspectionAction::RequireApproval(_) => {
                permission_result
                    .approved
                    .retain(|req| req.id != *request_id);

                if permission_result
                    .denied
                    .iter()
                    .any(|req| req.id == *request_id)
                {
                    continue;
                }

                if let Some(request) = requests_by_id.get(request_id) {
                    if !permission_result
                        .needs_approval
                        .iter()
                        .any(|req| req.id == *request_id)
                    {
                        permission_result.needs_approval.push(request.clone());
                    }
                }
            }
            InspectionAction::Allow => {
                // An allow result is not authority to reverse another inspector's restriction.
            }
        }
    }

    permission_result
}

pub fn get_security_finding_id_from_results(
    tool_request_id: &str,
    inspection_results: &[InspectionResult],
) -> Option<String> {
    inspection_results
        .iter()
        .find(|result| {
            result.tool_request_id == tool_request_id && result.inspector_name == "security"
        })
        .and_then(|result| result.finding_id.clone())
}

/// The inspector-authored approval prompt for a request, if any inspector
/// required approval with a message. Inspectors run in registration order and
/// the permission baseline usually reports first, so the first result for a
/// request is often a plain `Allow`; a later security finding must still
/// reach the approval prompt instead of being shadowed by it.
pub fn security_prompt_for_request<'a>(
    tool_request_id: &str,
    inspection_results: &'a [InspectionResult],
) -> Option<&'a str> {
    inspection_results.iter().find_map(|result| {
        if result.tool_request_id != tool_request_id {
            return None;
        }
        match &result.action {
            InspectionAction::RequireApproval(Some(message)) => Some(message.as_str()),
            _ => None,
        }
    })
}

/// The single still-unresolved egress domain an inspector attached to a
/// request. More than one flagged domain makes a one-click grant ambiguous,
/// so only an exact single entry is offered.
pub fn single_flagged_domain_for_request(
    tool_request_id: &str,
    inspection_results: &[InspectionResult],
) -> Option<String> {
    inspection_results
        .iter()
        .filter(|result| result.tool_request_id == tool_request_id)
        .find_map(|result| {
            let domains = result.metadata.as_ref()?.get("domains")?.as_array()?;
            if domains.len() != 1 {
                return None;
            }
            domains[0].as_str().map(str::to_string)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::ToolRequest;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    #[test]
    fn test_apply_inspection_results() {
        let tool_request = ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("test_tool").with_arguments(object!({}))),
            metadata: None,
            tool_meta: None,
        };

        let permission_result = PermissionCheckResult {
            approved: vec![tool_request.clone()],
            needs_approval: vec![],
            denied: vec![],
        };

        let inspection_results = vec![InspectionResult {
            tool_request_id: "req_1".to_string(),
            action: InspectionAction::Deny,
            reason: "Test denial".to_string(),
            confidence: 0.9,
            inspector_name: "test_inspector".to_string(),
            finding_id: Some("TEST-001".to_string()),
            metadata: None,
        }];

        let updated_result =
            apply_inspection_results_to_permissions(permission_result, &inspection_results);

        assert_eq!(updated_result.approved.len(), 0);
        assert_eq!(updated_result.denied.len(), 1);
        assert_eq!(updated_result.denied[0].id, "req_1");
    }

    #[test]
    fn test_deny_takes_precedence_over_later_approval_requirement() {
        let tool_request = ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("test_tool").with_arguments(object!({}))),
            metadata: None,
            tool_meta: None,
        };
        let permission_result = PermissionCheckResult {
            approved: vec![tool_request],
            needs_approval: vec![],
            denied: vec![],
        };
        let inspection_results = [
            InspectionResult {
                tool_request_id: "req_1".to_string(),
                action: InspectionAction::Deny,
                reason: "hard deny".to_string(),
                confidence: 1.0,
                inspector_name: "deny".to_string(),
                finding_id: None,
                metadata: None,
            },
            InspectionResult {
                tool_request_id: "req_1".to_string(),
                action: InspectionAction::RequireApproval(Some("fallback".to_string())),
                reason: "inspector failure".to_string(),
                confidence: 1.0,
                inspector_name: "fallback".to_string(),
                finding_id: None,
                metadata: None,
            },
        ];

        let updated_result =
            apply_inspection_results_to_permissions(permission_result, &inspection_results);
        assert!(updated_result.approved.is_empty());
        assert!(updated_result.needs_approval.is_empty());
        assert_eq!(updated_result.denied.len(), 1);
    }

    struct RequireApprovalInspector;

    #[async_trait]
    impl ToolInspector for RequireApprovalInspector {
        fn name(&self) -> &'static str {
            "require_approval"
        }

        async fn inspect(
            &self,
            _session_id: &str,
            tool_requests: &[ToolRequest],
            _messages: &[Message],
            _gosling_mode: GoslingMode,
        ) -> Result<Vec<InspectionResult>> {
            Ok(tool_requests
                .iter()
                .map(|request| InspectionResult {
                    tool_request_id: request.id.clone(),
                    action: InspectionAction::RequireApproval(Some("suspicious".to_string())),
                    reason: "test finding".to_string(),
                    confidence: 0.6,
                    inspector_name: self.name().to_string(),
                    finding_id: None,
                    metadata: None,
                })
                .collect())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    async fn test_auto_mode_downgrades_require_approval_to_allow() {
        let mut manager = ToolInspectionManager::new();
        manager.add_inspector(Box::new(RequireApprovalInspector));

        let tool_request = ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("shell").with_arguments(object!({}))),
            metadata: None,
            tool_meta: None,
        };

        let results = manager
            .inspect_tools(
                "session",
                std::slice::from_ref(&tool_request),
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, InspectionAction::Allow);

        let results = manager
            .inspect_tools(
                "session",
                std::slice::from_ref(&tool_request),
                &[],
                GoslingMode::SmartApprove,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            results[0].action,
            InspectionAction::RequireApproval(_)
        ));
    }

    struct FailClosedApprovalInspector;

    #[async_trait]
    impl ToolInspector for FailClosedApprovalInspector {
        fn name(&self) -> &'static str {
            "fail_closed"
        }

        fn auto_downgrades_require_approval(&self) -> bool {
            false
        }

        async fn inspect(
            &self,
            _session_id: &str,
            tool_requests: &[ToolRequest],
            _messages: &[Message],
            _gosling_mode: GoslingMode,
        ) -> Result<Vec<InspectionResult>> {
            Ok(tool_requests
                .iter()
                .enumerate()
                .map(|(index, request)| InspectionResult {
                    tool_request_id: request.id.clone(),
                    action: if index == 0 {
                        InspectionAction::Deny
                    } else {
                        InspectionAction::RequireApproval(Some("fallback".to_string()))
                    },
                    reason: "test result".to_string(),
                    confidence: 1.0,
                    inspector_name: self.name().to_string(),
                    finding_id: None,
                    metadata: None,
                })
                .collect())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    async fn test_auto_mode_preserves_fail_closed_approval() {
        let mut manager = ToolInspectionManager::new();
        manager.add_inspector(Box::new(FailClosedApprovalInspector));
        let requests = [
            ToolRequest {
                id: "req_1".to_string(),
                tool_call: Ok(CallToolRequestParams::new("first").with_arguments(object!({}))),
                metadata: None,
                tool_meta: None,
            },
            ToolRequest {
                id: "req_2".to_string(),
                tool_call: Ok(CallToolRequestParams::new("second").with_arguments(object!({}))),
                metadata: None,
                tool_meta: None,
            },
        ];

        let results = manager
            .inspect_tools("session", &requests, &[], GoslingMode::Auto)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].action, InspectionAction::Deny);
        assert!(matches!(
            results[1].action,
            InspectionAction::RequireApproval(_)
        ));
    }

    struct FailingInspector;

    #[async_trait]
    impl ToolInspector for FailingInspector {
        fn name(&self) -> &'static str {
            "failing"
        }

        async fn inspect(
            &self,
            _session_id: &str,
            _tool_requests: &[ToolRequest],
            _messages: &[Message],
            _gosling_mode: GoslingMode,
        ) -> Result<Vec<InspectionResult>> {
            Err(anyhow::anyhow!("inspector boom"))
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    async fn test_inspector_failure_fails_closed() {
        let mut manager = ToolInspectionManager::new();
        manager.add_inspector(Box::new(FailingInspector));

        let tool_request = ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("test_tool").with_arguments(object!({}))),
            metadata: None,
            tool_meta: None,
        };

        // Even in Auto mode (baseline Allow), a failing inspector must not let
        // the tool through unjudged: inspect_tools returns Ok with a synthesized
        // RequireApproval rather than dropping the verdict.
        let results = manager
            .inspect_tools(
                "session",
                std::slice::from_ref(&tool_request),
                &[],
                GoslingMode::Auto,
            )
            .await
            .expect("inspect_tools should not surface the inspector error");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_request_id, "req_1");
        assert!(matches!(
            results[0].action,
            InspectionAction::RequireApproval(_)
        ));
    }
}
