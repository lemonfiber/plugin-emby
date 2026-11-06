use super::{reader, sent, MEMBER, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Allowed, Failure, Household, Unrated};

const HELD: &str = r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Policy":{"IsHidden":true,"EnableAllFolders":true,"EnabledFolders":[],"InvalidLoginAttemptCount":3,"LockedOutDate":1700000000000,"IsDisabled":true,"SimultaneousStreamLimit":2}}"#;

fn written(answers: Vec<Answer>) -> std::sync::Arc<Fake> {
    let mut all = vec![Answer::reply(200, SIGNED_IN), Answer::reply(200, HELD)];
    all.extend(answers);
    Fake::in_turn(all)
}

fn policy(fake: &Fake) -> serde_json::Value {
    sent(fake, Method::Post, &format!("/Users/{MEMBER}/Policy"))
}

#[tokio::test]
async fn what_an_account_may_watch_is_written_over_its_own_policy_with_the_age_as_emby_s_level() {
    let fake = written(vec![Answer::reply(204, "")]);
    let allowed = Allowed {
        libraries: Some(vec!["a2a9b93ad0ac47e5b9640b87906d04f4".to_owned()]),
        age_limit: Some(12),
        unrated: Some(Unrated::HeldBack),
    };
    assert!(reader(&fake).allow(MEMBER, &allowed).await.is_ok());
    assert_eq!(
        policy(&fake),
        serde_json::json!({
            "IsHidden": true,
            "EnableAllFolders": false,
            "EnabledFolders": ["a2a9b93ad0ac47e5b9640b87906d04f4"],
            "MaxParentalRating": 7,
            "BlockUnratedItems": ["Movie","Trailer","Series","Music","Game","Book","LiveTvChannel","LiveTvProgram","ChannelContent","Other"],
            "InvalidLoginAttemptCount": 3,
            "LockedOutDate": 1_700_000_000_000_i64,
            "IsDisabled": true,
            "SimultaneousStreamLimit": 2
        })
    );
}

#[tokio::test]
async fn nothing_named_leaves_the_policy_as_it_was_and_unrated_content_may_be_let_through() {
    let fake = written(vec![Answer::reply(204, "")]);
    let allowed = Allowed {
        libraries: None,
        age_limit: None,
        unrated: Some(Unrated::LetThrough),
    };
    assert!(reader(&fake).allow(MEMBER, &allowed).await.is_ok());
    let mut expected: serde_json::Value = serde_json::from_str(HELD).unwrap_or_default();
    if let Some(held) = expected
        .get_mut("Policy")
        .and_then(serde_json::Value::as_object_mut)
    {
        held.insert("BlockUnratedItems".to_owned(), serde_json::json!([]));
    }
    assert_eq!(Some(&policy(&fake)), expected.get("Policy"));
}

#[tokio::test]
async fn a_claimable_account_is_switched_on_with_its_wrong_passwords_and_lockout_forgotten() {
    let fake = written(vec![Answer::reply(204, "")]);
    let allowed = Allowed {
        libraries: Some(Vec::new()),
        age_limit: None,
        unrated: None,
    };
    assert!(reader(&fake).claimable(MEMBER, &allowed).await.is_ok());
    let written = policy(&fake);
    assert_eq!(written.get("IsDisabled"), Some(&serde_json::json!(false)));
    assert_eq!(
        written.get("InvalidLoginAttemptCount"),
        Some(&serde_json::json!(0))
    );
    assert_eq!(written.get("LockedOutDate"), Some(&serde_json::json!(0)));
    assert_eq!(
        written.get("EnableAllFolders"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(
        written.get("EnabledFolders"),
        Some(&serde_json::json!(serde_json::json!([])))
    );
    assert_eq!(written.get("IsHidden"), Some(&serde_json::json!(true)));
}

#[tokio::test]
async fn a_suspended_account_is_switched_off_and_kept() {
    let fake = written(vec![Answer::reply(204, "")]);
    assert!(reader(&fake).suspend(MEMBER).await.is_ok());
    assert_eq!(
        policy(&fake).get("IsDisabled"),
        Some(&serde_json::json!(true))
    );
}

#[tokio::test]
async fn a_policy_write_emby_refuses_is_an_error_and_an_account_without_one_is_refused() {
    let refused = written(vec![Answer::reply(400, "")]);
    assert!(reader(&refused).suspend(MEMBER).await.is_err());
    let bare = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(
            200,
            r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Policy":[]}"#,
        ),
    ]);
    assert!(matches!(
        reader(&bare).suspend(MEMBER).await,
        Err(Failure::Refused { .. })
    ));
    let silent = Fake::silent();
    assert!(matches!(
        reader(&silent).suspend("../Users").await,
        Err(Failure::Refused { .. })
    ));
    assert!(silent.requests().is_empty());
}
