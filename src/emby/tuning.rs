use async_trait::async_trait;

use super::Emby;
use crate::shape::{a_live_stream_file, a_play_session, Account, Item, LiveStream};
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Channel, Failure, Programme, Tuned, Tuning};

const LICENSED: &str = "/Plugins/SecurityInfo";

const CHANNELS: &str = "/LiveTv/Channels";

const CLOSE: &str = "/LiveStreams/Close";

const FILES: &str = "/livetv/livestreamfiles/";

const STREAM: &str = "stream.";

const LONGEST_CONTAINER: usize = 8;

const NOT_ALLOWED: &str = "NotAllowed";

const UNLICENSED: &str =
    "Emby runs live TV only under an Emby Premiere subscription, and this Emby did not say it holds one";

const UNREAD_ACCESS: &str = "whether the account may watch live TV could not be read";

const UNREAD_CHANNELS: &str = "the live TV channels could not be read";

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
struct ProgrammeResource {
    #[serde(rename = "Name", default)]
    title: String,
    #[serde(rename = "OfficialRating", default)]
    certificate: Option<String>,
    #[serde(rename = "EndDate", default)]
    ends: Option<String>,
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
    let response = emby
        .as_admin(Method::Get, &format!("/Users/{member}"), None)
        .await?;
    let holder: Holder = emby.endpoint.decode(&response, UNREAD_ACCESS)?;
    Ok(holder.policy.live_tv && !holder.policy.disabled)
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
                    certificate: now.certificate.filter(|rating| !rating.is_empty()),
                    ends: now.ends.filter(|ends| !ends.is_empty()),
                }),
        }
    }
}
