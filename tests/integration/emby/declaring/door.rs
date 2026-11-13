use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use emby_adapter::credential::Credential;
use emby_adapter::emby::Emby;
use lemonfiber_ports::http::{Http, Request, Response, Unreachable};
use lemonfiber_ports::service::{BytePath, Declared, Held, Presentation, TOKEN};
use regex::Regex;
use tower::ServiceExt as _;

use super::declared;

const KEY: &str = "efefefefefefefefefefefefefefefef";

const MEMBER: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const UNKNOWN: &str = "0000000000000000000000000000dead";

const OPEN: &str = "8";

const CLOSED: &str = "9";

type Case<'a> = (&'a str, String, Vec<(&'a str, &'a str)>, bool);

struct Shown;

#[async_trait]
impl Http for Shown {
    async fn send(&self, request: &Request) -> Result<Response, Unreachable> {
        let token = request
            .headers
            .iter()
            .find(|(name, _)| name == "X-Emby-Token")
            .map(|(_, value)| value.as_str());
        let answered = |status: u16, body: &str| Response {
            status,
            headers: Vec::new(),
            body: body.to_owned(),
        };
        if token != Some(MEMBER) {
            return Ok(answered(401, ""));
        }
        let shown = if request.url.ends_with(&format!("Ids={OPEN}")) {
            format!(r#"{{"Items":[{{"Id":"{OPEN}"}}]}}"#)
        } else {
            r#"{"Items":[]}"#.to_owned()
        };
        Ok(answered(200, &shown))
    }
}

fn adapter() -> axum::Router {
    let held = Credential {
        username: "admin".to_owned(),
        password: ('a'..='p').collect(),
    };
    let emby = Emby::new(Arc::new(Shown), "http://emby:8096", Arc::new(held));
    emby_adapter::adapter::kit(KEY.to_owned(), Arc::new(emby)).router()
}

fn matching(pattern: &str, text: &str) -> bool {
    Regex::new(pattern).is_ok_and(|compiled| compiled.is_match(text))
}

fn pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (name.to_owned(), value.to_owned())
        })
        .collect()
}

fn read(
    declared: &Declared,
    pairs: &[(String, String)],
    headers: &[(&str, &str)],
) -> Option<(Option<String>, bool)> {
    let token = &declared.token;
    let shaped = |value: &str| matching(&token.shape, value);
    let mut tokens: Vec<String> = Vec::new();
    for (name, value) in pairs {
        let Some(declared) = token
            .parameters
            .iter()
            .find(|one| one.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        if declared != name || !shaped(value) {
            return None;
        }
        tokens.push(value.clone());
    }
    let header = |wanted: &str| {
        headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| (*value).to_owned())
    };
    let mut unread = false;
    if let Some(authorization) = header("Authorization") {
        match authorization
            .strip_prefix("Bearer ")
            .filter(|value| shaped(value))
        {
            Some(bearer) => tokens.push(bearer.to_owned()),
            None => unread = true,
        }
    }
    for name in &token.headers {
        if let Some(value) = header(name) {
            if !shaped(&value) {
                return None;
            }
            tokens.push(value);
        }
    }
    if let Presentation::Header { name, value } = &token.presented {
        if let Some(held) = header(name) {
            let (before, after) = value.split_once(TOKEN)?;
            match held
                .strip_prefix(before)
                .and_then(|held| held.strip_suffix(after))
                .filter(|held| shaped(held))
            {
                Some(held) => tokens.push(held.to_owned()),
                None => unread = true,
            }
        }
    }
    tokens.sort_unstable();
    tokens.dedup();
    (tokens.len() <= 1).then(|| (tokens.pop(), unread))
}

fn items(bytes: &BytePath, path: &str, pairs: &[(String, String)]) -> Option<Vec<String>> {
    let item = Regex::new(&bytes.pattern)
        .ok()?
        .captures(path)?
        .name("item")?
        .as_str()
        .to_owned();
    if !matching(&bytes.item, &item) {
        return None;
    }
    let mut items = vec![item];
    for named in &bytes.naming {
        for (name, value) in pairs
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(&named.parameter))
        {
            match &named.held {
                Held::Asked { shape } if *name == named.parameter && matching(shape, value) => {
                    items.push(value.clone());
                }
                _ => return None,
            }
        }
    }
    Some(items)
}

async fn may(item: &str, token: Option<&str>) -> bool {
    let mut request = axum::http::Request::builder().method("GET").uri(format!(
        "/lemonfiber/media.serve/v1/may?space=item&item={item}"
    ));
    if let Some(token) = token {
        request = request.header("X-Emby-Token", token);
    }
    let Ok(request) = request.body(Body::empty()) else {
        return false;
    };
    let Ok(answered) = adapter().oneshot(request).await;
    answered.status() == axum::http::StatusCode::NO_CONTENT
}

async fn passes(declared: &Declared, target: &str, headers: &[(&str, &str)]) -> bool {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let pairs = pairs(query);
    let Some((token, unread)) = read(declared, &pairs, headers) else {
        return false;
    };
    if declared
        .refused
        .iter()
        .any(|pattern| matching(pattern, path))
    {
        return false;
    }
    let Some(bytes) = declared
        .bytes
        .iter()
        .find(|bytes| matching(&bytes.pattern, path))
    else {
        return true;
    };
    if bytes.exempt.iter().any(|pattern| matching(pattern, path)) {
        return true;
    }
    let Some(items) = items(bytes, path, &pairs).filter(|_| !unread) else {
        return false;
    };
    for item in items {
        if !may(&item, token.as_deref()).await {
            return false;
        }
    }
    true
}

