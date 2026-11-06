use async_trait::async_trait;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, FiledAt, Furnished, Furnishing};
use serde::Deserialize;

use super::Emby;

const LIBRARIES: &str = "/Library/VirtualFolders";

const VALIDATE_PATH: &str = "/Environment/ValidatePath";

const NOT_THERE: u16 = 404;

struct Kind {
    media_type: &'static str,
    name: &'static str,
    collection: &'static str,
}

const TAKEN: [Kind; 2] = [
    Kind {
        media_type: "movies",
        name: "Movies",
        collection: "movies",
    },
    Kind {
        media_type: "tv",
        name: "TV Shows",
        collection: "tvshows",
    },
];

#[derive(Deserialize)]
struct Library {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "Locations", default)]
    locations: Vec<String>,
}

fn reads(libraries: &[Library], folder: &str) -> bool {
    libraries
        .iter()
        .flat_map(|library| &library.locations)
        .any(|location| location.trim_end_matches('/') == folder.trim_end_matches('/'))
}

fn named(libraries: &[Library], name: &str) -> bool {
    libraries
        .iter()
        .any(|library| library.name.eq_ignore_ascii_case(name))
}

impl Emby {
    async fn libraries(&self) -> Result<Vec<Library>, Failure> {
        let response = self.as_admin(Method::Get, LIBRARIES, None).await?;
        self.endpoint
            .decode(&response, "the libraries Emby holds could not be read")
    }

    async fn exists(&self, folder: &str) -> Result<bool, Failure> {
        let body = serde_json::json!({ "Path": folder, "IsFile": false });
        let response = self
            .as_admin(Method::Post, VALIDATE_PATH, Some(body.to_string()))
            .await?;
        if response.status == NOT_THERE {
            return Ok(false);
        }
        self.endpoint.expect_success(&response).map(|()| true)
    }

    async fn added(&self, kind: &Kind, folder: &str) -> Result<(), Failure> {
        let body = serde_json::json!({
            "Name": kind.name,
            "CollectionType": kind.collection,
            "Paths": [folder],
            "RefreshLibrary": true,
            "LibraryOptions": {},
        });
        let response = self
            .as_admin(Method::Post, LIBRARIES, Some(body.to_string()))
            .await?;
        self.endpoint.expect_success(&response)
    }
}

#[async_trait]
impl Furnishing for Emby {
    async fn furnish(&self, filed: &[FiledAt]) -> Result<Vec<Furnished>, Failure> {
        let mut libraries = self.libraries().await?;
        let mut furnished = Vec::with_capacity(filed.len());
        for one in filed {
            let held = match TAKEN.iter().find(|kind| kind.media_type == one.media_type) {
                None => false,
                Some(_) if reads(&libraries, &one.folder) => true,
                Some(kind) if named(&libraries, kind.name) => false,
                Some(kind) => {
                    let there = self.exists(&one.folder).await?;
                    if there {
                        self.added(kind, &one.folder).await?;
                        libraries.push(Library {
                            name: kind.name.to_owned(),
                            locations: vec![one.folder.clone()],
                        });
                    }
                    there
                }
            };
            furnished.push(Furnished {
                media_type: one.media_type.clone(),
                held,
            });
        }
        Ok(furnished)
    }
}
