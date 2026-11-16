use async_trait::async_trait;

use super::{rating, Emby};
use crate::shape::{a_live_stream_file, a_play_session, Account, Item, LiveStream};
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::instant::HOUR;
use lemonfiber_ports::service::{
    Airing, Channel, Failure, Household as _, Listing, Programme, Tuned, Tuning, GUIDE_HOURS,
};
use lemonfiber_ports::Instant;
use std::collections::HashMap;

const LICENSED: &str = "/Plugins/SecurityInfo";

const CHANNELS: &str = "/LiveTv/Channels";

const CLOSE: &str = "/LiveStreams/Close";

const PROGRAMMES: &str = "/LiveTv/Programs";

const FILES: &str = "/livetv/livestreamfiles/";

const STREAM: &str = "stream.";

const LONGEST_CONTAINER: usize = 8;

const NOT_ALLOWED: &str = "NotAllowed";

const UNLICENSED: &str =
    "Emby runs live TV only under an Emby Premiere subscription, and this Emby did not say it holds one";

const UNREAD_ACCESS: &str = "whether the account may watch live TV could not be read";

const UNREAD_CHANNELS: &str = "the live TV channels could not be read";

const UNREAD_GUIDE: &str = "the guide could not be read";

const UNREAD_OPENED: &str = "the live stream Emby opened could not be read";

const UNSERVED: &str = "Emby opened the live stream at no address the guard serves it from";

const PROFILE: &str = r#"{"DeviceProfile":{"DirectPlayProfiles":[{"Type":"Video","Container":"ts"}],"TranscodingProfiles":[{"Type":"Video","Container":"ts","VideoCodec":"h264","AudioCodec":"aac","Protocol":"hls","Context":"Streaming"}]}}"#;

#[derive(serde::Deserialize)]
struct Licence {
    #[serde(rename = "IsMBSupporter", default)]
    supporter: bool,
}

#[derive(serde::Deserialize)]
struct Holder {
    #[serde(rename = "Policy", default)]
    policy: Access,
}

#[derive(serde::Deserialize, Default)]
struct Access {
    #[serde(rename = "EnableLiveTvAccess", default)]
    live_tv: bool,
    #[serde(rename = "IsDisabled", default)]
    disabled: bool,
    #[serde(rename = "MaxParentalRating", default)]
    level: Option<u32>,
    #[serde(rename = "BlockedTags", default)]
    blocked_tags: Vec<String>,
    #[serde(rename = "BlockUnratedItems", default)]
    unrated_blocked: Vec<String>,
}

#[derive(serde::Deserialize)]
struct ChannelsResource {
    #[serde(rename = "Items", default)]
    items: Vec<ChannelResource>,
}

#[derive(serde::Deserialize)]
struct ChannelResource {
    #[serde(rename = "Id", default)]
    id: String,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "ChannelNumber", default)]
    number: Option<String>,
    #[serde(rename = "CurrentProgram", default)]
    now: Option<ProgrammeResource>,
}

#[derive(serde::Deserialize)]
struct ProgrammesResource {
    #[serde(rename = "Items", default)]
    items: Vec<ProgrammeResource>,
}

#[derive(serde::Deserialize)]
struct ProgrammeResource {
    #[serde(rename = "ChannelId", default)]
    channel: String,
    #[serde(rename = "Name", default)]
    title: String,
    #[serde(rename = "OfficialRating", default)]
    certificate: Option<String>,
    #[serde(rename = "Tags", default)]
    tags: Vec<String>,
    #[serde(
        rename = "StartDate",
        default,
        deserialize_with = "crate::emby::moment"
    )]
    starts: Option<Instant>,
    #[serde(rename = "EndDate", default, deserialize_with = "crate::emby::moment")]
    ends: Option<Instant>,
}

#[derive(serde::Deserialize)]
struct OpenedResource {
    #[serde(rename = "PlaySessionId", default)]
    play_session: String,
    #[serde(rename = "ErrorCode", default)]
    error: Option<String>,
    #[serde(rename = "MediaSources", default)]
    sources: Vec<SourceResource>,
}

