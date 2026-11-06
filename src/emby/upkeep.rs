use async_trait::async_trait;

use super::item::EPISODE;
use super::{item_type, Emby};
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Failure, SeriesHeld, Upkeep};

const COUNTED: &str = "ChildCount,RecursiveItemCount";

const REFRESHED: [(&str, &str); 5] = [
    ("Recursive", "true"),
    ("MetadataRefreshMode", "FullRefresh"),
    ("ImageRefreshMode", "Default"),
    ("ReplaceAllMetadata", "false"),
    ("ReplaceAllImages", "false"),
];

#[derive(serde::Deserialize)]
struct SeriesPage {
    #[serde(rename = "Items", default)]
    items: Vec<Series>,
}

#[derive(serde::Deserialize)]
struct Series {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "ChildCount", default)]
    seasons: u32,
    #[serde(rename = "RecursiveItemCount", default)]
    episodes: Option<u32>,
}

#[derive(serde::Deserialize)]
struct Counted {
    #[serde(rename = "TotalRecordCount")]
    total: u32,
}

#[async_trait]
impl Upkeep for Emby {
    async fn series_held(&self, most: u32) -> Result<Vec<SeriesHeld>, Failure> {
        held(self, most).await
    }

    async fn refresh(&self, id: &str) -> Result<(), Failure> {
        refreshed(self, id).await
    }
}

async fn held(emby: &Emby, most: u32) -> Result<Vec<SeriesHeld>, Failure> {
    let asked = form_encoded(&[
        ("Recursive", "true"),
        ("IncludeItemTypes", item_type(Kind::Tv)),
        ("Fields", COUNTED),
        ("SortBy", "SortName"),
        ("Limit", &most.to_string()),
    ]);
    let response = emby
        .as_admin_naming(
            Method::Get,
            |admin| format!("/Users/{admin}/Items?{asked}"),
            None,
        )
        .await?;
    let page: SeriesPage = emby
        .endpoint
        .decode(&response, "the series the library holds could not be read")?;
    let mut held = Vec::with_capacity(page.items.len());
    for series in page.items {
        let episodes = match series.seasons {
            0 => filed_under(emby, &series.id).await?,
            _ => series.episodes.unwrap_or_default(),
        };
        held.push(SeriesHeld {
            id: series.id,
            title: series.name,
            seasons: series.seasons,
            episodes,
        });
    }
    Ok(held)
}

async fn filed_under(emby: &Emby, id: &str) -> Result<u32, Failure> {
    let asked = form_encoded(&[
        ("ParentId", id),
        ("Recursive", "true"),
        ("IncludeItemTypes", EPISODE),
        ("IsMissing", "false"),
        ("Limit", "0"),
    ]);
    let response = emby
        .as_admin(Method::Get, &format!("/Items?{asked}"), None)
        .await?;
    let counted: Counted = emby.endpoint.decode(
        &response,
        "the episodes a series holds could not be counted",
    )?;
    Ok(counted.total)
}

async fn refreshed(emby: &Emby, id: &str) -> Result<(), Failure> {
    if !crate::shape::an_item(id) {
        return Err(emby
            .endpoint
            .refused("that names nothing the media server holds"));
    }
    let asked = form_encoded(&REFRESHED);
    let response = emby
        .as_admin(Method::Post, &format!("/Items/{id}/Refresh?{asked}"), None)
        .await?;
    emby.endpoint.expect_success(&response)
}
