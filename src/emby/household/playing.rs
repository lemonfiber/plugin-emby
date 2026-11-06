use super::super::item::EPISODE;
use super::super::{item_type, Emby};
use super::SESSIONS;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Failure, Medium, Playback};

#[derive(serde::Deserialize)]
struct PlayingResource {
    #[serde(rename = "UserId", default)]
    user_id: String,
    #[serde(rename = "UserName", default)]
    user: String,
    #[serde(rename = "DeviceName", default)]
    device: String,
    #[serde(rename = "NowPlayingItem", default)]
    item: Option<NowPlaying>,
    #[serde(rename = "PlayState", default)]
    state: PlayState,
}

#[derive(serde::Deserialize)]
struct NowPlaying {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "SeriesName", default)]
    series: Option<String>,
    #[serde(rename = "ParentIndexNumber", default)]
    season: Option<u32>,
    #[serde(rename = "IndexNumber", default)]
    episode: Option<u32>,
    #[serde(rename = "Type", default)]
    medium: String,
}

#[derive(serde::Deserialize, Default)]
struct PlayState {
    #[serde(rename = "IsPaused", default)]
    paused: bool,
}

pub(super) async fn playing(emby: &Emby, member: Option<&str>) -> Result<Vec<Playback>, Failure> {
    let response = emby.as_admin(Method::Get, SESSIONS, None).await?;
    let listed: Vec<PlayingResource> = emby.endpoint.decode(
        &response,
        "what the media server is playing could not be read",
    )?;
    Ok(listed
        .into_iter()
        .filter(|session| member.is_none_or(|member| session.user_id == member))
        .filter_map(PlayingResource::playback)
        .collect())
}

fn medium(word: &str) -> Medium {
    if word == item_type(Kind::Movies) {
        Medium::Film
    } else if word == EPISODE || word == item_type(Kind::Tv) {
        Medium::Series
    } else {
        Medium::Other
    }
}

impl PlayingResource {
    fn playback(self) -> Option<Playback> {
        let item = self.item?;
        Some(Playback {
            member_id: self.user_id,
            member: self.user,
            title: item.name,
            series: item.series,
            season: item.season,
            episode: item.episode,
            medium: medium(&item.medium),
            paused: self.state.paused,
            device: self.device,
        })
    }
}
