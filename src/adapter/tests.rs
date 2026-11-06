use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lemonfiber_contract::adapter::{about_path, ready_path, About};
use lemonfiber_fixtures::http::Answer;
use lemonfiber_fixtures::http::Fake;
use tower::ServiceExt as _;

use super::{about, kit, DIGEST, RELEASE, UPSTREAM};
use crate::credential::Credential;
use crate::emby::Emby;

fn a_key() -> String {
    ('e'..='t').collect()
}

fn emby(fake: &Arc<Fake>) -> Arc<Emby> {
    let held = Credential {
        username: "admin".to_owned(),
        password: ('a'..='p').collect(),
    };
    Arc::new(Emby::new(fake.clone(), "http://emby:8096", Arc::new(held)))
}

async fn answered_by(
    fake: &Arc<Fake>,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    let Ok(request) = request.body(Body::from("{}")) else {
        return (StatusCode::IM_A_TEAPOT, Vec::new());
    };
    let Ok(response) = kit(a_key(), emby(fake)).router().oneshot(request).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default();
    (status, bytes)
}

fn keyed(key: &str) -> Vec<(&'static str, String)> {
    vec![(header::AUTHORIZATION.as_str(), format!("Bearer {key}"))]
}

#[test]
fn it_speaks_both_contracts_and_names_the_release_its_recordings_were_taken_from() {
    let said = about();
    assert_eq!(said.speaks, ["media.serve@1", "identity.source@1"]);
    assert_eq!(said.upstream, UPSTREAM);
    assert_eq!(
        said.releases
            .iter()
            .map(|release| (release.version.as_str(), release.digest.as_str()))
            .collect::<Vec<_>>(),
        [(RELEASE, DIGEST)]
    );
}

#[tokio::test]
async fn about_is_answered_under_the_plugin_s_key_and_nothing_without_it() {
    let (status, body) = answered_by(&Fake::silent(), "GET", &about_path(), &keyed(&a_key())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<About>(&body).ok(), Some(about()));
    for headers in [Vec::new(), keyed("not-the-plugin-s-key")] {
        let (status, _) = answered_by(
            &Fake::silent(),
            "POST",
            "/lemonfiber/identity.source/v1/startup_completed",
            &headers,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn an_unreachable_emby_is_answered_as_the_refusal_the_contract_declares() {
    let (status, body) = answered_by(
        &Fake::silent(),
        "POST",
        "/lemonfiber/identity.source/v1/startup_completed",
        &keyed(&a_key()),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let refusal: Option<serde_json::Value> = serde_json::from_slice(&body).ok();
    assert_eq!(
        refusal.and_then(|refusal| refusal.get("type").cloned()),
        Some(serde_json::json!("unavailable"))
    );
}

#[tokio::test]
async fn it_is_ready_while_emby_names_its_server_and_version() {
    let up = Fake::always(Answer::reply(
        200,
        r#"{"Id":"63f4c05487a44959a498affc4d145bd7","Version":"4.10.2.0","ServerName":"nas"}"#,
    ));
    let (status, body) = answered_by(&up, "GET", &ready_path(), &keyed(&a_key())).await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"true".as_slice())
    );
    assert!(up
        .requests()
        .iter()
        .all(|asked| asked.url == "http://emby:8096/System/Info/Public"));
    for down in [
        Fake::silent(),
        Fake::always(Answer::reply(503, "")),
        Fake::always(Answer::reply(200, r#"{"Id":"","Version":"4.10.2.0"}"#)),
        Fake::always(Answer::reply(200, r#"{"Id":"63f4c054","Version":""}"#)),
        Fake::always(Answer::reply(200, "not json")),
    ] {
        let (status, body) = answered_by(&down, "GET", &ready_path(), &keyed(&a_key())).await;
        assert_eq!(
            (status, body.as_slice()),
            (StatusCode::OK, b"false".as_slice())
        );
    }
}

#[tokio::test]
async fn the_guard_s_question_is_answered_with_the_member_s_token_alone() {
    let token: String = ('a'..='f').cycle().take(32).collect();
    let fake = Fake::always(Answer::reply(200, r#"{"Items":[{"Id":"8"}]}"#));
    let (status, _) = answered_by(
        &fake,
        "GET",
        "/lemonfiber/media.serve/v1/may?space=item&item=8",
        &[("X-Emby-Token", token.clone())],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = answered_by(
        &fake,
        "GET",
        "/lemonfiber/media.serve/v1/may?space=item&item=8",
        &keyed(&token),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
