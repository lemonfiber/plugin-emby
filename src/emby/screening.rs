use async_trait::async_trait;

use super::item::ItemResource;
use super::Emby;
use crate::shape::{a_device, Account, Item as ItemId};
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_adapter_kit::picture;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    EpisodeDetail, Failure, HowFar, Image, Item, ItemDetail, ItemProgress, Picture, Playback,
    Screening, SeasonDetail, PICTURE_MOST,
};

const TICKS: u64 = 10_000_000;

pub(super) const ABSENT: u16 = 404;

const DETAILED: &str = "Overview,Genres,OfficialRating,PremiereDate,ProductionYear";

#[derive(serde::Deserialize)]
struct DetailResource {
    #[serde(flatten)]
    item: ItemResource,
    #[serde(rename = "Overview", default)]
    overview: Option<String>,
    #[serde(rename = "RunTimeTicks", default)]
    ticks: Option<u64>,
    #[serde(rename = "Genres", default)]
    genres: Vec<String>,
    #[serde(rename = "OfficialRating", default)]
    certificate: Option<String>,
    #[serde(rename = "PremiereDate", default)]
    premiered: Option<String>,
    #[serde(rename = "IndexNumber", default)]
    number: Option<u32>,
    #[serde(rename = "SeasonId", default)]
    season: Option<String>,
    #[serde(rename = "UserData", default)]
    watched: Option<UserData>,
}

#[derive(serde::Deserialize)]
struct UserData {
    #[serde(rename = "PlaybackPositionTicks", default)]
    position: u64,
}

#[derive(serde::Deserialize)]
struct DetailsResource {
    #[serde(rename = "Items", default)]
    items: Vec<DetailResource>,
}

#[derive(serde::Serialize)]
struct UserDataUpdate {
    #[serde(rename = "PlaybackPositionTicks")]
    position: u64,
    #[serde(rename = "Played")]
    played: bool,
}

#[async_trait]
impl Screening for Emby {
    async fn holdings(&self, member: Option<&str>, most: u32) -> Result<Vec<Item>, Failure> {
        super::household::holdings(self, member, most).await
    }

    async fn playing(&self, member: Option<&str>) -> Result<Vec<Playback>, Failure> {
        super::household::now_playing(self, member).await
    }

    async fn signed_in(&self, member: &str, device: &str) -> Result<Option<String>, Failure> {
        account(self, Some(member))?;
        if !a_device(device) {
            return Err(self.misshapen("a device"));
        }
        Ok(None)
    }

    async fn title(&self, member: Option<&str>, id: &str) -> Result<Option<ItemDetail>, Failure> {
        let member = account(self, member)?;
        match ItemId::of(id) {
            Some(id) => titled(self, member, id).await,
            None => Ok(None),
        }
    }

    async fn part_way(&self, member: &str, most: u32) -> Result<Vec<ItemProgress>, Failure> {
        let Some(member) = Account::of(member) else {
            return Err(self.misshapen("an account"));
        };
        let asked = form_encoded(&[
            ("Limit", &most.to_string()),
            ("MediaTypes", "Video"),
            ("Fields", super::item::LISTED),
        ]);
        let response = self
            .as_admin(
                Method::Get,
                &format!("/Users/{member}/Items/Resume?{asked}"),
                None,
            )
            .await?;
        let page: DetailsResource = self
            .endpoint
            .decode(&response, "what was part-way through could not be read")?;
        Ok(page
            .items
            .into_iter()
            .map(DetailResource::part_way)
            .collect())
    }

    async fn progressed(&self, member: &str, id: &str, how_far: &HowFar) -> Result<(), Failure> {
        progress(self, member, id, *how_far).await
    }

    async fn picture(
        &self,
        member: Option<&str>,
        id: &str,
        which: Picture,
    ) -> Result<Option<Image>, Failure> {
        pictured(self, member, id, which).await
    }

    async fn sign_out(&self, device: &str) -> Result<(), Failure> {
        if !a_device(device) {
            return Err(self.misshapen("a device"));
        }
        let asked = form_encoded(&[("Id", device)]);
        let response = self
            .as_admin(Method::Delete, &format!("/Devices?{asked}"), None)
            .await?;
        self.endpoint.expect_success(&response)
    }
}

pub(super) fn picture_path(item: impl std::fmt::Display, which: Picture) -> String {
    let named = match which {
        Picture::Poster => "Primary",
        Picture::Backdrop => "Backdrop",
    };
    format!("/Items/{item}/Images/{named}")
}

async fn pictured(
    emby: &Emby,
    member: Option<&str>,
    id: &str,
    which: Picture,
) -> Result<Option<Image>, Failure> {
    let member = account(emby, member)?;
    let Some(id) = ItemId::of(id) else {
        return Ok(None);
    };
    let Some(seen) = seen(emby, member, id).await? else {
        return Ok(None);
    };
    let holds = seen.item.holds();
    let held = match which {
        Picture::Poster => holds.poster,
        Picture::Backdrop => holds.backdrop,
    };
    if !held {
        return Ok(None);
    }
    let request = emby.request(Method::Get, &picture_path(id, which), None);
    let fetched = emby.fetch(&request, PICTURE_MOST).await?;
    if fetched.status == ABSENT {
        return Ok(None);
    }
    if !fetched.is_success() {
        return Err(emby
            .endpoint
            .refused("the media server would not answer the picture"));
    }
    let served = fetched.header("content-type").map(str::to_owned);
    Ok(fetched
        .bytes
        .and_then(|bytes| picture::image(served.as_deref(), bytes)))
}

