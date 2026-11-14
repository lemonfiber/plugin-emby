use async_trait::async_trait;

use super::screening::picture_path;
use super::{Emby, TOKEN_HEADER};
use crate::shape::SOURCE_PREFIX;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Addressee, Answers, BytePath, Declared, Declaring, Failure, Held, LivePath, Locations, Naming,
    Picture, Ports, Presentation, Question, Token, BASE, ITEM, MAY, PLAY, TOKEN,
};

const PORT: u16 = 8096;

pub(super) const INTERNAL_TLS: u16 = 8920;

pub(super) const SPACE: &str = "item";

const PREFIXED: &str = "(?i)^(?:/emby|/mediabrowser)?";

const ITEM_SEGMENT: &str = "(?P<item>[^/]+)";

const STREAM_SEGMENT: &str = "(?P<stream>[^/]+)";

const ITEM_SHAPE: &str = "^[0-9]{1,19}$";

const SOURCE_SHAPE: &str = "(?i)^(?:mediasource_)?[0-9]{1,19}$";

const HEX: &str = "^[0-9a-f]{32}$";

const TOKEN_PARAMETERS: [&str; 2] = ["api_key", "X-Emby-Token"];

const TOKEN_HEADERS: [&str; 1] = ["X-MediaBrowser-Token"];

const REFUSED_PARAMETERS: [&str; 2] = ["ItemId", "LiveStreamId"];

const SOURCE_PARAMETER: &str = "MediaSourceId";

const KEYED: &str = "(?:Videos|Audio|LiveTv/LiveRecordings)";

const UNKEYED: &str = "Videos/(?:ActiveEncodings|MergeVersions)(?:/|$)";

const PICTURED: &str = "(?:Images|Download|File|ThumbnailSet)";

const BY_NAME: &str = "(?:Artists|Genres|GameGenres|MusicGenres|Persons|Studios)";

const STREAMED: &str = "VideoCodec=h264&AudioCodec=aac,mp3,ac3,eac3";

/// How the guard presents a member's token to Emby: in the header Emby reads it from
/// before any other.
#[must_use]
pub fn presented() -> Presentation {
    Presentation::Header {
        name: TOKEN_HEADER.to_owned(),
        value: TOKEN.to_owned(),
    }
}

fn naming() -> Vec<Naming> {
    let mut naming: Vec<Naming> = REFUSED_PARAMETERS
        .map(|parameter| Naming {
            parameter: parameter.to_owned(),
            held: Held::Refused,
        })
        .to_vec();
    naming.push(Naming {
        parameter: SOURCE_PARAMETER.to_owned(),
        held: Held::Asked {
            shape: SOURCE_SHAPE.to_owned(),
        },
    });
    naming
}

fn byte_path(route: &str, item: &str, exempt: Vec<String>) -> BytePath {
    BytePath {
        pattern: format!("{PREFIXED}/{route}"),
        query: None,
        space: Some(SPACE.to_owned()),
        item: item.to_owned(),
        exempt,
        naming: naming(),
    }
}

fn playlist() -> String {
    format!(
        "{BASE}/Videos/{ITEM}/master.m3u8?MediaSourceId={SOURCE_PREFIX}{ITEM}&PlaySessionId={PLAY}&{STREAMED}"
    )
}

fn declared() -> Declared {
    Declared {
        bytes: vec![
            byte_path(
                &format!("(?:Videos|Items)/[^/]+/{ITEM_SEGMENT}/Subtitles(?:/|$)"),
                SOURCE_SHAPE,
                Vec::new(),
            ),
            byte_path(
                &format!("{KEYED}/{ITEM_SEGMENT}(?:/|$)"),
                ITEM_SHAPE,
                vec![format!("{PREFIXED}/{UNKEYED}")],
            ),
            byte_path(
                &format!("Items/{ITEM_SEGMENT}/{PICTURED}(?:/|$)"),
                ITEM_SHAPE,
                Vec::new(),
            ),
        ],
        live: vec![LivePath {
            pattern: format!(
                "{PREFIXED}/LiveTv/LiveStreamFiles/{STREAM_SEGMENT}/(?:stream\\.|hls/)"
            ),
            parameter: None,
            stream: HEX.to_owned(),
        }],
        refused: vec![
            format!("{PREFIXED}/{BY_NAME}/[^/]+/Images(?:/|$)"),
            format!("{PREFIXED}/Sync/JobItems/[^/]+/(?:File|AdditionalFiles)(?:/|$)"),
        ],
        question: Some(Question {
            method: Method::Get,
            path: MAY.to_owned(),
            to: Addressee::Adapter,
            answers: Answers::from_adapter(),
        }),
        token: Token {
            presented: presented(),
            shape: HEX.to_owned(),
            parameters: TOKEN_PARAMETERS.map(str::to_owned).to_vec(),
            headers: TOKEN_HEADERS.map(str::to_owned).to_vec(),
        },
        locations: Locations {
            stream: playlist(),
            poster: format!("{BASE}{}", picture_path(ITEM, Picture::Poster)),
            backdrop: format!("{BASE}{}", picture_path(ITEM, Picture::Backdrop)),
            playlist: Some(playlist()),
        },
        ports: Ports {
            upstream: PORT,
            encrypted: Some(INTERNAL_TLS),
            internal_tls: Some(INTERNAL_TLS),
        },
    }
}

#[async_trait]
impl Declaring for Emby {
    async fn declared(&self) -> Result<Declared, Failure> {
        Ok(declared())
    }
}
