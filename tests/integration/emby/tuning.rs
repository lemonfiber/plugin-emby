use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Channel, Declaring as _, Failure, Programme, Tuned, Tuning as _};
use regex::Regex;

use super::{reader, MEMBER, SIGNED_IN};

const SIGNING_IN: &str = "AuthenticateByName";

const LICENCE: &str = "/Plugins/SecurityInfo";

const HOLDER: &str = "/Users/4f53cda18c2baa0c0354bb5f9a3ecbe5";

const CHANNELS: &str = "/LiveTv/Channels";

const OPENING: &str = "/PlaybackInfo";

const CLOSING: &str = "/LiveStreams/Close";

const CHANNEL: &str = "12";

const SUPPORTER: &str = r#"{"IsMBSupporter":true}"#;

const WATCHES: &str = r#"{"Id":"4f53cda18c2baa0c0354bb5f9a3ecbe5","Policy":{"EnableLiveTvAccess":true,"IsDisabled":false,"EnableAllChannels":true}}"#;

const SHOWN: &str = r#"{"Items":[{"Id":"12","Name":"Channel One","Type":"TvChannel","ChannelNumber":"1","CurrentProgram":{"Id":"40","Name":"Morning Film","OfficialRating":"TV-14","StartDate":"2026-11-13T17:00:00.0000000Z","EndDate":"2026-11-13T18:00:00.0000000Z"}},{"Id":"13","Name":"Channel Two","Type":"TvChannel","ChannelNumber":"","CurrentProgram":{"Name":"","OfficialRating":"TV-MA"}},{"Id":"../13","Name":"Elsewhere"}],"TotalRecordCount":3}"#;

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
                    ends: Some("2026-11-13T18:00:00.0000000Z".to_owned()),
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
        assert!(!asked_for(&fake, CHANNELS));
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
