use super::{asked, reader, sent, MEMBER, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, HowFar, Medium, Picture, Screening};

type Season = (String, Option<u32>, Vec<(String, Option<u32>, Option<u32>)>);

const A_SHELF: &str = r#"{"Items":[
    {"Id":"8","Name":"A Film","ProductionYear":1994,"Type":"Movie","IsFolder":false},
    {"Id":"9","Name":"A Series","Type":"Series","IsFolder":true}
]}"#;

const A_FILM: &str = r#"{"Items":[{"Id":"8","Name":"A Film","ProductionYear":2008,"Type":"Movie","IsFolder":false,
    "Overview":"Rabbits.","RunTimeTicks":59400000000,"Genres":["Comedy"],"OfficialRating":"PG","PremiereDate":"2008-05-30T00:00:00.0000000Z",
    "ImageTags":{"Primary":"p"},"BackdropImageTags":[]}]}"#;

const A_SERIES: &str =
    r#"{"Items":[{"Id":"9","Name":"A Series","Type":"Series","IsFolder":true}]}"#;

const SEASONS: &str = r#"{"Items":[{"Id":"10","Name":"Season 1","IndexNumber":1,"Type":"Season","IsFolder":true},
    {"Id":"13","Name":"Specials","IndexNumber":0,"Type":"Season","IsFolder":true}]}"#;

const EPISODES: &str = r#"{"Items":[
    {"Id":"11","Name":"Pilot","IndexNumber":1,"SeasonId":"10","Type":"Episode","RunTimeTicks":1200000000,"Overview":"It starts."},
    {"Id":"12","Name":"Second","IndexNumber":2,"SeasonId":"10","Type":"Episode"},
    {"Id":"14","Name":"Lost","Type":"Episode"}]}"#;

#[tokio::test]
async fn what_the_house_holds_is_asked_newest_first_and_for_one_member_under_their_own_limits() {
    let fake = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("/Items", Answer::reply(200, A_SHELF)),
    ]);
    let emby = reader(&fake);
    let held = emby.holdings(None, 10).await.unwrap_or_default();
    assert_eq!(
        held.iter()
            .map(|item| (item.id.as_str(), item.medium, item.year))
            .collect::<Vec<_>>(),
        [("8", Medium::Film, Some(1994)), ("9", Medium::Series, None)]
    );
    assert!(emby.holdings(Some(MEMBER), 5).await.is_ok());
    let urls: Vec<String> = fake
        .requests()
        .into_iter()
        .filter(|asked| asked.url.contains("/Items?"))
        .map(|asked| asked.url)
        .collect();
    assert_eq!(
        urls,
        [
            "http://127.0.0.1:8096/Items?Recursive=true&IncludeItemTypes=Series%2CMovie&SortBy=DateCreated&SortOrder=Descending&Limit=10&Fields=ProductionYear".to_owned(),
            format!("http://127.0.0.1:8096/Items?Recursive=true&IncludeItemTypes=Series%2CMovie&SortBy=DateCreated&SortOrder=Descending&Limit=5&Fields=ProductionYear&UserId={MEMBER}"),
        ]
    );
    assert!(matches!(
        emby.holdings(Some("8"), 5).await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn what_is_playing_is_read_from_the_sessions_playing_something() {
    let sessions = r#"[
        {"UserId":"4f53cda18c2baa0c0354bb5f9a3ecbe5","UserName":"bo","DeviceName":"Lounge",
         "NowPlayingItem":{"Name":"Pilot","SeriesName":"A Series","ParentIndexNumber":1,"IndexNumber":1,"Type":"Episode"},
         "PlayState":{"IsPaused":true}},
        {"UserId":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3","UserName":"admin","DeviceName":"Web",
         "NowPlayingItem":{"Name":"A Film","Type":"Movie"}},
        {"UserId":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3","UserName":"admin","DeviceName":"Phone",
         "NowPlayingItem":{"Name":"A Song","Type":"Audio"}},
        {"UserId":"4f53cda18c2baa0c0354bb5f9a3ecbe5","UserName":"bo","DeviceName":"Idle"}
    ]"#;
    let fake = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("/Sessions", Answer::reply(200, sessions)),
    ]);
    let emby = reader(&fake);
    let all = emby.playing(None).await.unwrap_or_default();
    assert_eq!(
        all.iter()
            .map(|playing| (
                playing.title.as_str(),
                playing.medium,
                playing.paused,
                playing.device.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("Pilot", Medium::Series, true, "Lounge"),
            ("A Film", Medium::Film, false, "Web"),
            ("A Song", Medium::Other, false, "Phone"),
        ]
    );
    let hers = emby.playing(Some(MEMBER)).await.unwrap_or_default();
    assert_eq!(
        hers.iter()
            .map(|playing| (
                playing.member.as_str(),
                playing.series.as_deref(),
                playing.season,
                playing.episode
            ))
            .collect::<Vec<_>>(),
        [("bo", Some("A Series"), Some(1), Some(1))]
    );
}

