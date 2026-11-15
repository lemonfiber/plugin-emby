use lemonfiber_fixtures::http::Fake;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Addressee, Answers, Declared, Declaring as _, Held, Presentation, MAY,
};

use super::emby;

mod door;

const PREFIXED: &str = "(?i)^(?:/emby|/mediabrowser)?";

async fn declared() -> Option<Declared> {
    emby(&Fake::silent()).declared().await.ok()
}

#[tokio::test]
async fn emby_is_not_asked_anything_to_answer_the_declaration() {
    let fake = Fake::silent();
    assert!(emby(&fake).declared().await.is_ok());
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn the_declaration_is_one_a_guard_can_enforce() {
    let declared = declared().await;
    assert!(declared.is_some());
    assert_eq!(
        declared
            .clone()
            .and_then(|declared| declared.checked().ok()),
        declared
    );
}

#[tokio::test]
async fn every_byte_route_is_matched_under_both_prefixes_and_asks_the_adapter_in_one_space() {
    let bytes = declared()
        .await
        .map(|declared| declared.bytes)
        .unwrap_or_default();
    assert_eq!(
        bytes
            .iter()
            .map(|path| (
                path.pattern.as_str(),
                path.item.as_str(),
                path.space.as_deref()
            ))
            .collect::<Vec<_>>(),
        [
            (
                format!("{PREFIXED}/(?:Videos|Items)/[^/]+/(?P<item>[^/]+)/Subtitles(?:/|$)")
                    .as_str(),
                "(?i)^(?:mediasource_)?[0-9]{1,19}$",
                Some("item"),
            ),
            (
                format!("{PREFIXED}/(?:Videos|Audio|LiveTv/LiveRecordings)/(?P<item>[^/]+)(?:/|$)")
                    .as_str(),
                "^[0-9]{1,19}$",
                Some("item"),
            ),
            (
                format!(
                    "{PREFIXED}/Items/(?P<item>[^/]+)/(?:Images|Download|File|ThumbnailSet)(?:/|$)"
                )
                .as_str(),
                "^[0-9]{1,19}$",
                Some("item"),
            ),
        ]
    );
    assert_eq!(
        bytes
            .iter()
            .map(|path| path.exempt.clone())
            .collect::<Vec<_>>(),
        [
            Vec::new(),
            vec![format!(
                "{PREFIXED}/Videos/(?:ActiveEncodings|MergeVersions)(?:/|$)"
            )],
            Vec::new(),
        ]
    );
    for path in &bytes {
        assert_eq!(
            path.naming
                .iter()
                .map(|named| (named.parameter.as_str(), named.held.clone()))
                .collect::<Vec<_>>(),
            [
                ("ItemId", Held::Refused),
                ("LiveStreamId", Held::Refused),
                (
                    "MediaSourceId",
                    Held::Asked {
                        shape: "(?i)^(?:mediasource_)?[0-9]{1,19}$".to_owned()
                    }
                ),
            ]
        );
    }
}

#[tokio::test]
async fn a_live_stream_a_picture_found_by_a_name_and_a_sync_download_are_never_asked_about() {
    let declared = declared().await;
    assert_eq!(
        declared.as_ref().map(|declared| declared
            .live
            .iter()
            .map(|live| (live.pattern.clone(), live.stream.clone()))
            .collect::<Vec<_>>()),
        Some(vec![(
            format!("{PREFIXED}/LiveTv/LiveStreamFiles/(?P<stream>[^/]+)/(?:stream\\.|hls/)"),
            "^[0-9a-f]{32}$".to_owned()
        )])
    );
    assert_eq!(
        declared.map(|declared| declared.refused),
        Some(vec![
            format!("{PREFIXED}/(?:Artists|Genres|GameGenres|MusicGenres|Persons|Studios)/[^/]+/Images(?:/|$)"),
            format!("{PREFIXED}/Sync/JobItems/[^/]+/(?:File|AdditionalFiles)(?:/|$)"),
        ])
    );
}

#[tokio::test]
async fn the_guard_asks_the_adapter_and_presents_the_token_where_emby_reads_it_first() {
    let declared = declared().await;
    assert_eq!(
        declared
            .as_ref()
            .and_then(|declared| declared.question.as_ref())
            .map(|asked| (
                asked.method,
                asked.path.as_str(),
                asked.to,
                asked.answers.clone()
            )),
        Some((
            Method::Get,
            MAY,
            Addressee::Adapter,
            Answers::from_adapter()
        ))
    );
    let token = declared.map(|declared| declared.token);
    assert_eq!(
        token.as_ref().map(|token| &token.presented),
        Some(&Presentation::Header {
            name: "X-Emby-Token".to_owned(),
            value: "{token}".to_owned()
        })
    );
    assert_eq!(
        token.map(|token| (token.shape, token.parameters, token.headers)),
        Some((
            "^[0-9a-f]{32}$".to_owned(),
            vec!["api_key".to_owned(), "X-Emby-Token".to_owned()],
            vec!["X-MediaBrowser-Token".to_owned()]
        ))
    );
}

#[tokio::test]
async fn an_item_is_streamed_played_in_a_browser_and_pictured_where_emby_serves_it_beneath_the_door(
) {
    let declared = declared().await;
    let played = "{base}/Videos/{item}/master.m3u8?MediaSourceId=mediasource_{item}&PlaySessionId={play}&VideoCodec=h264&AudioCodec=aac,mp3,ac3,eac3";
    assert_eq!(
        declared.as_ref().map(|declared| (
            declared.locations.stream.as_str(),
            declared.locations.poster.as_str(),
            declared.locations.backdrop.as_str(),
            declared.locations.playlist.as_deref()
        )),
        Some((
            played,
            "{base}/Items/{item}/Images/Primary",
            "{base}/Items/{item}/Images/Backdrop",
            Some(played),
        ))
    );
    assert_eq!(
        declared.map(|declared| (
            declared.ports.upstream,
            declared.ports.encrypted,
            declared.ports.internal_tls
        )),
        Some((8096, Some(8920), Some(8920)))
    );
}
