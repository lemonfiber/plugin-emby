use super::{emby, header, reader, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::service::{Failure, Household};

#[tokio::test]
async fn one_sign_in_serves_every_call_until_emby_turns_its_token_away() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, "[]"),
        Answer::reply(200, "[]"),
        Answer::reply(401, ""),
        Answer::reply(
            200,
            r#"{"AccessToken":"again","User":{"Id":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3"}}"#,
        ),
        Answer::reply(200, "[]"),
    ]);
    let emby = reader(&fake);
    for _ in 0..3 {
        assert!(emby.household().await.is_ok());
    }
    let tokens: Vec<Option<String>> = fake
        .requests()
        .iter()
        .filter(|asked| asked.url.ends_with("/Users"))
        .map(|asked| header(asked, "X-Emby-Token"))
        .collect();
    assert_eq!(
        tokens,
        [Some("token"), Some("token"), Some("token"), Some("again")]
            .map(|token| token.map(str::to_owned))
    );
}

#[tokio::test]
async fn without_a_credential_nothing_is_asked() {
    let fake = Fake::silent();
    assert!(matches!(
        emby(&fake).household().await,
        Err(Failure::Refused { detail, .. }) if detail.contains("no credential")
    ));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_sign_in_emby_refuses_is_unauthorised() {
    let fake = Fake::in_turn(vec![Answer::reply(401, "")]);
    assert!(matches!(
        reader(&fake).household().await,
        Err(Failure::Unauthorised { .. })
    ));
}