#[tokio::test]
async fn emby_opens_no_session_for_a_member_s_device_and_is_not_asked() {
    let fake = Fake::silent();
    let emby = reader(&fake);
    assert_eq!(emby.signed_in(MEMBER, "phone-0001").await.ok(), Some(None));
    assert!(matches!(
        emby.signed_in("8", "phone-0001").await,
        Err(Failure::Refused { .. })
    ));
    assert!(matches!(
        emby.signed_in(MEMBER, "phone").await,
        Err(Failure::Refused { .. })
    ));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_film_is_read_as_one_member_may_see_it() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, A_FILM),
    ]);
    let detail = reader(&fake).title(Some(MEMBER), "8").await.ok().flatten();
    assert_eq!(
        detail.as_ref().map(|detail| (
            detail.item.title.as_str(),
            detail.overview.as_deref(),
            detail.minutes,
            detail.genres.clone(),
            detail.certificate.as_deref(),
            detail.released.as_deref(),
            detail.item.holds.poster,
            detail.seasons.len(),
        )),
        Some((
            "A Film",
            Some("Rabbits."),
            Some(99),
            vec!["Comedy".to_owned()],
            Some("PG"),
            Some("2008-05-30"),
            true,
            0
        ))
    );
    assert_eq!(
        asked(&fake, Method::Get, &format!("UserId={MEMBER}")).map(|asked| asked.url),
        Some(format!("http://127.0.0.1:8096/Items?Ids=8&Fields=Overview%2CGenres%2COfficialRating%2CPremiereDate%2CProductionYear&UserId={MEMBER}"))
    );
}

#[tokio::test]
async fn a_series_is_read_with_each_season_holding_its_own_episodes_in_order() {
    let fake = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("/Shows/9/Seasons", Answer::reply(200, SEASONS)),
        ("/Shows/9/Episodes", Answer::reply(200, EPISODES)),
        ("/Items?Ids=9", Answer::reply(200, A_SERIES)),
    ]);
    let emby = reader(&fake);
    let detail = emby.title(None, "9").await.ok().flatten();
    let seasons: Vec<Season> = detail
        .map(|detail| detail.seasons)
        .unwrap_or_default()
        .into_iter()
        .map(|season| {
            (
                season.name,
                season.number,
                season
                    .episodes
                    .into_iter()
                    .map(|episode| (episode.item.title, episode.number, episode.minutes))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        seasons,
        [
            (
                "Season 1".to_owned(),
                Some(1),
                vec![
                    ("Pilot".to_owned(), Some(1), Some(2)),
                    ("Second".to_owned(), Some(2), None)
                ]
            ),
            ("Specials".to_owned(), Some(0), Vec::new()),
        ]
    );
    assert!(asked(&fake, Method::Get, "/Shows/9/Seasons").is_some());
    assert!(asked(&fake, Method::Get, "/Shows/9/Episodes?Fields=Overview").is_some());
    assert!(emby.title(Some(MEMBER), "9").await.is_ok());
    assert!(asked(
        &fake,
        Method::Get,
        &format!("/Shows/9/Seasons?UserId={MEMBER}")
    )
    .is_some());
    assert!(asked(
        &fake,
        Method::Get,
        &format!("/Shows/9/Episodes?UserId={MEMBER}&Fields=Overview")
    )
    .is_some());
}

#[tokio::test]
async fn a_title_no_id_could_name_or_the_member_may_not_see_is_none() {
    let silent = Fake::silent();
    assert_eq!(reader(&silent).title(None, "../8").await.ok(), Some(None));
    assert!(silent.requests().is_empty());
    let unseen = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(
            200,
            r#"{"Items":[{"Id":"80","Name":"Another","Type":"Movie"}]}"#,
        ),
    ]);
    assert_eq!(
        reader(&unseen).title(Some(MEMBER), "8").await.ok(),
        Some(None)
    );
    assert!(matches!(
        reader(&silent).title(Some("8"), "8").await,
        Err(Failure::Refused { .. })
    ));
    let unread = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, "not json"),
    ]);
    assert!(reader(&unread).title(None, "8").await.is_err());
}

