use super::{a_word, asked, header, reader, sent, ADMIN, MEMBER, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, Household, Invited, Signed, Unrated};

const HOUSEHOLD: &str = r#"[
    {"Id":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3","Name":"admin","HasPassword":true,
     "Policy":{"IsAdministrator":true,"EnableAllFolders":true},"LastLoginDate":"2026-11-01T00:00:00Z"},
    {"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Name":"bo","HasPassword":false,
     "Policy":{"EnableAllFolders":false,"EnabledFolders":["a2a9b93ad0ac47e5b9640b87906d04f4"],
     "MaxParentalRating":7,"BlockUnratedItems":["Movie"],"IsDisabled":true},
     "LastActivityDate":"2026-11-02T00:00:00Z","LastLoginDate":"2026-11-01T00:00:00Z"},
    {"Id":"5f53cda18c2baa0c0354bb5f9a3ecbe5","Name":"cy","HasPassword":true,
     "Policy":{"MaxParentalRating":12}}
]"#;

fn a_token() -> String {
    "0123456789abcdef".repeat(2)
}

fn signed_in_as_member() -> String {
    format!(
        r#"{{"AccessToken":"{}","User":{{"Id":"{MEMBER}"}}}}"#,
        a_token()
    )
}

fn member_signed() -> Signed {
    Signed {
        id: MEMBER.to_owned(),
        token: a_token(),
    }
}

#[tokio::test]
async fn each_account_is_read_with_what_it_may_watch_said_as_an_age() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, HOUSEHOLD),
    ]);
    let held = reader(&fake).household().await.unwrap_or_default();
    let read: Vec<_> = held
        .iter()
        .map(|member| {
            (
                member.name.as_str(),
                member.claimed,
                member.access.every_library,
                member.access.libraries.len(),
                member.access.age_limit,
                member.access.unrated,
                member.access.administrator,
                member.access.disabled,
                member.last_seen.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        read,
        [
            (
                "admin",
                true,
                true,
                0,
                None,
                Unrated::LetThrough,
                true,
                false,
                Some("2026-11-01T00:00:00Z")
            ),
            (
                "bo",
                false,
                false,
                1,
                Some(12),
                Unrated::HeldBack,
                false,
                true,
                Some("2026-11-02T00:00:00Z")
            ),
            (
                "cy",
                true,
                false,
                0,
                None,
                Unrated::LetThrough,
                false,
                false,
                None
            ),
        ]
    );
    assert!(asked(&fake, Method::Get, "/Users").is_some());
}

#[tokio::test]
async fn an_invitation_makes_an_account_with_no_password_and_hides_it() {
    let made = r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Name":"ana","HasPassword":false,"Policy":{"IsHidden":false}}"#;
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, made),
        Answer::reply(200, made),
        Answer::reply(204, ""),
    ]);
    let invited = reader(&fake).invite("ana").await;
    assert!(invited.is_ok_and(|member| member.name == "ana" && !member.claimed));
    assert_eq!(
        sent(&fake, Method::Post, "/Users/New"),
        serde_json::json!({ "Name": "ana" })
    );
    assert_eq!(
        sent(&fake, Method::Post, &format!("/Users/{MEMBER}/Policy")),
        serde_json::json!({ "IsHidden": true })
    );
}

#[tokio::test]
async fn an_invitation_emby_answers_unreadably_is_refused() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, "[]"),
    ]);
    assert!(matches!(
        reader(&fake).invite("ana").await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn withdrawing_and_unclaiming_name_the_account_and_send_no_password() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(204, ""),
        Answer::reply(204, ""),
    ]);
    let emby = reader(&fake);
    assert!(emby.withdraw(MEMBER).await.is_ok());
    assert!(emby.unclaim(MEMBER).await.is_ok());
    assert!(asked(&fake, Method::Delete, &format!("/Users/{MEMBER}")).is_some());
    assert_eq!(
        sent(&fake, Method::Post, &format!("/Users/{MEMBER}/Password")),
        serde_json::json!({ "ResetPassword": true })
    );
}

