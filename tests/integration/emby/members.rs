use super::{header, reader};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::service::Answered;

fn token() -> String {
    "0123456789abcdef".repeat(2)
}

#[tokio::test]
async fn the_member_may_have_an_item_emby_shows_them_under_their_own_token() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"{"Items":[{"Id":"8","Name":"A Film"}]}"#,
    ));
    let emby = reader(&fake);
    assert_eq!(
        emby.may("item", "8", &token()).await.ok(),
        Some(Answered::May)
    );
    assert_eq!(
        emby.may("item", "mediasource_8", &token()).await.ok(),
        Some(Answered::May)
    );
    let asked = fake.requests();
    assert!(asked
        .iter()
        .all(|asked| asked.url == "http://127.0.0.1:8096/Items?Ids=8"
            && header(asked, "X-Emby-Token").as_deref() == Some(token().as_str())));
}

#[tokio::test]
async fn an_item_emby_does_not_show_the_member_is_one_they_may_not_have() {
    let fake = Fake::always(Answer::reply(200, r#"{"Items":[{"Id":"80"}]}"#));
    assert_eq!(
        reader(&fake).may("item", "8", &token()).await.ok(),
        Some(Answered::MayNot)
    );
}

#[tokio::test]
async fn a_space_or_an_id_emby_does_not_know_is_answered_without_asking() {
    let fake = Fake::silent();
    let emby = reader(&fake);
    for (space, item) in [("part", "8"), ("item", "x8"), ("item", "source_8")] {
        assert_eq!(
            emby.may(space, item, &token()).await.ok(),
            Some(Answered::MayNot)
        );
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn emby_s_refusals_are_read_as_the_guard_reads_them() {
    for (status, answered) in [
        (401, Answered::NoSession),
        (403, Answered::NotFromThere),
        (404, Answered::MayNot),
    ] {
        let fake = Fake::always(Answer::reply(status, ""));
        assert_eq!(
            reader(&fake).may("item", "8", &token()).await.ok(),
            Some(answered)
        );
    }
    for unreadable in [Answer::reply(500, ""), Answer::reply(200, "not json")] {
        let fake = Fake::always(unreadable);
        assert!(reader(&fake).may("item", "8", &token()).await.is_err());
    }
}
