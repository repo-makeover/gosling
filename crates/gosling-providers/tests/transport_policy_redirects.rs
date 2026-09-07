//! ARC-GSL-006: a redirect must not carry provider credentials somewhere the
//! validated base URL never authorized.
//!
//! `reqwest` strips `Authorization` across an origin change, but vendor API-key
//! headers (`x-api-key`, `api-key`, `x-goog-api-key`) are ordinary headers it
//! knows nothing about, so without a policy they would follow a redirect to
//! whatever host the provider named.

use gosling_providers::api_client::inference_client_builder;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `reqwest` wraps a redirect-policy rejection, so the policy's own message
/// only shows up by walking the error's source chain.
fn full_chain(error: &reqwest::Error) -> String {
    let mut chain = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        chain.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    chain
}

fn client() -> reqwest::Client {
    inference_client_builder(Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::test]
async fn a_same_origin_redirect_is_followed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/start"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/end"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/end"))
        .respond_with(ResponseTemplate::new(200).set_body_string("arrived"))
        .mount(&server)
        .await;

    let response = client()
        .get(format!("{}/start", server.uri()))
        .send()
        .await
        .expect("a same-origin redirect is ordinary provider behavior");
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "arrived");
}

#[tokio::test]
async fn a_redirect_to_another_origin_is_refused() {
    let elsewhere = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/collect"))
        .respond_with(ResponseTemplate::new(200).set_body_string("credentials"))
        .mount(&elsewhere)
        .await;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/start"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/collect", elsewhere.uri()).as_str()),
        )
        .mount(&server)
        .await;

    let error = client()
        .get(format!("{}/start", server.uri()))
        .send()
        .await
        .expect_err("a cross-origin redirect must not be followed");
    let chain = full_chain(&error);
    assert!(
        chain.contains("different origin"),
        "unexpected error: {chain}"
    );
    assert_eq!(
        elsewhere.received_requests().await.unwrap().len(),
        0,
        "the redirect target must never be contacted"
    );
}

#[tokio::test]
async fn a_redirect_loop_is_capped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/loop"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/loop"))
        .mount(&server)
        .await;

    let error = client()
        .get(format!("{}/loop", server.uri()))
        .send()
        .await
        .expect_err("a redirect loop must terminate");
    let chain = full_chain(&error);
    assert!(
        chain.contains("redirected more than"),
        "unexpected error: {chain}"
    );
}
