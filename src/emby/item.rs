use super::item_type;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Holds, Item, Medium};

pub(super) const LISTED: &str = "ProductionYear";

#[derive(serde::Deserialize)]
pub(super) struct ItemResource {
    #[serde(rename = "Id", default)]
    pub(super) id: String,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "ProductionYear", default)]
    year: Option<u16>,
    #[serde(rename = "Type", default)]
    medium: String,
    #[serde(rename = "ImageTags", default)]
    images: Images,
    #[serde(rename = "BackdropImageTags", default)]
    backdrops: Vec<String>,
    #[serde(rename = "IsFolder", default)]
    folder: bool,
}

#[derive(serde::Deserialize, Default)]
struct Images {
    #[serde(rename = "Primary", default)]
    primary: Option<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct ItemsResource {
    #[serde(rename = "Items", default)]
    pub(super) items: Vec<ItemResource>,
}

impl ItemResource {
    pub(super) fn item(self) -> Item {
        Item {
            holds: self.holds(),
            id: self.id,
            title: self.name,
            year: self.year,
            medium: medium(&self.medium),
        }
    }

    pub(super) fn holds(&self) -> Holds {
        Holds {
            poster: self.images.primary.is_some(),
            backdrop: !self.backdrops.is_empty(),
            plays: !self.folder,
        }
    }

    pub(super) fn is_a_series(&self) -> bool {
        self.medium == item_type(Kind::Tv)
    }
}

pub(super) const EPISODE: &str = "Episode";

fn medium(word: &str) -> Medium {
    if word == item_type(Kind::Movies) {
        Medium::Film
    } else if word == item_type(Kind::Tv) {
        Medium::Series
    } else if word == EPISODE {
        Medium::Episode
    } else {
        Medium::Other
    }
}

#[cfg(test)]
mod tests;
