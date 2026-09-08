//! Shared name-based classification for permission and security inspectors.
//!
//! Shared predicates keep inspectors from disagreeing about a recognized tool's
//! authority. They match known names and extension suffixes, not call arguments;
//! an unrecognized name is not proof of read-only behavior. (INV-GSL-004)

/// Bare tool names that execute an arbitrary command line.
const SHELL_TOOL_NAMES: &[&str] = &[
    "shell",
    "bash",
    "execute_command",
    "run_command",
    "terminal",
];

/// Suffixes an MCP extension appends when it re-exports a shell tool
/// (`<extension>__shell`).
const SHELL_TOOL_SUFFIXES: &[&str] = &[
    "__shell",
    "__bash",
    "__terminal",
    "__execute_command",
    "__run_command",
];

/// Tools that run code or drive the machine without looking like a shell.
/// `computercontroller__automation_script` writes a 0o755 temp script and
/// executes it, so it carries shell authority under a different name.
const CODE_EXECUTION_TOOL_NAMES: &[&str] = &[
    "computercontroller__automation_script",
    "computercontroller__computer_control",
];

/// Bare tool names that mutate the filesystem.
const WRITE_TOOL_NAMES: &[&str] = &["write", "edit", "str_replace", "create", "patch", "apply"];

const WRITE_TOOL_SUFFIXES: &[&str] = &[
    "__write",
    "__edit",
    "__str_replace",
    "__create",
    "__patch",
    "__apply",
];

const EGRESS_TOOL_NAMES: &[&str] = &[
    "web_fetch",
    "web_scrape",
    "fetch",
    "browser_navigate",
    "http_request",
];

const EGRESS_TOOL_SUFFIXES: &[&str] = &[
    "__web_fetch",
    "__web_scrape",
    "__fetch",
    "__browser_navigate",
    "__http_request",
];

const EXTENSION_MANAGEMENT_TOOL_NAMES: &[&str] =
    &["manage_extensions", "extensionmanager__manage_extensions"];

const MIXED_RISK_TOOL_NAMES: &[&str] = &[
    "computercontroller__cache",
    "computercontroller__xlsx_tool",
    "computercontroller__docx_tool",
    "computercontroller__pdf_tool",
];

fn matches_name_or_suffix(
    tool_name: &str,
    bare_names: &[&str],
    extension_suffixes: &[&str],
) -> bool {
    // Match an extension's complete tool suffix, not a substring elsewhere in its name.
    bare_names.contains(&tool_name)
        || extension_suffixes
            .iter()
            .any(|suffix| tool_name.ends_with(suffix))
}

/// A tool that executes an arbitrary command line.
pub fn is_shell_tool(name: &str) -> bool {
    matches_name_or_suffix(name, SHELL_TOOL_NAMES, SHELL_TOOL_SUFFIXES)
}

/// A tool that runs code or drives the machine, shell-named or not.
pub fn is_code_execution_tool(name: &str) -> bool {
    is_shell_tool(name) || CODE_EXECUTION_TOOL_NAMES.contains(&name)
}

/// A tool that mutates the filesystem.
pub fn is_write_tool(name: &str) -> bool {
    matches_name_or_suffix(name, WRITE_TOOL_NAMES, WRITE_TOOL_SUFFIXES)
}

/// Matches recognized network tool names; destination and transfer direction are inspected separately.
pub fn is_egress_tool(name: &str) -> bool {
    matches_name_or_suffix(name, EGRESS_TOOL_NAMES, EGRESS_TOOL_SUFFIXES)
}

/// Known side-effecting or mixed-action tools that need an explicit permission in Auto.
/// Enabling their extension alone must not grant that authority. (SEC-GOS-003)
pub fn requires_explicit_grant_in_auto(name: &str) -> bool {
    is_code_execution_tool(name)
        || is_write_tool(name)
        || is_egress_tool(name)
        || EXTENSION_MANAGEMENT_TOOL_NAMES.contains(&name)
        || MIXED_RISK_TOOL_NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_and_namespaced_shell_tools_are_shell() {
        assert!(is_shell_tool("shell"));
        assert!(is_shell_tool("bash"));
        assert!(is_shell_tool("developer__shell"));
        assert!(is_shell_tool("some_ext__run_command"));
        assert!(!is_shell_tool("read"));
    }

    #[test]
    fn automation_script_is_code_execution_without_being_shell_named() {
        assert!(!is_shell_tool("computercontroller__automation_script"));
        assert!(is_code_execution_tool(
            "computercontroller__automation_script"
        ));
    }

    #[test]
    fn substring_lookalikes_are_not_shell_tools() {
        // The old `contains("shell")` predicate matched these.
        assert!(!is_shell_tool("read_shellcheck_report"));
        assert!(!is_shell_tool("list_commands"));
    }

    #[test]
    fn write_tools_are_recognized_bare_and_namespaced() {
        assert!(is_write_tool("write"));
        assert!(is_write_tool("developer__edit"));
        assert!(!is_write_tool("read"));
    }

    #[test]
    fn auto_grant_gate_covers_execution_and_write_but_not_read() {
        assert!(requires_explicit_grant_in_auto("shell"));
        assert!(requires_explicit_grant_in_auto("developer__edit"));
        assert!(requires_explicit_grant_in_auto(
            "computercontroller__automation_script"
        ));
        assert!(requires_explicit_grant_in_auto("web_fetch"));
        assert!(requires_explicit_grant_in_auto("network__http_request"));
        assert!(requires_explicit_grant_in_auto(
            "extensionmanager__manage_extensions"
        ));
        assert!(requires_explicit_grant_in_auto("computercontroller__cache"));
        assert!(!requires_explicit_grant_in_auto("read"));
        assert!(!requires_explicit_grant_in_auto("developer__read"));
    }

    #[test]
    fn egress_tools_are_recognized_bare_and_namespaced() {
        assert!(is_egress_tool("web_fetch"));
        assert!(is_egress_tool("computercontroller__web_scrape"));
        assert!(is_egress_tool("network__http_request"));
        assert!(!is_egress_tool("read_http_cache"));
    }
}
