//! Session handoff: a fresh session seeded with a continuation briefing
//! instead of the source session's full conversation.
//!
//! Maintainers: unlike `fork_session`, the new session is left dormant —
//! resuming it (session/load, which every navigation into an existing
//! session already goes through) is what activates it, so this handler
//! doesn't duplicate that activation logic.

use super::*;
use crate::conversation::Conversation;

impl GoslingAcpAgent {
    pub(super) async fn on_handoff_session(
        &self,
        req: HandoffSessionRequest,
    ) -> Result<HandoffSessionResponse, agent_client_protocol::Error> {
        let source_session_id = req.session_id.trim();
        if source_session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        let source = self
            .session_manager
            .get_session(source_session_id, true)
            .await
            .internal_err()?;
        let conversation = source.conversation.clone().unwrap_or_default();

        let agent = self.get_session_agent(source_session_id).await?;
        let provider = agent.provider().await.internal_err()?;

        let handoff_summary = if provider.manages_own_context() {
            // Gosling's own message array is a display-only mirror for these
            // providers (see manages_own_context's doc comment) — there's no
            // real history here to summarize. The new session still carries
            // over the same settings, just without a generated briefing.
            None
        } else {
            let model_config = agent
                .model_config_for_session(source_session_id)
                .await
                .internal_err()?;
            let (summary, _usage) = crate::context_mgmt::generate_handoff_summary(
                provider.as_ref(),
                &model_config,
                source_session_id,
                &conversation,
            )
            .await
            .internal_err()?;
            Some(summary)
        };

        let handoff_name = if source.name.trim().is_empty() {
            "(handoff)".to_string()
        } else {
            format!("{} (handoff)", source.name)
        };
        let new_session = self
            .session_manager
            .copy_session(source_session_id, handoff_name)
            .await
            .internal_err()?;

        if let Err(error) = self
            .session_manager
            .replace_conversation(&new_session.id, &Conversation::new_unvalidated(Vec::new()))
            .await
        {
            self.cleanup_failed_new_session(&new_session.id).await;
            return Err(error).internal_err();
        }

        Ok(HandoffSessionResponse {
            session_id: new_session.id,
            handoff_summary,
        })
    }
}
