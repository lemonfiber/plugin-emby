use super::{asked, reader, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{AppKeys, Failure, Fronted};

fn unsupported<T>(answer: &Result<T, Failure>) -> bool {
    matches!(answer, Err(Failure::Unsupported { .. }))
}

#[tokio::test]
async fn what_emby_holds_no_setting_for_is_unsupported_and_emby_is_not_asked() {
    let fake = Fake::silent();
    let emby = reader(&fake);
    assert!(unsupported(&emby.known_proxies().await));
    assert!(unsupported(&emby.trust_only("10.0.0.2").await));
    assert!(unsupported(&emby.allowed_origins().await));
    assert!(unsupported(&emby.allow_only("https://watch.example").await));
    assert!(unsupported(&emby.account("lemonfiber").await));
    assert!(unsupported(&emby.listed_as().await));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn emby_is_asked_to_start_again() {
    let fake = Fake::in_turn(vec![Answer::reply(200, SIGNED_IN), Answer::reply(204, "")]);
    assert!(reader(&fake).restart().await.is_ok());
    assert!(asked(&fake, Method::Post, "/System/Restart").is_some());
}