#[derive(serde::Deserialize)]
struct SourceResource {
    #[serde(rename = "LiveStreamId", default)]
    live_stream: Option<String>,
    #[serde(rename = "DirectStreamUrl", default)]
    direct: Option<String>,
    #[serde(rename = "Path", default)]
    path: Option<String>,
}

#[async_trait]
impl Tuning for Emby {
    async fn channels(&self, member: &str) -> Result<Vec<Channel>, Failure> {
        licensed(self).await?;
        let member = Account::of(member).ok_or_else(|| self.misshapen("an account"))?;
        if !watches_live_tv(self, member).await? {
            return Ok(Vec::new());
        }
        listed(self, member, None).await
    }

    async fn guide(&self, member: &str, hours: u32) -> Result<Vec<Listing>, Failure> {
        licensed(self).await?;
        let member = Account::of(member).ok_or_else(|| self.misshapen("an account"))?;
        if !GUIDE_HOURS.contains(&hours) {
            return Err(self.endpoint.refused(&format!(
                "a guide is asked across {} to {} hours",
                GUIDE_HOURS.start(),
                GUIDE_HOURS.end()
            )));
        }
        let Some(access) = live_tv_access(self, member).await? else {
            return Ok(Vec::new());
        };
        let channels = listed(self, member, None).await?;
        if channels.is_empty() {
            return Ok(Vec::new());
        }
        let now = Instant::of(std::time::SystemTime::now());
        let (Some(from), Some(until)) = (stamped(now), stamped(now.later(u64::from(hours) * HOUR)))
        else {
            return Err(self
                .endpoint
                .refused("the guide's window falls outside the calendar"));
        };
        let window = Window {
            channels: &channels,
            from: &from,
            until: &until,
        };
        let held = aired(scheduled(self, &window, None).await?);
        let ages = ages(self, &access).await?;
        let mut kept = aired(
            scheduled(self, &window, Some(member))
                .await?
                .into_iter()
                .filter(|programme| admitted(programme, &access, &ages))
                .collect(),
        );
        Ok(channels
            .into_iter()
            .map(|channel| Listing {
                until: held
                    .get(&channel.id)
                    .and_then(|programmes| programmes.iter().map(|airing| airing.ends).max()),
                programmes: kept.remove(&channel.id).unwrap_or_default(),
                channel: channel.id,
            })
            .collect())
    }

    async fn tune(&self, member: &str, channel: &str) -> Result<Option<Tuned>, Failure> {
        licensed(self).await?;
        let member = Account::of(member).ok_or_else(|| self.misshapen("an account"))?;
        let Some(channel) = Item::of(channel) else {
            return Ok(None);
        };
        if !watches_live_tv(self, member).await? {
            return Ok(None);
        }
        let wanted = channel.to_string();
        if !listed(self, member, Some(channel))
            .await?
            .iter()
            .any(|shown| shown.id == wanted)
        {
            return Ok(None);
        }
        opened(self, member, channel).await
    }

    async fn untune(&self, closes_by: &str) -> Result<(), Failure> {
        licensed(self).await?;
        let stream = LiveStream::of(closes_by).ok_or_else(|| self.misshapen("a live stream"))?;
        closed(self, stream).await
    }
}

async fn licensed(emby: &Emby) -> Result<(), Failure> {
    let supporter = emby
        .as_admin(Method::Get, LICENSED, None)
        .await
        .ok()
        .filter(lemonfiber_ports::http::Response::is_success)
        .and_then(|response| serde_json::from_str::<Licence>(&response.body).ok())
        .is_some_and(|licence| licence.supporter);
    if supporter {
        Ok(())
    } else {
        Err(Emby::unsupported(UNLICENSED))
    }
}

async fn watches_live_tv(emby: &Emby, member: Account<'_>) -> Result<bool, Failure> {
    Ok(live_tv_access(emby, member).await?.is_some())
}