#[tokio::test]
async fn a_guard_enforcing_the_declaration_passes_only_what_the_member_may_have() {
    let declared = declared().await;
    assert!(declared.is_some());
    let Some(declared) = declared else {
        return;
    };
    let member = [("X-Emby-Token", MEMBER)];
    let bearer = format!("Bearer {MEMBER}");
    let cases: Vec<Case<'_>> = vec![
        ("stream presenting nothing", format!("/Videos/{OPEN}/stream?static=true"), vec![], false),
        ("stream with the member's token", format!("/Videos/{OPEN}/stream?static=true"), member.to_vec(), true),
        ("stream outside the member's libraries", format!("/Videos/{CLOSED}/stream?static=true"), member.to_vec(), false),
        ("stream with an unknown token", format!("/Videos/{OPEN}/stream"), vec![("X-Emby-Token", UNKNOWN)], false),
        ("stream with api_key", format!("/Videos/{OPEN}/stream?api_key={MEMBER}"), vec![], true),
        ("stream with X-Emby-Token in the query", format!("/Videos/{OPEN}/stream?X-Emby-Token={MEMBER}"), vec![], true),
        ("stream with another spelling of api_key", format!("/Videos/{OPEN}/stream?API_KEY={MEMBER}"), vec![], false),
        ("stream with X-MediaBrowser-Token", format!("/Videos/{OPEN}/stream"), vec![("X-MediaBrowser-Token", MEMBER)], true),
        ("stream with a bearer grant", format!("/Videos/{OPEN}/stream"), vec![("Authorization", bearer.as_str())], true),
        ("closed stream with a bearer grant", format!("/Videos/{CLOSED}/stream"), vec![("Authorization", bearer.as_str())], false),
        ("stream with Emby's own authorization", format!("/Videos/{OPEN}/stream"), vec![("Authorization", r#"Emby Token="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""#)], false),
        ("stream with two tokens", format!("/Videos/{OPEN}/stream?api_key={MEMBER}"), vec![("X-Emby-Token", UNKNOWN)], false),
        ("stream named by no item id", "/Videos/abc/stream".to_owned(), member.to_vec(), false),
        ("image presenting nothing", format!("/Items/{OPEN}/Images/Primary"), vec![], false),
        ("image with the member's token", format!("/Items/{OPEN}/Images/Primary"), member.to_vec(), true),
        ("image outside the member's libraries", format!("/Items/{CLOSED}/Images/Backdrop/0"), member.to_vec(), false),
        ("image under the legacy prefix presenting nothing", format!("/emby/items/{OPEN}/images/primary"), vec![], false),
        ("download outside the member's libraries", format!("/Items/{CLOSED}/Download"), member.to_vec(), false),
        ("audio outside the member's libraries", format!("/Audio/{CLOSED}/stream.mp3"), member.to_vec(), false),
        ("recording presenting nothing", format!("/LiveTv/LiveRecordings/{OPEN}/stream"), vec![], false),
        ("subtitles of another item's source", format!("/Videos/{OPEN}/mediasource_{CLOSED}/Subtitles/0/Stream.vtt"), member.to_vec(), false),
        ("subtitles of the item's own source", format!("/Videos/{OPEN}/mediasource_{OPEN}/Subtitles/0/Stream.vtt"), member.to_vec(), true),
        ("thumbnails of another item's source", format!("/Videos/{OPEN}/index.bif?MediaSourceId=mediasource_{CLOSED}"), member.to_vec(), false),
        ("the location the core gives a member", format!("/Videos/{OPEN}/master.m3u8?MediaSourceId=mediasource_{OPEN}&PlaySessionId=lemonfiber-{OPEN}&VideoCodec=h264&AudioCodec=aac,mp3,ac3,eac3&api_key={MEMBER}"), vec![], true),
        ("the playlist the master names", format!("/Videos/{OPEN}/main.m3u8?MediaSourceId=mediasource_{OPEN}&PlaySessionId=lemonfiber-{OPEN}&VideoCodec=h264&AudioCodec=aac,mp3,ac3,eac3&SegmentContainer=ts"), member.to_vec(), true),
        ("a segment the playlist names", format!("/Videos/{OPEN}/hls1/main/0.ts?PlaySessionId=lemonfiber-{OPEN}"), member.to_vec(), true),
        ("a segment presenting nothing", format!("/Videos/{OPEN}/hls1/main/0.ts?PlaySessionId=lemonfiber-{OPEN}"), vec![], false),
        ("a segment of an item outside the member's libraries", format!("/Videos/{CLOSED}/hls1/main/0.ts?PlaySessionId=lemonfiber-{CLOSED}"), member.to_vec(), false),
        ("another item named in the query", format!("/Videos/{OPEN}/stream?ItemId={CLOSED}"), member.to_vec(), false),
        ("a live stream named in the query", format!("/Videos/{OPEN}/stream?LiveStreamId=x"), member.to_vec(), false),
        ("a picture found by a name", "/Artists/Someone/Images/Primary".to_owned(), member.to_vec(), false),
        ("a sync download", "/Sync/JobItems/1/File".to_owned(), member.to_vec(), false),
        ("the encodings Emby is running", "/Videos/ActiveEncodings".to_owned(), vec![], true),
        ("a path serving no item's bytes", "/System/Info/Public".to_owned(), vec![], true),
    ];
    let mut differing = Vec::new();
    for (name, target, headers, expected) in &cases {
        if passes(&declared, target, headers).await != *expected {
            differing.push(*name);
        }
    }
    assert!(differing.is_empty(), "{differing:#?}");
}