#[tokio::test]
async fn a_series_whose_seasons_or_episodes_cannot_be_read_is_refused() {
    for (seasons, episodes) in [("not json", EPISODES), (SEASONS, "not json")] {
        let fake = Fake::by_path(vec![
            ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
            ("/Shows/9/Seasons", Answer::reply(200, seasons)),
            ("/Shows/9/Episodes", Answer::reply(200, episodes)),
            ("/Items?Ids=9", Answer::reply(200, A_SERIES)),
        ]);
        assert!(reader(&fake).title(None, "9").await.is_err());
    }
}

#[tokio::test]
async fn what_a_member_is_part_way_through_is_read_in_seconds() {
    let resumed = r#"{"Items":[{"Id":"8","Name":"A Film","Type":"Movie","RunTimeTicks":1200000000,"UserData":{"PlaybackPositionTicks":600000000}},
        {"Id":"11","Name":"Pilot","Type":"Episode"}]}"#;
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, resumed),
    ]);
    let read = reader(&fake).part_way(MEMBER, 10).await.unwrap_or_default();
    assert_eq!(
        read.iter()
            .map(|part| (part.item.id.as_str(), part.position, part.length))
            .collect::<Vec<_>>(),
        [("8", 60, Some(120)), ("11", 0, None)]
    );
    assert_eq!(
        asked(&fake, Method::Get, "Fields=ProductionYear").map(|asked| asked.url),
        Some(format!("http://127.0.0.1:8096/Users/{MEMBER}/Items/Resume?Limit=10&MediaTypes=Video&Fields=ProductionYear"))
    );
    let silent = Fake::silent();
    assert!(matches!(
        reader(&silent).part_way("8", 10).await,
        Err(Failure::Refused { .. })
    ));
    let unread = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, "not json"),
    ]);
    assert!(reader(&unread).part_way(MEMBER, 10).await.is_err());
}

#[tokio::test]
async fn how_far_a_member_got_is_written_in_ticks_and_an_ended_item_as_played_from_the_start() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(204, ""),
        Answer::reply(204, ""),
    ]);
    let emby = reader(&fake);
    assert!(emby
        .progressed(
            MEMBER,
            "8",
            &HowFar {
                position: 60,
                ended: false
            }
        )
        .await
        .is_ok());
    assert_eq!(
        sent(
            &fake,
            Method::Post,
            &format!("/Users/{MEMBER}/Items/8/UserData")
        ),
        serde_json::json!({ "PlaybackPositionTicks": 600_000_000, "Played": false })
    );
    assert!(emby
        .progressed(
            MEMBER,
            "9",
            &HowFar {
                position: 60,
                ended: true
            }
        )
        .await
        .is_ok());
    assert_eq!(
        sent(
            &fake,
            Method::Post,
            &format!("/Users/{MEMBER}/Items/9/UserData")
        ),
        serde_json::json!({ "PlaybackPositionTicks": 0, "Played": true })
    );
    let silent = Fake::silent();
    let far = HowFar {
        position: 1,
        ended: false,
    };
    assert!(matches!(
        reader(&silent).progressed("8", "8", &far).await,
        Err(Failure::Refused { .. })
    ));
    assert!(matches!(
        reader(&silent).progressed(MEMBER, "x", &far).await,
        Err(Failure::Refused { .. })
    ));
    assert!(silent.requests().is_empty());
}

