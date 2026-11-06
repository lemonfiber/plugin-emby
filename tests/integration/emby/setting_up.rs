use super::{a_word, asked, emby, reader, sent, ADMIN, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, MediaServer};

const POLICY: &str = r#"{"Id":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3","Policy":{"IsAdministrator":true,"IsHidden":false}}"#;

#[tokio::test]
async fn a_wizard_that_answers_anybody_is_not_done() {
    let fake = Fake::in_turn(vec![Answer::reply(200, r#"{"UICulture":"en-us"}"#)]);
    assert_eq!(emby(&fake).startup_completed().await.ok(), Some(false));
    assert!(asked(&fake, Method::Get, "/Startup/Configuration").is_some());
}

#[tokio::test]
async fn a_wizard_that_answers_only_an_administrator_is_done() {
    let fake = Fake::in_turn(vec![Answer::reply(
        401,
        "Access token is invalid or expired.",
    )]);
    assert_eq!(emby(&fake).startup_completed().await.ok(), Some(true));
}

#[tokio::test]
async fn any_other_answer_from_the_wizard_is_refused_or_unavailable() {
    let refused = Fake::in_turn(vec![Answer::reply(500, "")]);
    assert!(matches!(
        emby(&refused).startup_completed().await,
        Err(Failure::Refused { detail, .. }) if detail == "HTTP 500"
    ));
    for unavailable in [Fake::in_turn(vec![Answer::reply(503, "")]), Fake::silent()] {
        assert!(matches!(
            emby(&unavailable).startup_completed().await,
            Err(Failure::Unavailable { .. })
        ));
    }
}

fn first_run() -> std::sync::Arc<Fake> {
    Fake::in_turn(vec![
        Answer::reply(200, r#"{"Name":"bin"}"#),
        Answer::reply(200, "{}"),
        Answer::reply(204, ""),
        Answer::reply(204, ""),
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, POLICY),
        Answer::reply(204, ""),
    ])
}

#[tokio::test]
async fn the_first_run_names_the_administrator_turns_port_mapping_off_and_hides_the_account() {
    let password = a_word();
    let fake = first_run();

    assert!(emby(&fake).create_admin("admin", &password).await.is_ok());

    let paths: Vec<(Method, String)> = fake
        .requests()
        .into_iter()
        .map(|asked| (asked.method, asked.url.replace("http://127.0.0.1:8096", "")))
        .collect();
    assert_eq!(
        paths,
        [
            (Method::Get, "/Startup/User".to_owned()),
            (Method::Post, "/Startup/User".to_owned()),
            (Method::Post, "/Startup/RemoteAccess".to_owned()),
            (Method::Post, "/Startup/Complete".to_owned()),
            (Method::Post, "/Users/AuthenticateByName".to_owned()),
            (Method::Get, format!("/Users/{ADMIN}")),
            (Method::Post, format!("/Users/{ADMIN}/Policy")),
        ]
    );
    assert_eq!(
        sent(&fake, Method::Post, "/Startup/User"),
        serde_json::json!({ "Name": "admin", "Password": password })
    );
    assert_eq!(
        sent(&fake, Method::Post, "/Startup/RemoteAccess"),
        serde_json::json!({ "EnableAutomaticPortMapping": false })
    );
    assert_eq!(
        sent(&fake, Method::Post, "/Policy"),
        serde_json::json!({ "IsAdministrator": true, "IsHidden": true })
    );
    let hiding = asked(&fake, Method::Post, "/Policy");
    assert_eq!(
        hiding.and_then(|asked| super::header(&asked, "X-Emby-Token")),
        Some("token".to_owned())
    );
}

#[tokio::test]
async fn a_first_run_step_refused_stops_the_run() {
    for refused_at in 0..4 {
        let mut answers = vec![
            Answer::reply(200, r#"{"Name":"bin"}"#),
            Answer::reply(200, "{}"),
            Answer::reply(204, ""),
            Answer::reply(204, ""),
        ];
        if let Some(answer) = answers.get_mut(refused_at) {
            *answer = Answer::reply(500, "");
        }
        let fake = Fake::in_turn(answers);
        assert!(emby(&fake).create_admin("admin", &a_word()).await.is_err());
        assert_eq!(fake.requests().len(), refused_at + 1);
    }
}

#[tokio::test]
async fn an_administrator_whose_account_carries_no_policy_is_not_hidden_and_says_so() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, r#"{"Name":"bin"}"#),
        Answer::reply(200, "{}"),
        Answer::reply(204, ""),
        Answer::reply(204, ""),
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, r#"{"Id":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3"}"#),
    ]);
    assert!(matches!(
        emby(&fake).create_admin("admin", &a_word()).await,
        Err(Failure::Refused { detail, .. }) if detail == "the account carries no policy"
    ));
}

#[tokio::test]
async fn a_sign_in_that_opens_no_session_or_names_no_account_is_refused() {
    for opened in [
        r#"{"AccessToken":"","User":{"Id":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3"}}"#,
        r#"{"AccessToken":"token","User":{"Id":"../Users"}}"#,
    ] {
        let fake = Fake::in_turn(vec![
            Answer::reply(200, r#"{"Name":"bin"}"#),
            Answer::reply(200, "{}"),
            Answer::reply(204, ""),
            Answer::reply(204, ""),
            Answer::reply(200, opened),
        ]);
        assert!(matches!(
            emby(&fake).create_admin("admin", &a_word()).await,
            Err(Failure::Refused { .. })
        ));
    }
}

#[tokio::test]
async fn the_password_is_replaced_signed_in_with_the_one_in_force_and_proved_with_the_new_one() {
    let replacement = a_word();
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(204, ""),
        Answer::reply(200, SIGNED_IN),
    ]);

    assert!(reader(&fake)
        .replace_admin_password(&replacement)
        .await
        .is_ok());

    assert_eq!(
        sent(&fake, Method::Post, &format!("/Users/{ADMIN}/Password")),
        serde_json::json!({ "NewPw": replacement })
    );
    let proved = fake
        .requests()
        .into_iter()
        .filter(|asked| asked.url.ends_with("/Users/AuthenticateByName"))
        .filter_map(|asked| asked.body)
        .next_back()
        .unwrap_or_default();
    assert!(proved.contains(&replacement));
}

#[tokio::test]
async fn a_replacement_emby_did_not_take_is_refused() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(204, ""),
        Answer::reply(401, ""),
    ]);
    assert!(matches!(
        reader(&fake).replace_admin_password("new").await,
        Err(Failure::Refused { detail, .. }) if detail == "Emby did not take the new password at a sign-in"
    ));
    let unreachable = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(204, ""),
        Answer::Silent,
    ]);
    assert!(matches!(
        reader(&unreachable).replace_admin_password("new").await,
        Err(Failure::Unavailable { .. })
    ));
}

#[tokio::test]
async fn no_replacement_is_tried_without_a_password_or_a_credential() {
    let fake = Fake::silent();
    assert!(matches!(
        reader(&fake).replace_admin_password("").await,
        Err(Failure::Refused { detail, .. }) if detail == "no replacement password was given"
    ));
    assert!(matches!(
        emby(&fake).replace_admin_password("new").await,
        Err(Failure::Refused { detail, .. }) if detail.starts_with("this adapter holds no credential for Emby")
    ));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_change_emby_refuses_is_not_proved() {
    let fake = Fake::in_turn(vec![Answer::reply(200, SIGNED_IN), Answer::reply(400, "")]);
    assert!(reader(&fake).replace_admin_password("new").await.is_err());
    assert_eq!(fake.requests().len(), 2);
}