#[tokio::test]
async fn an_account_that_is_no_id_is_refused_before_emby_is_asked() {
    let fake = Fake::silent();
    let emby = reader(&fake);
    for id in ["", "../Users", "8"] {
        assert!(matches!(
            emby.withdraw(id).await,
            Err(Failure::Refused { .. })
        ));
        assert!(matches!(
            emby.unclaim(id).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert!(fake.requests().is_empty());
}

const MADE: &str = r#"[
    {"Id":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3","DateCreated":"2026-08-01T00:00:00.0000000Z"},
    {"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","DateCreated":"2026-08-29T08:59:59.9000000Z"},
    {"Id":"5f53cda18c2baa0c0354bb5f9a3ecbe5","DateCreated":"2026-07-01T00:00:00.0000000Z"},
    {"Id":"6f53cda18c2baa0c0354bb5f9a3ecbe5"},
    {"Id":"not-an-id","DateCreated":"2026-08-02T00:00:00.0000000Z"}
]"#;

#[tokio::test]
async fn each_account_made_since_is_dated_by_the_latest_of_its_making_and_what_emby_recorded() {
    let recorded = r#"{"Items":[
        {"Type":"user.passwordchanged","Date":"2026-08-30T09:00:00.0000000Z","UserId":"2"},
        {"Type":"user.created","Date":"2026-08-29T09:00:00.0000000Z","UserId":"2"},
        {"Type":"user.authenticated","Date":"2026-08-30T10:00:00.0000000Z","UserId":"2"},
        {"Type":"user.passwordchanged","Date":"2026-07-31T00:00:00.0000000Z","UserId":"1"},
        {"Type":"user.created","Date":"2026-08-31T09:00:00.0000000Z","UserId":"3"},
        {"Type":"user.created","Date":"2026-08-31T10:00:00.0000000Z","UserId":"9"},
        {"Type":"user.passwordchanged","Date":"2026-08-31T11:00:00.0000000Z","UserId":"4"},
        {"Type":"user.created","Date":"2026-08-31T11:00:00.0000000Z"},
        {"Type":"user.created","Date":"2026-08-31T12:00:00.0000000Z","UserId":"../2"}
    ]}"#;
    let fake = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("/System/ActivityLog/Entries", Answer::reply(200, recorded)),
        (
            "/Users/1",
            Answer::reply(200, r#"{"Id":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3"}"#),
        ),
        (
            "/Users/2",
            Answer::reply(200, r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5"}"#),
        ),
        ("/Users/3", Answer::reply(404, "")),
        (
            "/Users/4",
            Answer::reply(200, r#"{"Id":"7f53cda18c2baa0c0354bb5f9a3ecbe5"}"#),
        ),
        ("/Users/9", Answer::reply(200, r#"{"Id":"not-an-id"}"#)),
        ("/Users", Answer::reply(200, MADE)),
    ]);
    let invited = reader(&fake).when_invited("2026-08-01T00:00:00Z").await;
    let at = |member: &str, at: &str| Invited {
        member: member.to_owned(),
        at: at.to_owned(),
    };
    assert_eq!(
        invited.ok(),
        Some(vec![
            at(ADMIN, "2026-08-01T00:00:00.0000000Z"),
            at(MEMBER, "2026-08-30T09:00:00.0000000Z"),
            at(
                "7f53cda18c2baa0c0354bb5f9a3ecbe5",
                "2026-08-31T11:00:00.0000000Z"
            ),
        ])
    );
    let entries = asked(&fake, Method::Get, "Limit=200").map(|asked| asked.url);
    assert_eq!(
        entries,
        Some("http://127.0.0.1:8096/System/ActivityLog/Entries?MinDate=2026-08-01T00%3A00%3A00Z&Limit=200".to_owned())
    );
    assert_eq!(
        fake.requests()
            .iter()
            .filter(|asked| asked.url.ends_with("/Users/2"))
            .count(),
        1
    );
}

#[tokio::test]
async fn an_account_emby_recorded_and_cannot_name_fails_the_read() {
    let fake = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        (
            "/System/ActivityLog/Entries",
            Answer::reply(
                200,
                r#"{"Items":[{"Type":"user.created","Date":"d","UserId":"2"}]}"#,
            ),
        ),
        ("/Users/2", Answer::reply(500, "")),
        ("/Users", Answer::reply(200, "[]")),
    ]);
    assert!(reader(&fake).when_invited("since").await.is_err());
}

#[tokio::test]
async fn whoever_signs_in_from_the_device_named_and_answers_the_account_and_its_token() {
    let fake = Fake::in_turn(vec![Answer::reply(200, signed_in_as_member())]);
    let password = a_word();
    let signed = reader(&fake)
        .whoever("bo", &password, "conformance-whoever")
        .await;
    assert_eq!(signed.ok().flatten(), Some(member_signed()));
    let request = asked(&fake, Method::Post, "/Users/AuthenticateByName");
    assert_eq!(
        request.as_ref().and_then(|asked| header(asked, "X-Emby-Authorization")),
        Some(r#"Emby Client="lemonfiber", Device="lemonfiber", DeviceId="conformance-whoever", Version="1""#.to_owned())
    );
    assert_eq!(
        sent(&fake, Method::Post, "/Users/AuthenticateByName"),
        serde_json::json!({ "Username": "bo", "Pw": password })
    );
}

#[tokio::test]
async fn a_refused_sign_in_is_nobody_and_an_unreadable_one_is_refused() {
    for status in [401, 403] {
        let fake = Fake::in_turn(vec![Answer::reply(status, "")]);
        assert_eq!(
            reader(&fake)
                .whoever("bo", &a_word(), "phone-0001")
                .await
                .ok(),
            Some(None)
        );
    }
    let odd = Fake::in_turn(vec![Answer::reply(
        200,
        r#"{"AccessToken":"a/b","User":{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5"}}"#,
    )]);
    assert_eq!(
        reader(&odd)
            .whoever("bo", &a_word(), "phone-0001")
            .await
            .ok(),
        Some(None)
    );
    let unread = Fake::in_turn(vec![Answer::reply(200, "not json")]);
    assert!(reader(&unread)
        .whoever("bo", &a_word(), "phone-0001")
        .await
        .is_err());
    let silent = Fake::silent();
    assert!(matches!(
        reader(&silent).whoever("bo", &a_word(), "phone 1").await,
        Err(Failure::Refused { .. })
    ));
    assert!(silent.requests().is_empty());
}

fn standing_fake(sessions: &'static str, user: Answer) -> std::sync::Arc<Fake> {
    Fake::by_path(vec![
        ("/Sessions", Answer::reply(200, sessions)),
        ("/Users/", user),
    ])
}

const ITS_SESSION: &str =
    r#"[{"UserId":"4f53cda18c2baa0c0354bb5f9a3ecbe5","DeviceId":"phone-0001"}]"#;

#[tokio::test]
async fn a_sign_in_stands_while_its_token_opens_a_session_of_a_switched_on_account_holding_a_password(
) {
    let fake = standing_fake(
        ITS_SESSION,
        Answer::reply(
            200,
            r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","HasPassword":true,"Policy":{"IsDisabled":false}}"#,
        ),
    );
    assert_eq!(
        reader(&fake).standing(&member_signed()).await.ok(),
        Some(true)
    );
    assert!(fake
        .requests()
        .iter()
        .all(|asked| header(asked, "X-Emby-Token").as_deref() == Some(a_token().as_str())));
}

#[tokio::test]
async fn a_sign_in_does_not_stand_once_anything_about_it_has_moved() {
    let cases = [
        standing_fake("[]", Answer::reply(200, "{}")),
        standing_fake(
            r#"[{"UserId":"5f53cda18c2baa0c0354bb5f9a3ecbe5"}]"#,
            Answer::reply(200, "{}"),
        ),
        standing_fake(ITS_SESSION, Answer::reply(404, "")),
        standing_fake(ITS_SESSION, Answer::reply(403, "")),
        standing_fake(
            ITS_SESSION,
            Answer::reply(
                200,
                r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","HasPassword":false}"#,
            ),
        ),
        standing_fake(
            ITS_SESSION,
            Answer::reply(
                200,
                r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","HasPassword":true,"Policy":{"IsDisabled":true}}"#,
            ),
        ),
        standing_fake(
            ITS_SESSION,
            Answer::reply(
                200,
                r#"{"Id":"5f53cda18c2baa0c0354bb5f9a3ecbe5","HasPassword":true}"#,
            ),
        ),
        Fake::in_turn(vec![Answer::reply(401, "")]),
    ];
    for fake in cases {
        assert_eq!(
            reader(&fake).standing(&member_signed()).await.ok(),
            Some(false)
        );
    }
    let misshapen = Fake::silent();
    for signed in [
        Signed {
            id: MEMBER.to_owned(),
            token: "a/b".to_owned(),
        },
        Signed {
            id: "8".to_owned(),
            token: "abc".to_owned(),
        },
    ] {
        assert_eq!(reader(&misshapen).standing(&signed).await.ok(), Some(false));
    }
    assert!(misshapen.requests().is_empty());
}

#[tokio::test]
async fn standing_emby_answers_unreadably_is_refused() {
    let sessions = Fake::in_turn(vec![Answer::reply(200, "{}")]);
    assert!(reader(&sessions).standing(&member_signed()).await.is_err());
    let user = standing_fake(ITS_SESSION, Answer::reply(200, "not json"));
    assert!(reader(&user).standing(&member_signed()).await.is_err());
}

#[tokio::test]
async fn claiming_signs_in_with_no_password_sets_the_first_one_and_leaves() {
    let first = a_word();
    let fake = Fake::in_turn(vec![
        Answer::reply(200, signed_in_as_member()),
        Answer::reply(204, ""),
        Answer::Silent,
    ]);
    assert_eq!(
        reader(&fake)
            .claim("bo", &first, "conformance-claim")
            .await
            .ok(),
        Some(true)
    );
    assert_eq!(
        sent(&fake, Method::Post, "/Users/AuthenticateByName"),
        serde_json::json!({ "Username": "bo", "Pw": "" })
    );
    assert_eq!(
        sent(&fake, Method::Post, &format!("/Users/{MEMBER}/Password")),
        serde_json::json!({ "NewPw": first })
    );
    assert!(asked(&fake, Method::Post, "/Sessions/Logout").is_some());
}

#[tokio::test]
async fn an_account_already_claimed_is_not_claimed_again_and_a_refused_password_is_an_error() {
    let first = a_word();
    let claimed = Fake::in_turn(vec![Answer::reply(401, "")]);
    assert_eq!(
        reader(&claimed)
            .claim("bo", &first, "conformance-claim")
            .await
            .ok(),
        Some(false)
    );
    let refused = Fake::in_turn(vec![
        Answer::reply(200, signed_in_as_member()),
        Answer::reply(400, ""),
    ]);
    assert!(reader(&refused)
        .claim("bo", &first, "conformance-claim")
        .await
        .is_err());
}

#[tokio::test]
async fn ratings_are_said_at_the_age_each_level_is_read_as_and_a_level_off_the_ladder_is_left_out()
{
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(
            200,
            r#"[{"Name":"G","Value":1},{"Name":"PG-13","Value":7},{"Name":"R","Value":9},{"Name":"odd","Value":12},{"Name":"none"}]"#,
        ),
    ]);
    let read: Vec<(String, u32)> = reader(&fake)
        .ratings()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|certificate| (certificate.name, certificate.age))
        .collect();
    assert_eq!(
        read,
        [
            ("G".to_owned(), 0),
            ("PG-13".to_owned(), 12),
            ("R".to_owned(), 18)
        ]
    );
}

