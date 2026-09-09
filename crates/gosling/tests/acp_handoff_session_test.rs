#[allow(dead_code)]
#[path = "acp_common_tests/mod.rs"]
mod common_tests;

use common_tests::fixtures::server::AcpServerConnection;
use common_tests::fixtures::{
    run_test, send_custom, Connection, OpenAiFixture, Session, TestConnectionConfig,
};
use gosling::conversation::message::Message;
use gosling::session::SessionManager;

const HANDOFF_RESPONSE: &str = concat!(
    "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":1766229303,",
    "\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",",
    "\"content\":\"Goal: finish the thing.\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":1766229303,",
    "\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":1766229303,",
    "\"model\":\"gpt-4o\",\"choices\":[],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":10,",
    "\"total_tokens\":110}}\n\n",
    "data: [DONE]\n\n"
);

async fn new_connection(data_root: &std::path::Path) -> AcpServerConnection {
    let openai = OpenAiFixture::new(
        vec![(String::new(), HANDOFF_RESPONSE)],
        <AcpServerConnection as Connection>::expected_session_id(),
    )
    .await;
    <AcpServerConnection as Connection>::new(
        TestConnectionConfig {
            data_root: data_root.to_path_buf(),
            ..Default::default()
        },
        openai,
    )
    .await
}

#[test]
fn handoff_session_returns_a_summary_and_a_history_free_new_session() {
    run_test(async {
        let data_root = tempfile::tempdir().unwrap();
        let mut conn = new_connection(data_root.path()).await;
        let session_manager = SessionManager::new(data_root.path().to_path_buf());

        let session_data = conn.new_session().await.unwrap();
        let session_id = session_data.session.session_id().0.to_string();
        session_manager
            .add_message(
                &session_id,
                &Message::user().with_text("let's do the thing"),
            )
            .await
            .unwrap();
        session_manager
            .add_message(
                &session_id,
                &Message::assistant().with_text("sure, working on it"),
            )
            .await
            .unwrap();

        let response = send_custom(
            conn.cx(),
            "_gosling/unstable/session/handoff",
            serde_json::json!({ "sessionId": session_id }),
        )
        .await
        .unwrap();

        let new_session_id = response
            .get("sessionId")
            .and_then(|v| v.as_str())
            .expect("missing sessionId")
            .to_string();
        assert_ne!(new_session_id, session_id);
        assert_eq!(
            response.get("handoffSummary").and_then(|v| v.as_str()),
            Some("Goal: finish the thing.")
        );

        let new_session = session_manager
            .get_session(&new_session_id, true)
            .await
            .unwrap();
        assert!(new_session
            .conversation
            .map(|c| c.messages().is_empty())
            .unwrap_or(true));

        // The original session's own conversation is untouched.
        let original = session_manager
            .get_session(&session_id, true)
            .await
            .unwrap();
        assert_eq!(original.conversation.unwrap().messages().len(), 2);
    });
}

#[test]
fn handoff_session_rejects_an_empty_session_id() {
    run_test(async {
        let data_root = tempfile::tempdir().unwrap();
        let conn = new_connection(data_root.path()).await;

        let error = send_custom(
            conn.cx(),
            "_gosling/unstable/session/handoff",
            serde_json::json!({ "sessionId": "" }),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("sessionId"));
    });
}
