use async_trait::async_trait;
use serde::Deserialize;

use super::{item_type, Emby};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::{Kind, TitleKind};
use lemonfiber_ports::service::{Failure, Library};

#[derive(Deserialize)]
struct ItemPage {
    #[serde(rename = "Items", default)]
    items: Vec<LibraryItem>,
}

#[derive(Deserialize)]
struct LibraryItem {
    #[serde(rename = "Name", default)]
    name: String,
}

const fn held_types(kind: TitleKind) -> &'static str {
    match kind {
        TitleKind::Tv => item_type(Kind::Tv),
        TitleKind::Movies => item_type(Kind::Movies),
        TitleKind::Music => "MusicAlbum,MusicArtist",
        TitleKind::Books => "Book,AudioBook",
    }
}

#[async_trait]
impl Library for Emby {
    async fn has_item(&self, kind: TitleKind, term: &str) -> Result<bool, Failure> {
        let path = format!(
            "/Items?Recursive=true&IncludeItemTypes={}",
            held_types(kind)
        );
        let response = self.as_admin(Method::Get, &path, None).await?;
        let page: ItemPage = self
            .endpoint
            .decode(&response, "the library could not be read")?;

        let needle = term.to_lowercase();
        Ok(page
            .items
            .iter()
            .any(|item| item.name.to_lowercase().contains(&needle)))
    }

    async fn rescan(&self) -> Result<(), Failure> {
        let response = self
            .as_admin(Method::Post, "/Library/Refresh", None)
            .await?;
        self.endpoint.expect_success(&response)
    }
}