#[tokio::test]
async fn the_country_is_the_one_emby_files_metadata_for() {
    for (configured, country) in [
        (r#"{"MetadataCountryCode":"gb"}"#, Some("GB".to_owned())),
        (r#"{"MetadataCountryCode":"G1"}"#, None),
        ("{}", None),
    ] {
        let fake = Fake::in_turn(vec![
            Answer::reply(200, SIGNED_IN),
            Answer::reply(200, configured),
        ]);
        assert_eq!(reader(&fake).country().await.ok(), Some(country));
    }
}

#[tokio::test]
async fn a_library_is_named_by_the_id_an_account_s_policy_opens_it_by() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(
            200,
            r#"{"Items":[{"Id":"3","Guid":"a2a9b93ad0ac47e5b9640b87906d04f4","Name":"Movies"}]}"#,
        ),
    ]);
    let read: Vec<(String, String)> = reader(&fake)
        .libraries()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|library| (library.id, library.name))
        .collect();
    assert_eq!(
        read,
        [(
            "a2a9b93ad0ac47e5b9640b87906d04f4".to_owned(),
            "Movies".to_owned()
        )]
    );
    assert!(asked(&fake, Method::Get, "/Library/MediaFolders?Fields=Guid").is_some());
}

#[tokio::test]
async fn the_devices_signed_in_to_one_account_are_its_sessions_alone() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(
            200,
            r#"[{"UserId":"4f53cda18c2baa0c0354bb5f9a3ecbe5","DeviceId":"tv","DeviceName":"Lounge","Client":"Emby Theater","LastActivityDate":"2026-11-06T00:00:00Z"},
               {"UserId":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3","DeviceId":"web","DeviceName":"Chrome","Client":"Emby Web"}]"#,
        ),
    ]);
    let held = reader(&fake).sessions(MEMBER).await.unwrap_or_default();
    assert_eq!(
        held.iter()
            .map(|session| (
                session.device_id.as_str(),
                session.device.as_str(),
                session.client.as_str(),
                session.last_seen.as_deref()
            ))
            .collect::<Vec<_>>(),
        [("tv", "Lounge", "Emby Theater", Some("2026-11-06T00:00:00Z"))]
    );
    assert!(!held.iter().any(|session| session.device_id == ADMIN));
}

#[tokio::test]
async fn emby_signs_no_device_in_by_a_code_and_is_not_asked() {
    let fake = Fake::silent();
    assert_eq!(reader(&fake).signs_devices_in().await.ok(), Some(false));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn an_activity_log_or_a_household_emby_answers_unreadably_is_refused() {
    for answers in [
        vec![
            Answer::reply(200, SIGNED_IN),
            Answer::reply(200, "not json"),
        ],
        vec![
            Answer::reply(200, SIGNED_IN),
            Answer::reply(200, "[]"),
            Answer::reply(200, "not json"),
        ],
    ] {
        let fake = Fake::in_turn(answers);
        assert!(matches!(
            reader(&fake).when_invited("since").await,
            Err(Failure::Refused { .. })
        ));
    }
}
