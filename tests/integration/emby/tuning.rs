use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Airing, Channel, Declaring as _, Failure, Listing, Programme, Tuned, Tuning as _,
};
use regex::Regex;

use super::{reader, MEMBER, SIGNED_IN};

const SIGNING_IN: &str = "AuthenticateByName";

const LICENCE: &str = "/Plugins/SecurityInfo";

const HOLDER: &str = "/Users/4f53cda18c2baa0c0354bb5f9a3ecbe5";

const CHANNELS: &str = "/LiveTv/Channels";

const PROGRAMMES: &str = "/LiveTv/Programs";

const OPENING: &str = "/PlaybackInfo";

const CLOSING: &str = "/LiveStreams/Close";

const CHANNEL: &str = "12";

const SUPPORTER: &str = r#"{"IsMBSupporter":true}"#;

const WATCHES: &str = r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Policy":{"EnableLiveTvAccess":true,"IsDisabled":false,"EnableAllChannels":true}}"#;

const SHOWN: &str = r#"{"Items":[{"Id":"12","Name":"Channel One","Type":"TvChannel","ChannelNumber":"1","CurrentProgram":{"Id":"40","Name":"Morning Film","OfficialRating":"TV-14","StartDate":"2026-11-13T17:00:00.0000000Z","EndDate":"2026-11-13T18:00:00.0000000Z"}},{"Id":"13","Name":"Channel Two","Type":"TvChannel","ChannelNumber":"","CurrentProgram":{"Name":"","OfficialRating":"TV-MA"}},{"Id":"../13","Name":"Elsewhere"}],"TotalRecordCount":3}"#;

const SCHEDULED: &str = r#"{"Items":[{"Id":"41","Name":"Evening News","OfficialRating":"","ChannelId":"12","Type":"Program","StartDate":"2026-11-13T18:00:00.0000000Z","EndDate":"2026-11-13T19:00:00.0000000Z"},{"Id":"40","Name":"Morning Film","OfficialRating":"TV-14","ChannelId":"12","Type":"Program","StartDate":"2026-11-13T17:00:00.0000000Z","EndDate":"2026-11-13T18:00:00.0000000Z"},{"Id":"43","Name":"Fright Night","OfficialRating":"TV-PG","ChannelId":"12","Type":"Program","Tags":["horror"],"StartDate":"2026-11-13T19:00:00.0000000Z","EndDate":"2026-11-13T20:30:00.0000000Z"},{"Id":"42","Name":"Undated","OfficialRating":"TV-G","ChannelId":"13","Type":"Program","EndDate":"2026-11-13T20:00:00.0000000Z"}],"TotalRecordCount":4}"#;

const GUARDED: &str = r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Policy":{"EnableLiveTvAccess":true,"IsDisabled":false,"EnableAllChannels":true,"MaxParentalRating":7,"BlockedTags":["Horror"],"BlockUnratedItems":["Movie"]}}"#;

const ADULT: &str = r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Policy":{"EnableLiveTvAccess":true,"IsDisabled":false,"EnableAllChannels":true,"MaxParentalRating":9}}"#;

const OFF_THE_LADDER: &str = r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Policy":{"EnableLiveTvAccess":true,"IsDisabled":false,"EnableAllChannels":true,"MaxParentalRating":0}}"#;

const RATINGS: &str = "/Localization/ParentalRatings";

const RATED: &str = r#"[{"Name":"TV-PG","Value":5},{"Name":"TV-14","Value":8}]"#;

const LIVE_STREAM: &str = "a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3_9d1c9d1c9d1c9d1c9d1c9d1c9d1c9d1c";

const PLAY_SESSION: &str = "986f9938c33c4e149a9ed1efdb4cb08f";

const FILE: &str = "0123456789abcdef0123456789abcdef";