fn account<'a>(emby: &Emby, member: Option<&'a str>) -> Result<Option<Account<'a>>, Failure> {
    member
        .map(|member| Account::of(member).ok_or_else(|| emby.misshapen("an account")))
        .transpose()
}

fn whose(member: Option<Account<'_>>) -> Vec<(&'static str, String)> {
    member
        .map(|member| vec![("UserId", member.to_string())])
        .unwrap_or_default()
}

fn query(fields: &[(&str, String)]) -> String {
    let borrowed: Vec<(&str, &str)> = fields
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    form_encoded(&borrowed)
}

async fn seen(
    emby: &Emby,
    member: Option<Account<'_>>,
    id: ItemId<'_>,
) -> Result<Option<DetailResource>, Failure> {
    let mut fields = vec![("Ids", id.to_string()), ("Fields", DETAILED.to_owned())];
    fields.extend(whose(member));
    let response = emby
        .as_admin(Method::Get, &format!("/Items?{}", query(&fields)), None)
        .await?;
    let page: DetailsResource = emby
        .endpoint
        .decode(&response, "what the title is could not be read")?;
    let wanted = id.to_string();
    Ok(page.items.into_iter().find(|item| item.item.id == wanted))
}

async fn titled(
    emby: &Emby,
    member: Option<Account<'_>>,
    id: ItemId<'_>,
) -> Result<Option<ItemDetail>, Failure> {
    let Some(detail) = seen(emby, member, id).await? else {
        return Ok(None);
    };
    let seasons = if detail.item.is_a_series() {
        seasons(emby, member, id).await?
    } else {
        Vec::new()
    };
    Ok(Some(detail.titled(seasons)))
}

async fn progress(emby: &Emby, member: &str, id: &str, how_far: HowFar) -> Result<(), Failure> {
    let Some(member) = Account::of(member) else {
        return Err(emby.misshapen("an account"));
    };
    let Some(id) = ItemId::of(id) else {
        return Err(emby.misshapen("an item"));
    };
    let body = serde_json::to_string(&UserDataUpdate {
        position: if how_far.ended {
            0
        } else {
            how_far.position.saturating_mul(TICKS)
        },
        played: how_far.ended,
    })
    .unwrap_or_default();
    let response = emby
        .as_admin(
            Method::Post,
            &format!("/Users/{member}/Items/{id}/UserData"),
            Some(body),
        )
        .await?;
    emby.endpoint.expect_success(&response)
}

async fn seasons(
    emby: &Emby,
    member: Option<Account<'_>>,
    id: ItemId<'_>,
) -> Result<Vec<SeasonDetail>, Failure> {
    let held: DetailsResource = listed(emby, &format!("/Shows/{id}/Seasons"), whose(member))
        .await
        .and_then(|response| {
            emby.endpoint
                .decode(&response, "the series' seasons could not be read")
        })?;
    let mut fields = whose(member);
    fields.push(("Fields", "Overview".to_owned()));
    let episodes: DetailsResource = listed(emby, &format!("/Shows/{id}/Episodes"), fields)
        .await
        .and_then(|response| {
            emby.endpoint
                .decode(&response, "the series' episodes could not be read")
        })?;
    Ok(grouped(held.items, episodes.items))
}

async fn listed(
    emby: &Emby,
    path: &str,
    fields: Vec<(&'static str, String)>,
) -> Result<lemonfiber_ports::http::Response, Failure> {
    let path = if fields.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{}", query(&fields))
    };
    emby.as_admin(Method::Get, &path, None).await
}

fn grouped(seasons: Vec<DetailResource>, episodes: Vec<DetailResource>) -> Vec<SeasonDetail> {
    let mut episodes: Vec<(Option<String>, EpisodeDetail)> = episodes
        .into_iter()
        .map(|episode| (episode.season.clone(), episode.episode()))
        .collect();
    seasons
        .into_iter()
        .map(|season| {
            let id = season.item.id.clone();
            let (within, rest): (Vec<_>, Vec<_>) = episodes
                .drain(..)
                .partition(|(of, _)| of.as_deref() == Some(id.as_str()));
            episodes = rest;
            SeasonDetail {
                number: season.number,
                name: season.item.item().title,
                id,
                episodes: within.into_iter().map(|(_, episode)| episode).collect(),
            }
        })
        .collect()
}

fn minutes(ticks: Option<u64>) -> Option<u32> {
    ticks.and_then(|ticks| u32::try_from(ticks / TICKS / 60).ok())
}

impl DetailResource {
    fn titled(self, seasons: Vec<SeasonDetail>) -> ItemDetail {
        ItemDetail {
            overview: self.overview,
            minutes: minutes(self.ticks),
            genres: self.genres,
            certificate: self.certificate,
            released: self
                .premiered
                .map(|premiered| premiered.chars().take(10).collect()),
            seasons,
            item: self.item.item(),
        }
    }

    fn episode(self) -> EpisodeDetail {
        EpisodeDetail {
            number: self.number,
            overview: self.overview,
            minutes: minutes(self.ticks),
            item: self.item.item(),
        }
    }

    fn part_way(self) -> ItemProgress {
        ItemProgress {
            position: self.watched.map_or(0, |watched| watched.position / TICKS),
            length: self.ticks.map(|ticks| ticks / TICKS),
            item: self.item.item(),
        }
    }
}