fn pictured(film: &'static str, picture: Answer) -> std::sync::Arc<Fake> {
    Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("/Images/", picture),
        ("/Items?Ids=8", Answer::reply(200, film)),
    ])
}

#[tokio::test]
async fn a_picture_the_member_may_see_and_emby_holds_is_answered_with_its_type() {
    let fake = pictured(A_FILM, Answer::served(200, "image/gif", "GIF89a-bytes"));
    let image = reader(&fake)
        .picture(Some(MEMBER), "8", Picture::Poster)
        .await
        .ok()
        .flatten();
    assert_eq!(
        image.map(|image| image.media_type),
        Some("image/gif".to_owned())
    );
    assert!(asked(&fake, Method::Get, "/Items/8/Images/Primary").is_some());
}

#[tokio::test]
async fn a_picture_emby_does_not_hold_is_none_and_never_fetched() {
    let fake = pictured(A_FILM, Answer::reply(500, ""));
    assert_eq!(
        reader(&fake)
            .picture(None, "8", Picture::Backdrop)
            .await
            .ok(),
        Some(None)
    );
    assert!(asked(&fake, Method::Get, "/Images/").is_none());
    let unseen = pictured(r#"{"Items":[]}"#, Answer::reply(500, ""));
    assert_eq!(
        reader(&unseen)
            .picture(None, "8", Picture::Poster)
            .await
            .ok(),
        Some(None)
    );
    let silent = Fake::silent();
    assert_eq!(
        reader(&silent)
            .picture(None, "x", Picture::Poster)
            .await
            .ok(),
        Some(None)
    );
    assert!(matches!(
        reader(&silent)
            .picture(Some("8"), "8", Picture::Poster)
            .await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn a_picture_emby_no_longer_has_is_none_and_one_it_refuses_is_an_error() {
    let backdrop =
        r#"{"Items":[{"Id":"8","Name":"A Film","Type":"Movie","BackdropImageTags":["b"]}]}"#;
    let gone = pictured(backdrop, Answer::reply(404, ""));
    assert_eq!(
        reader(&gone)
            .picture(None, "8", Picture::Backdrop)
            .await
            .ok(),
        Some(None)
    );
    assert!(asked(&gone, Method::Get, "/Items/8/Images/Backdrop").is_some());
    let refused = pictured(backdrop, Answer::reply(500, ""));
    assert!(matches!(
        reader(&refused).picture(None, "8", Picture::Backdrop).await,
        Err(Failure::Refused { .. })
    ));
    let unreached = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("/Items?Ids=8", Answer::reply(200, backdrop)),
    ]);
    assert!(matches!(
        reader(&unreached)
            .picture(None, "8", Picture::Backdrop)
            .await,
        Err(Failure::Unavailable { .. })
    ));
}

#[tokio::test]
async fn a_device_is_signed_out_by_the_id_it_reported() {
    let fake = Fake::in_turn(vec![Answer::reply(200, SIGNED_IN), Answer::reply(204, "")]);
    assert!(reader(&fake).sign_out("phone-0001").await.is_ok());
    assert!(asked(&fake, Method::Delete, "/Devices?Id=phone-0001").is_some());
    let silent = Fake::silent();
    for device in ["12345678", "phone 1"] {
        assert!(matches!(
            reader(&silent).sign_out(device).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert!(silent.requests().is_empty());
}

#[tokio::test]
async fn sessions_emby_answers_unreadably_are_refused() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, SIGNED_IN),
        Answer::reply(200, "not json"),
    ]);
    assert!(matches!(
        reader(&fake).playing(None).await,
        Err(Failure::Refused { .. })
    ));
}