fn opened(direct: &str, path: &str) -> String {
    serde_json::json!({
        "PlaySessionId": PLAY_SESSION,
        "MediaSources": [{
            "Id": "0b1c",
            "LiveStreamId": LIVE_STREAM,
            "Protocol": "Http",
            "IsInfiniteStream": true,
            "RequiresClosing": true,
            "DirectStreamUrl": direct,
            "Path": path,
            "TranscodingUrl": format!("/videos/{CHANNEL}/master.m3u8?LiveStreamId={LIVE_STREAM}&PlaySessionId={PLAY_SESSION}&api_key=0f0f"),
        }]
    })
    .to_string()
}

fn closes_by() -> String {
    format!("{LIVE_STREAM}.{PLAY_SESSION}")
}

fn emby_answering(routes: Vec<(&'static str, Answer)>) -> Arc<Fake> {
    let mut all = vec![(SIGNING_IN, Answer::reply(200, SIGNED_IN))];
    all.extend(routes);
    Fake::by_path(all)
}

fn licensed(routes: Vec<(&'static str, Answer)>) -> Arc<Fake> {
    let mut all = vec![(LICENCE, Answer::reply(200, SUPPORTER))];
    all.extend(routes);
    emby_answering(all)
}

fn premiere<T>(answer: &Result<T, Failure>) -> bool {
    matches!(answer, Err(Failure::Unsupported { detail, .. }) if detail.contains("Emby Premiere"))
}

fn asked_for(fake: &Fake, fragment: &str) -> bool {
    fake.requests()
        .iter()
        .any(|request| request.url.contains(fragment))
}

#[tokio::test]
async fn live_tv_is_unsupported_where_emby_does_not_say_it_holds_emby_premiere() {
    for licence in [
        Answer::reply(200, r#"{"IsMBSupporter":false}"#),
        Answer::reply(200, "{}"),
        Answer::reply(200, "not json"),
        Answer::reply(403, ""),
        Answer::reply(500, r#"{"IsMBSupporter":true}"#),
        Answer::Silent,
    ] {
        let fake = emby_answering(vec![(LICENCE, licence)]);
        let emby = reader(&fake);
        assert!(premiere(&emby.channels(MEMBER).await));
        assert!(premiere(&emby.guide(MEMBER, 12).await));
        assert!(premiere(&emby.tune(MEMBER, CHANNEL).await));
        assert!(premiere(&emby.untune(&closes_by()).await));
        assert!(fake
            .requests()
            .iter()
            .all(|request| request.url.contains(SIGNING_IN) || request.url.contains(LICENCE)));
    }
}

#[tokio::test]
async fn the_channels_are_the_member_s_own_with_what_is_on_now_and_its_certificate() {
    let fake = licensed(vec![
        (HOLDER, Answer::reply(200, WATCHES)),
        (CHANNELS, Answer::reply(200, SHOWN)),
    ]);
    let channels = reader(&fake).channels(MEMBER).await;
    assert_eq!(
        channels.ok(),
        Some(vec![
            Channel {
                id: CHANNEL.to_owned(),
                name: "Channel One".to_owned(),
                number: Some("1".to_owned()),
                now: Some(Programme {
                    title: "Morning Film".to_owned(),
                    certificate: Some("TV-14".to_owned()),
                    ends: lemonfiber_ports::Instant::from_service_text(
                        "2026-11-13T18:00:00.0000000Z"
                    ),
                }),
            },
            Channel {
                id: "13".to_owned(),
                name: "Channel Two".to_owned(),
                number: None,
                now: None,
            },
        ])
    );
    let listed = fake
        .requests()
        .into_iter()
        .find(|request| request.url.contains(CHANNELS))
        .map(|request| request.url);
    assert!(listed
        .is_some_and(|url| url.contains(&format!("UserId={MEMBER}"))
            && url.contains("AddCurrentProgram=true")));
}

#[tokio::test]
async fn a_member_whose_own_account_may_not_watch_live_tv_is_answered_no_channel_and_nothing_is_opened(
) {
    for held in [
        r#"{"Policy":{"EnableLiveTvAccess":false,"IsDisabled":false}}"#,
        r#"{"Policy":{"EnableLiveTvAccess":true,"IsDisabled":true}}"#,
        r#"{"Policy":{}}"#,
        "{}",
    ] {
        let fake = licensed(vec![
            (HOLDER, Answer::reply(200, held)),
            (CHANNELS, Answer::reply(200, SHOWN)),
            (OPENING, Answer::reply(200, opened("", ""))),
        ]);
        let emby = reader(&fake);
        assert_eq!(emby.channels(MEMBER).await.ok(), Some(Vec::new()));
        assert_eq!(emby.tune(MEMBER, CHANNEL).await.ok(), Some(None));
        assert_eq!(emby.guide(MEMBER, 12).await.ok(), Some(Vec::new()));
        assert!(!asked_for(&fake, CHANNELS));
        assert!(!asked_for(&fake, PROGRAMMES));
        assert!(!asked_for(&fake, OPENING));
    }
}

#[tokio::test]
async fn a_channel_the_member_s_own_listing_does_not_show_is_not_opened() {
    let fake = licensed(vec![
        (HOLDER, Answer::reply(200, WATCHES)),
        (CHANNELS, Answer::reply(200, r#"{"Items":[]}"#)),
        (OPENING, Answer::reply(200, opened("", ""))),
    ]);
    assert_eq!(reader(&fake).tune(MEMBER, CHANNEL).await.ok(), Some(None));
    assert!(fake
        .requests()
        .iter()
        .any(|request| request.url.contains(CHANNELS) && request.url.contains("Ids=12")));
    assert!(!asked_for(&fake, OPENING));
}

#[tokio::test]
async fn a_tuned_channel_is_opened_on_the_member_s_account_at_paths_a_live_path_covers() {
    let fake = licensed(vec![
        (HOLDER, Answer::reply(200, WATCHES)),
        (CHANNELS, Answer::reply(200, SHOWN)),
        (
            OPENING,
            Answer::reply(
                200,
                opened(
                    &format!("/LiveTv/LiveStreamFiles/{FILE}/stream.ts?api_key=0f0f"),
                    "http://files/channel.ts",
                ),
            ),
        ),
    ]);
    let emby = reader(&fake);
    let tuned = emby.tune(MEMBER, CHANNEL).await;
    assert_eq!(
        tuned.as_ref().ok().cloned().flatten(),
        Some(Tuned {
            closes_by: closes_by(),
            direct: format!("/LiveTv/LiveStreamFiles/{FILE}/stream.ts"),
            transcoded: format!("/LiveTv/LiveStreamFiles/{FILE}/hls/master.m3u8"),
        })
    );
    let opening = fake
        .requests()
        .into_iter()
        .find(|request| request.url.contains(OPENING));
    assert!(opening
        .as_ref()
        .is_some_and(|request| request.method == Method::Post
            && request
                .url
                .contains(&format!("/Items/{CHANNEL}/PlaybackInfo?"))
            && request.url.contains(&format!("UserId={MEMBER}"))
            && request.url.contains("AutoOpenLiveStream=true")
            && request.url.contains("IsPlayback=true")));
    let live = emby
        .declared()
        .await
        .map(|declared| declared.live)
        .unwrap_or_default();
    let Some(Tuned {
        direct, transcoded, ..
    }) = tuned.ok().flatten()
    else {
        return;
    };
    for path in [direct, transcoded] {
        let covered = live.iter().any(|live| {
            Regex::new(&live.pattern)
                .ok()
                .and_then(|pattern| pattern.captures(&path))
                .and_then(|found| {
                    found
                        .name("stream")
                        .map(|stream| stream.as_str().to_owned())
                })
                .is_some_and(|stream| {
                    live.parameter.is_none()
                        && Regex::new(&live.stream).is_ok_and(|shape| shape.is_match(&stream))
                })
        });
        assert!(covered, "{path} is covered by no live path");
    }
}

#[tokio::test]
async fn the_buffer_is_found_in_the_path_where_the_direct_address_does_not_name_it() {
    let fake = licensed(vec![
        (HOLDER, Answer::reply(200, WATCHES)),
        (CHANNELS, Answer::reply(200, SHOWN)),
        (
            OPENING,
            Answer::reply(
                200,
                opened(
                    "",
                    &format!("http://127.0.0.1:8096/emby/LiveTv/LiveStreamFiles/{FILE}/Stream.MKV"),
                ),
            ),
        ),
    ]);
    let tuned = reader(&fake).tune(MEMBER, CHANNEL).await.ok().flatten();
    assert_eq!(
        tuned.map(|tuned| tuned.direct),
        Some(format!("/LiveTv/LiveStreamFiles/{FILE}/stream.mkv"))
    );
}

#[tokio::test]
async fn a_live_stream_opened_where_the_guard_serves_it_from_nowhere_is_closed_and_refused() {
    for (direct, path) in [
        (String::new(), "http://files/channel.ts".to_owned()),
        (
            format!("/LiveTv/LiveStreamFiles/{}/stream.ts", FILE.to_uppercase()),
            String::new(),
        ),
        (
            format!("/LiveTv/LiveStreamFiles/{FILE}/stream."),
            String::new(),
        ),
        (
            format!("/LiveTv/LiveStreamFiles/{FILE}/stream.t/s"),
            String::new(),
        ),
        (
            format!("/LiveTv/LiveStreamFiles/{FILE}/hls/live.m3u8"),
            String::new(),
        ),
    ] {
        let fake = licensed(vec![
            (HOLDER, Answer::reply(200, WATCHES)),
            (CHANNELS, Answer::reply(200, SHOWN)),
            (OPENING, Answer::reply(200, opened(&direct, &path))),
            (CLOSING, Answer::reply(204, "")),
        ]);
        assert!(matches!(
            reader(&fake).tune(MEMBER, CHANNEL).await,
            Err(Failure::Refused { .. })
        ));
        assert!(fake
            .requests()
            .iter()
            .any(|request| request.url.contains(CLOSING)
                && request.url.contains(&format!("LiveStreamId={LIVE_STREAM}"))
                && request
                    .url
                    .contains(&format!("PlaySessionId={PLAY_SESSION}"))));
    }
}

#[tokio::test]
async fn emby_refusing_to_open_the_channel_for_the_member_is_nothing_and_any_other_failure_an_error(
) {
    let refusing = |answer: Answer| {
        licensed(vec![
            (HOLDER, Answer::reply(200, WATCHES)),
            (CHANNELS, Answer::reply(200, SHOWN)),
            (OPENING, answer),
        ])
    };
    let not_allowed = refusing(Answer::reply(
        200,
        r#"{"ErrorCode":"NotAllowed","MediaSources":[]}"#,
    ));
    assert_eq!(
        reader(&not_allowed).tune(MEMBER, CHANNEL).await.ok(),
        Some(None)
    );
    for answer in [
        Answer::reply(
            200,
            r#"{"ErrorCode":"NoCompatibleStream","MediaSources":[]}"#,
        ),
        Answer::reply(
            200,
            r#"{"PlaySessionId":"986f9938c33c4e149a9ed1efdb4cb08f","MediaSources":[]}"#,
        ),
        Answer::reply(
            200,
            r#"{"PlaySessionId":"986f","MediaSources":[{"LiveStreamId":"a"}]}"#,
        ),
        Answer::reply(
            200,
            r#"{"PlaySessionId":"986f9938c33c4e149a9ed1efdb4cb08f","MediaSources":[{"Path":"/x"}]}"#,
        ),
        Answer::reply(500, ""),
    ] {
        assert!(reader(&refusing(answer))
            .tune(MEMBER, CHANNEL)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn untuning_closes_the_live_stream_under_the_play_session_that_opened_it() {
    let fake = licensed(vec![(CLOSING, Answer::reply(204, ""))]);
    assert!(reader(&fake).untune(&closes_by()).await.is_ok());
    let closing = fake
        .requests()
        .into_iter()
        .find(|request| request.url.contains(CLOSING));
    assert!(closing.is_some_and(|request| request.method == Method::Post
        && request.url.ends_with(&format!(
            "{CLOSING}?LiveStreamId={LIVE_STREAM}&PlaySessionId={PLAY_SESSION}"
        ))));
    let refused = licensed(vec![(CLOSING, Answer::reply(500, ""))]);
    assert!(reader(&refused).untune(&closes_by()).await.is_err());
}

#[tokio::test]
async fn ids_of_another_shape_never_reach_a_path() {
    let fake = licensed(vec![
        (HOLDER, Answer::reply(200, WATCHES)),
        (CHANNELS, Answer::reply(200, SHOWN)),
    ]);
    let emby = reader(&fake);
    assert!(matches!(
        emby.channels("../Users").await,
        Err(Failure::Refused { .. })
    ));
    assert!(matches!(
        emby.tune("../Users", CHANNEL).await,
        Err(Failure::Refused { .. })
    ));
    assert_eq!(emby.tune(MEMBER, "12/../13").await.ok(), Some(None));
    for misshapen in [
        "conformance-live-stream",
        LIVE_STREAM,
        &format!("{LIVE_STREAM}.986f"),
        &format!("a/b.{PLAY_SESSION}"),
        &format!(".{PLAY_SESSION}"),
    ] {
        assert!(matches!(
            emby.untune(misshapen).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert!(fake
        .requests()
        .iter()
        .all(|request| request.url.contains(SIGNING_IN) || request.url.contains(LICENCE)));
}

fn guided() -> Arc<Fake> {
    guided_as(WATCHES, RATED)
}

/// Channel One with an unrated news bulletin on now.
const SHOWN_NEWS: &str = r#"{"Items":[{"Id":"12","Name":"Channel One","Type":"TvChannel","ChannelNumber":"1","CurrentProgram":{"Id":"41","Name":"Evening News","OfficialRating":""}}],"TotalRecordCount":1}"#;

/// Channel One with a cartoon rated TV-PG on now.
const SHOWN_PG: &str = r#"{"Items":[{"Id":"12","Name":"Channel One","Type":"TvChannel","ChannelNumber":"1","CurrentProgram":{"Id":"44","Name":"Cartoons","OfficialRating":"TV-PG","Tags":["Cartoon"]}}],"TotalRecordCount":1}"#;

fn guided_as(holder: &'static str, rated: &'static str) -> Arc<Fake> {
    guided_on(holder, rated, SHOWN)
}

fn guided_on(holder: &'static str, rated: &'static str, shown: &'static str) -> Arc<Fake> {
    licensed(vec![
        (HOLDER, Answer::reply(200, holder)),
        (CHANNELS, Answer::reply(200, shown)),
        (PROGRAMMES, Answer::reply(200, SCHEDULED)),
        (RATINGS, Answer::reply(200, rated)),
    ])
}

fn instant(stamp: &str) -> Option<lemonfiber_ports::Instant> {
    lemonfiber_ports::Instant::from_service_text(stamp)
}

fn aired(title: &str, certificate: Option<&str>, starts: &str, ends: &str) -> Option<Airing> {
    Some(Airing {
        title: title.to_owned(),
        certificate: certificate.map(str::to_owned),
        starts: lemonfiber_ports::Instant::from_service_text(starts)?,
        ends: lemonfiber_ports::Instant::from_service_text(ends)?,
    })
}

fn query(url: &str, name: &str) -> Option<String> {
    url.split_once('?')?
        .1
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(field, _)| *field == name)
        .map(|(_, value)| value.replace("%3A", ":").replace("%2C", ","))
}

#[tokio::test]
async fn each_channel_s_guide_holds_its_dated_programmes_in_the_order_they_air() {
    let fake = guided();
    let programmes: Vec<Airing> = [
        aired(
            "Morning Film",
            Some("TV-14"),
            "2026-11-13T17:00:00Z",
            "2026-11-13T18:00:00Z",
        ),
        aired(
            "Evening News",
            None,
            "2026-11-13T18:00:00Z",
            "2026-11-13T19:00:00Z",
        ),
        aired(
            "Fright Night",
            Some("TV-PG"),
            "2026-11-13T19:00:00Z",
            "2026-11-13T20:30:00Z",
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_eq!(programmes.len(), 3);
    assert_eq!(
        reader(&fake).guide(MEMBER, 12).await.ok(),
        Some(vec![
            Listing {
                channel: CHANNEL.to_owned(),
                programmes,
                until: instant("2026-11-13T20:30:00Z"),
            },
            Listing {
                channel: "13".to_owned(),
                programmes: Vec::new(),
                until: None,
            },
        ])
    );
    assert!(!asked_for(&fake, RATINGS));
    assert!(fake
        .requests()
        .iter()
        .any(|request| request.url.contains(CHANNELS)
            && request.url.contains(&format!("UserId={MEMBER}"))));
}

#[tokio::test]
async fn the_guide_is_read_across_the_hours_asked_from_now_once_whole_and_once_as_the_member_may_watch_it(
) {
    let fake = guided();
    let before = lemonfiber_ports::Instant::of(std::time::SystemTime::now());
    assert!(reader(&fake).guide(MEMBER, 30).await.is_ok());
    let after = lemonfiber_ports::Instant::of(std::time::SystemTime::now());
    let url = fake
        .requests()
        .into_iter()
        .find(|request| request.url.contains(PROGRAMMES))
        .map(|request| request.url)
        .unwrap_or_default();
    let read = |field: &str| {
        query(&url, field)
            .as_deref()
            .and_then(lemonfiber_ports::Instant::from_service_text)
    };
    let from = read("MinEndDate");
    assert!(
        from.is_some_and(|from| before <= from && from <= after),
        "the guide was not read from now: {url}"
    );
    assert_eq!(
        read("MaxStartDate")
            .zip(from)
            .map(|(until, from)| until.since(from)),
        Some(30 * lemonfiber_ports::instant::HOUR)
    );
    assert_eq!(query(&url, "ChannelIds").as_deref(), Some("12,13"));
    assert!(
        query(&url, "IsAiring").is_none(),
        "the guide was read only for now: {url}"
    );
    let asked: Vec<String> = fake
        .requests()
        .into_iter()
        .filter(|request| request.url.contains(PROGRAMMES))
        .map(|request| request.url)
        .collect();
    assert_eq!(
        asked
            .iter()
            .map(|url| query(url, "UserId"))
            .collect::<Vec<_>>(),
        vec![None, Some(MEMBER.to_owned())]
    );
    assert!(asked
        .iter()
        .all(|url| query(url, "Fields").as_deref() == Some("Tags")));
}

#[tokio::test]
async fn a_channel_whose_programme_on_now_the_member_s_policy_keeps_from_them_is_left_out_of_the_guide_channels_and_tune(
) {
    let fake = guided_as(GUARDED, RATED);
    assert_eq!(reader(&fake).guide(MEMBER, 12).await.ok(), Some(Vec::new()));
    assert_eq!(reader(&fake).channels(MEMBER).await.ok(), Some(Vec::new()));
    assert_eq!(reader(&fake).tune(MEMBER, CHANNEL).await.ok(), Some(None));
}

#[tokio::test]
async fn a_programme_kept_from_the_member_is_left_out_of_a_channel_they_may_watch_and_the_guide_still_ends_where_emby_s_does(
) {
    let fake = guided_on(OFF_THE_LADDER, RATED, SHOWN_NEWS);
    assert_eq!(
        reader(&fake)
            .guide(MEMBER, 12)
            .await
            .ok()
            .and_then(|listings| listings.into_iter().next())
            .map(|listing| listing.until),
        Some(instant("2026-11-13T20:30:00Z"))
    );
}

#[tokio::test]
async fn a_guide_whose_ratings_cannot_be_read_is_refused() {
    let fake = guided_as(GUARDED, "<html>");
    assert!(matches!(
        reader(&fake).guide(MEMBER, 12).await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn a_member_shown_no_channel_asks_no_guide() {
    let fake = licensed(vec![
        (HOLDER, Answer::reply(200, WATCHES)),
        (CHANNELS, Answer::reply(200, r#"{"Items":[]}"#)),
        (PROGRAMMES, Answer::reply(200, SCHEDULED)),
    ]);
    assert_eq!(reader(&fake).guide(MEMBER, 12).await.ok(), Some(Vec::new()));
    assert!(!asked_for(&fake, PROGRAMMES));
}

#[tokio::test]
async fn a_guide_asked_outside_its_hours_or_for_a_misshapen_member_is_refused_before_anything_is_read(
) {
    let fake = guided();
    let emby = reader(&fake);
    for hours in [0, 169] {
        assert!(matches!(
            emby.guide(MEMBER, hours).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert!(matches!(
        emby.guide("../Users", 12).await,
        Err(Failure::Refused { .. })
    ));
    assert!(!asked_for(&fake, HOLDER) && !asked_for(&fake, PROGRAMMES));
}

#[tokio::test]
async fn a_guide_emby_answers_unreadably_is_refused() {
    let fake = licensed(vec![
        (HOLDER, Answer::reply(200, WATCHES)),
        (CHANNELS, Answer::reply(200, SHOWN)),
        (PROGRAMMES, Answer::reply(200, "<html>")),
    ]);
    assert!(matches!(
        reader(&fake).guide(MEMBER, 12).await,
        Err(Failure::Refused { .. })
    ));
}

fn titles(listings: Option<Vec<Listing>>) -> Option<Vec<String>> {
    listings?.into_iter().next().map(|listing| {
        listing
            .programmes
            .into_iter()
            .map(|airing| airing.title)
            .collect()
    })
}

#[tokio::test]
async fn under_an_age_limit_a_programme_whose_certificate_emby_s_ratings_do_not_name_is_left_out() {
    let fake = guided_as(ADULT, r#"[{"Name":"TV-14","Value":8}]"#);
    assert_eq!(
        titles(reader(&fake).guide(MEMBER, 12).await.ok()),
        Some(vec!["Morning Film".to_owned(), "Evening News".to_owned()])
    );
}

#[tokio::test]
async fn a_rating_limit_off_emby_s_ladder_still_limits_and_one_below_it_is_the_strictest() {
    let fake = guided_on(OFF_THE_LADDER, RATED, SHOWN_NEWS);
    assert_eq!(
        titles(reader(&fake).guide(MEMBER, 12).await.ok()),
        Some(vec!["Evening News".to_owned()])
    );
}

#[tokio::test]
async fn on_a_channel_whose_programme_on_now_the_member_may_watch_a_later_programme_carrying_a_blocked_tag_is_left_out(
) {
    let fake = guided_on(GUARDED, RATED, SHOWN_PG);
    let listing = reader(&fake)
        .guide(MEMBER, 12)
        .await
        .ok()
        .and_then(|listings| listings.into_iter().next());
    assert_eq!(
        listing.as_ref().map(|listing| listing.until),
        Some(instant("2026-11-13T20:30:00Z"))
    );
    assert!(listing.is_some_and(|listing| listing
        .programmes
        .iter()
        .all(|airing| airing.title != "Fright Night")));
}
