pub mod artifacts;
mod chat_history_search;
mod diagnostics;
pub mod extension_data;
pub mod import_formats;
mod last_message_snippet;
mod legacy;
pub mod library;
#[cfg(feature = "nostr")]
pub mod nostr_share;
pub mod output_revisions;
pub mod research;
pub mod session_manager;
mod session_naming;

/// Returns whether an input is a Gosling Nostr session-share deeplink.
pub fn is_session_share_deeplink(input: &str) -> bool {
    input.trim_start().starts_with("gosling://sessions/nostr")
}

pub use artifacts::{
    DiscoveredArtifact, SessionArtifact, SessionArtifactProvenance, SessionArtifactRelation,
};
pub use diagnostics::{
    config_path, generate_diagnostics, get_system_info, latest_llm_log_path,
    latest_server_log_path, read_capped, read_tail, DiagnosticsConfig, DiagnosticsError,
    DiagnosticsExtensions, DiagnosticsLevel, DiagnosticsLogs, DiagnosticsPrompt, DiagnosticsReport,
    DiagnosticsTextFile, SystemInfo,
};
pub use extension_data::{
    AcpPromptRunState, DeepResearchState, EnabledExtensionsState, ExtensionData, ExtensionState,
    ShellSkillSelectionState, SystemPromptExtra, SystemPromptExtrasState, TodoState,
};
pub use library::{
    NewSessionLibraryContent, SessionLibraryItem, SessionLibraryItemKind, SessionLibraryScope,
};
pub(crate) use session_manager::ToolOperationStart;
pub use session_manager::{
    Session, SessionArtifactPage, SessionInsights, SessionManager, SessionNameUpdate,
    SessionSummary, SessionSummaryFact, SessionSummaryStatus, SessionType, SessionUpdateBuilder,
    DEFAULT_SESSION_TAIL_LIMIT, MAX_SESSION_MESSAGE_PAGE_LIMIT,
};

#[cfg(test)]
mod tests {
    use super::is_session_share_deeplink;

    #[test]
    fn detects_session_share_deeplinks_without_transport_features() {
        assert!(is_session_share_deeplink(
            "gosling://sessions/nostr?nevent=abc&key=def"
        ));
        assert!(!is_session_share_deeplink(
            "goose://sessions/nostr?nevent=abc&key=def"
        ));
    }
}