async fn live_tv_access(emby: &Emby, member: Account<'_>) -> Result<Option<Access>, Failure> {
    let response = emby
        .as_admin(Method::Get, &format!("/Users/{member}"), None)
        .await?;
    let holder: Holder = emby.endpoint.decode(&response, UNREAD_ACCESS)?;
    let access = holder.policy;
    Ok((access.live_tv && !access.disabled).then_some(access))
}

async fn ages(emby: &Emby, access: &Access) -> Result<HashMap<String, u32>, Failure> {
    if access.level.is_none() {
        return Ok(HashMap::new());
    }
    Ok(emby
        .ratings()
        .await?
        .into_iter()
        .map(|rating| (rating.name.to_lowercase(), rating.age))
        .collect())
}

fn admitted(programme: &ProgrammeResource, access: &Access, ages: &HashMap<String, u32>) -> bool {
    let blocked = programme.tags.iter().any(|tag| {
        access
            .blocked_tags
            .iter()
            .any(|blocked| blocked.eq_ignore_ascii_case(tag))
    });
    let certificate = programme
        .certificate
        .as_deref()
        .filter(|rated| !rated.is_empty());
    let unrated_held = certificate.is_none() && !access.unrated_blocked.is_empty();
    let too_old = access.level.map(rating::age_within).is_some_and(|limit| {
        certificate.is_some_and(|rated| {
            ages.get(&rated.to_lowercase())
                .is_none_or(|age| *age > limit)
        })
    });
    !blocked && !unrated_held && !too_old
}

async fn listed(
    emby: &Emby,
    member: Account<'_>,
    only: Option<Item<'_>>,
) -> Result<Vec<Channel>, Failure> {
    let member = member.to_string();
    let only = only.map(|channel| channel.to_string());
    let mut fields = vec![
        ("UserId", member.as_str()),
        ("AddCurrentProgram", "true"),
        ("EnableImages", "false"),
        ("EnableUserData", "false"),
    ];
    if let Some(only) = &only {
        fields.push(("Ids", only.as_str()));
    }
    let response = emby
        .as_admin(
            Method::Get,
            &format!("{CHANNELS}?{}", form_encoded(&fields)),
            None,
        )
        .await?;
    let page: ChannelsResource = emby.endpoint.decode(&response, UNREAD_CHANNELS)?;
    Ok(page
        .items
        .into_iter()
        .filter(|channel| Item::of(&channel.id).is_some())
        .map(ChannelResource::channel)
        .collect())
}

struct Window<'a> {
    channels: &'a [Channel],
    from: &'a str,
    until: &'a str,
}

