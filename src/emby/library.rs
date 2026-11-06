use async_trait::async_trait;
use serde::Deserialize;

use super::{item_type, Emby};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
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

#[async_trait]
impl Library for Emby {
    async fn has_item(&self, kind: Kind, term: &str) -> Result<bool, Failure> {
        let path = format!("/Items?Recursive=true&IncludeItemTypes={}", item_type(kind));
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
