use super::{asked, header, reader, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{AppKeys, Dated, Failure};

const OURS: &str = "abababababababababababababababab";

const LISTED: &str = r#"{"Items":[{"AppName":"Seerr","AccessToken":"cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd"},{"AppName":"lemonfiber","AccessToken":"abababababababababababababababab","DateCreated":"2026-11-06T00:00:00Z"},{"AppName":"lemonfiber","AccessToken":""}],"TotalRecordCount":0}"#;

const LISTED_AND_NEW: &str = r#"{"Items":[{"AppName":"lemonfiber","AccessToken":"abababababababababababababababab"},{"AppName":"lemonfiber","AccessToken":"efefefefefefefefefefefefefefefef"}]}"#;

fn listing() -> std::sync::Arc<Fake> {
    Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("/Auth/Keys", Answer::reply(200, LISTED)),
    ])
}

#[tokio::test]
async fn the_keys_filed_under_an_app_are_its_own_and_dated_as_emby_made_them() {
    let fake = listing();
    let emby = reader(&fake);
    assert_eq!(
        emby.filed_as("lemonfiber").await.ok(),
        Some(vec![OURS.to_owned()])
    );
    assert_eq!(
        emby.dated("lemonfiber").await.ok(),
        Some(vec![Dated {
            created: Some("2026-11-06T00:00:00Z".to_owned()),
            last_used: None
        }])
    );
    assert_eq!(emby.astray("lemonfiber").await.ok(), Some(Vec::new()));
    let unread = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, "not json"),
    ]);
    assert!(reader(&unread).astray("lemonfiber").await.is_err());
}

#[tokio::test]
async fn a_minted_key_is_the_one_new_key_under_the_app() {
    let fake = Fake::by_route_in_turn(vec![
        (
            Method::Post,
            "/Users/AuthenticateByName",
            vec![Answer::reply(200, SIGNED_IN)],
        ),
        (Method::Post, "/Auth/Keys", vec![Answer::reply(204, "")]),
        (
            Method::Get,
            "/Auth/Keys",
            vec![
                Answer::reply(200, LISTED),
                Answer::reply(200, LISTED_AND_NEW),
            ],
        ),
    ]);
    assert_eq!(
        reader(&fake).mint("lemonfiber").await.ok(),
        Some("efefefefefefefefefefefefefefefef".to_owned())
    );
    assert!(asked(&fake, Method::Post, "/Auth/Keys?App=lemonfiber").is_some());
}

#[tokio::test]
async fn a_mint_that_shows_no_one_new_key_is_refused() {
    let fake = Fake::by_route_in_turn(vec![
        (
            Method::Post,
            "/Users/AuthenticateByName",
            vec![Answer::reply(200, SIGNED_IN)],
        ),
        (Method::Post, "/Auth/Keys", vec![Answer::reply(204, "")]),
        (Method::Get, "/Auth/Keys", vec![Answer::reply(200, LISTED)]),
    ]);
    assert!(matches!(
        reader(&fake).mint("lemonfiber").await,
        Err(Failure::Refused { .. })
    ));
    let refused = Fake::by_route_in_turn(vec![
        (
            Method::Post,
            "/Users/AuthenticateByName",
            vec![Answer::reply(200, SIGNED_IN)],
        ),
        (Method::Post, "/Auth/Keys", vec![Answer::reply(400, "")]),
        (Method::Get, "/Auth/Keys", vec![Answer::reply(200, LISTED)]),
    ]);
    assert!(reader(&refused).mint("lemonfiber").await.is_err());
}

#[tokio::test]
async fn a_key_is_proved_by_emby_answering_to_it_alone_and_revoked_by_name() {
    let fake = Fake::by_rules(vec![
        (
            None,
            "/Users/AuthenticateByName",
            Answer::reply(200, SIGNED_IN),
        ),
        (Some(Method::Get), "/System/Info", Answer::reply(200, "{}")),
        (Some(Method::Delete), "/Auth/Keys/", Answer::reply(204, "")),
    ]);
    let emby = reader(&fake);
    assert!(emby.answers_to(OURS).await.is_ok());
    let proving = asked(&fake, Method::Get, "/System/Info");
    assert_eq!(
        proving
            .as_ref()
            .and_then(|asked| header(asked, "X-Emby-Token")),
        Some(OURS.to_owned())
    );
    assert!(emby.revoke(OURS).await.is_ok());
    assert!(asked(&fake, Method::Delete, &format!("/Auth/Keys/{OURS}")).is_some());
    let silent = Fake::silent();
    for key in ["", "a/b"] {
        assert!(matches!(
            reader(&silent).answers_to(key).await,
            Err(Failure::Refused { .. })
        ));
        assert!(matches!(
            reader(&silent).revoke(key).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert!(silent.requests().is_empty());
}