async fn scheduled(
    emby: &Emby,
    window: &Window<'_>,
    member: Option<Account<'_>>,
) -> Result<Vec<ProgrammeResource>, Failure> {
    let ids = window
        .channels
        .iter()
        .map(|channel| channel.id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let member = member.map(|member| member.to_string());
    let mut fields = vec![
        ("ChannelIds", ids.as_str()),
        ("MinEndDate", window.from),
        ("MaxStartDate", window.until),
        ("Fields", "Tags"),
        ("EnableImages", "false"),
        ("EnableUserData", "false"),
    ];
    if let Some(member) = &member {
        fields.push(("UserId", member.as_str()));
    }
    let response = emby
        .as_admin(
            Method::Get,
            &format!("{PROGRAMMES}?{}", form_encoded(&fields)),
            None,
        )
        .await?;
    let page: ProgrammesResource = emby.endpoint.decode(&response, UNREAD_GUIDE)?;
    Ok(page.items)
}

fn stamped(at: Instant) -> Option<String> {
    at.calendar().map(|text| format!("{text}Z"))
}

fn certified(certificate: Option<String>) -> Option<String> {
    certificate.filter(|rating| !rating.is_empty())
}

fn aired(scheduled: Vec<ProgrammeResource>) -> HashMap<String, Vec<Airing>> {
    let mut aired: HashMap<String, Vec<Airing>> = HashMap::new();
    for programme in scheduled {
        let (Some(starts), Some(ends)) = (programme.starts, programme.ends) else {
            continue;
        };
        aired.entry(programme.channel).or_default().push(Airing {
            title: programme.title,
            certificate: certified(programme.certificate),
            starts,
            ends,
        });
    }
    for programmes in aired.values_mut() {
        programmes.sort_by_key(|airing| airing.starts);
    }
    aired
}

async fn opened(
    emby: &Emby,
    member: Account<'_>,
    channel: Item<'_>,
) -> Result<Option<Tuned>, Failure> {
    let asked = form_encoded(&[
        ("UserId", &member.to_string()),
        ("IsPlayback", "true"),
        ("AutoOpenLiveStream", "true"),
    ]);
    let response = emby
        .as_admin(
            Method::Post,
            &format!("/Items/{channel}/PlaybackInfo?{asked}"),
            Some(PROFILE.to_owned()),
        )
        .await?;
    let opened: OpenedResource = emby.endpoint.decode(&response, UNREAD_OPENED)?;
    match opened.error.as_deref() {
        Some(NOT_ALLOWED) => return Ok(None),
        Some(_) => return Err(emby.endpoint.refused(UNREAD_OPENED)),
        None => {}
    }
    let Some(source) = opened.sources.into_iter().next() else {
        return Err(emby.endpoint.refused(UNREAD_OPENED));
    };
    let closes_by = source
        .live_stream
        .as_deref()
        .filter(|_| a_play_session(&opened.play_session))
        .map(|live| format!("{live}.{}", opened.play_session));
    let Some(stream) = closes_by.as_deref().and_then(LiveStream::of) else {
        return Err(emby.endpoint.refused(UNREAD_OPENED));
    };
    let served = [&source.direct, &source.path]
        .into_iter()
        .flatten()
        .find_map(|address| buffered(address));
    let Some((file, container)) = served else {
        closed(emby, stream).await?;
        return Err(emby.endpoint.refused(UNSERVED));
    };
    Ok(Some(Tuned {
        closes_by: stream.to_string(),
        direct: format!("/LiveTv/LiveStreamFiles/{file}/stream.{container}"),
        transcoded: format!("/LiveTv/LiveStreamFiles/{file}/hls/master.m3u8"),
    }))
}

async fn closed(emby: &Emby, stream: LiveStream<'_>) -> Result<(), Failure> {
    let asked = form_encoded(&[
        ("LiveStreamId", stream.live()),
        ("PlaySessionId", stream.play_session()),
    ]);
    let response = emby
        .as_admin(Method::Post, &format!("{CLOSE}?{asked}"), None)
        .await?;
    emby.endpoint.expect_success(&response)
}

fn buffered(address: &str) -> Option<(String, String)> {
    let path = address.split(['?', '#']).next().unwrap_or_default();
    let lowered = path.to_ascii_lowercase();
    let at = lowered.find(FILES)?;
    let rest = path.get(at + FILES.len()..)?;
    let (file, named) = rest.split_once('/')?;
    let container = named
        .get(..STREAM.len())
        .filter(|word| word.eq_ignore_ascii_case(STREAM))
        .and_then(|_| named.get(STREAM.len()..))?;
    let plain = (1..=LONGEST_CONTAINER).contains(&container.len())
        && container
            .bytes()
            .all(|letter| letter.is_ascii_alphanumeric());
    (a_live_stream_file(file) && plain).then(|| (file.to_owned(), container.to_ascii_lowercase()))
}

impl ChannelResource {
    fn channel(self) -> Channel {
        Channel {
            id: self.id,
            name: self.name,
            number: self.number.filter(|number| !number.is_empty()),
            now: self
                .now
                .filter(|now| !now.title.is_empty())
                .map(|now| Programme {
                    title: now.title,
                    certificate: certified(now.certificate),
                    ends: now.ends,
                }),
        }
    }
}
