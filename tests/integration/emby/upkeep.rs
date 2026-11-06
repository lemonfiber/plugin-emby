use super::{asked, reader, ADMIN, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, Upkeep};

#[tokio::test]
async fn every_series_is_read_with_its_seasons_and_episodes_as_the_administrator_sees_them() {
    let series = r#"{"Items":[{"Id":"9","Name":"A Series","ChildCount":2,"RecursiveItemCount":12},
        {"Id":"20","Name":"Loose","ChildCount":0}]}"#;
    let fake = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        (
            "ParentId=20",
            Answer::reply(200, r#"{"TotalRecordCount":3}"#),
        ),
        ("/Items", Answer::reply(200, series)),
    ]);
    let held = reader(&fake).series_held(10).await.unwrap_or_default();
    assert_eq!(
        held.iter()
            .map(|series| (
                series.id.as_str(),
                series.title.as_str(),
                series.seasons,
                series.episodes
            ))
            .collect::<Vec<_>>(),
        [("9", "A Series", 2, 12), ("20", "Loose", 0, 3)]
    );
    assert!(asked(
        &fake,
        Method::Get,
        &format!("/Users/{ADMIN}/Items?Recursive=true&IncludeItemTypes=Series&Fields=ChildCount%2CRecursiveItemCount&SortBy=SortName&Limit=10")
    )
    .is_some());
}

#[tokio::test]
async fn series_emby_answers_unreadably_are_refused() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, "not json"),
    ]);
    assert!(reader(&fake).series_held(10).await.is_err());
    let count = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("ParentId=20", Answer::reply(200, "{}")),
        (
            "/Items",
            Answer::reply(200, r#"{"Items":[{"Id":"20","Name":"Loose"}]}"#),
        ),
    ]);
    assert!(reader(&count).series_held(10).await.is_err());
}

#[tokio::test]
async fn one_item_is_read_afresh_with_everything_under_it_and_nothing_replaced() {
    let fake = Fake::in_turn(vec![Answer::reply(200, SIGNED_IN), Answer::reply(204, "")]);
    assert!(reader(&fake).refresh("9").await.is_ok());
    assert!(asked(
        &fake,
        Method::Post,
        "/Items/9/Refresh?Recursive=true&MetadataRefreshMode=FullRefresh&ImageRefreshMode=Default&ReplaceAllMetadata=false&ReplaceAllImages=false"
    )
    .is_some());
    let silent = Fake::silent();
    assert!(matches!(
        reader(&silent).refresh("../9").await,
        Err(Failure::Refused { .. })
    ));
    assert!(silent.requests().is_empty());
}
