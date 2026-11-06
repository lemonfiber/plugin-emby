use super::{asked, reader, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::Library;

#[tokio::test]
async fn the_library_holds_an_item_whose_name_carries_the_term() {
    let fake = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        (
            "/Items",
            Answer::reply(200, r#"{"Items":[{"Name":"Big Buck Bunny"}]}"#),
        ),
    ]);
    let emby = reader(&fake);
    assert_eq!(emby.has_item(Kind::Movies, "buck").await.ok(), Some(true));
    assert_eq!(emby.has_item(Kind::Tv, "wire").await.ok(), Some(false));
    assert!(asked(
        &fake,
        Method::Get,
        "/Items?Recursive=true&IncludeItemTypes=Movie"
    )
    .is_some());
    assert!(asked(
        &fake,
        Method::Get,
        "/Items?Recursive=true&IncludeItemTypes=Series"
    )
    .is_some());
}

#[tokio::test]
async fn every_library_is_asked_to_look_again() {
    let fake = Fake::in_turn(vec![Answer::reply(200, SIGNED_IN), Answer::reply(204, "")]);
    assert!(reader(&fake).rescan().await.is_ok());
    assert!(asked(&fake, Method::Post, "/Library/Refresh").is_some());
}
