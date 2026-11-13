//! Emby telling the core what happened: a title added to a library, read from its items
//! newest first, and a problem it raised about itself, read from its activity log, every
//! interval the core asks.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use lemonfiber_adapter_kit::recounting::{Entry, History, Recounter};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Event, Failure, Happened, Heard, Recounting};
use serde::Deserialize;

use super::item::EPISODE;
use super::{item_type, Emby};

/// How many items or entries are read at a time.
const PAGE: usize = 100;

/// The severities an activity entry is told at.
const RAISED: [&str; 3] = ["Warn", "Error", "Fatal"];

/// The beginnings of the activity types Emby files a member's own doings under, as its
/// notification types name them: an account's, an item's a member marked, and playback.
const ABOUT_MEMBERS: [&str; 3] = ["user.", "item.", "playback."];

/// Said where `events` is asked of an Emby no recounter was handed.
const UNRECOUNTED: &str = "this adapter was started with nowhere to keep its cursor";

impl Emby {
    /// Tell the core what happened through `recounter` from now on, where nothing was
    /// handed before.
    pub fn recounted_by(&self, recounter: Recounter) {
        let _ = self.recounter.set(recounter);
    }
}

#[async_trait]
impl Recounting for Emby {
    async fn events(
        &self,
        address: Option<&str>,
        pin: Option<&str>,
        key: Option<&str>,
        every: Option<u32>,
    ) -> Result<Option<Heard>, Failure> {
        let recounter = self
            .recounter
            .get()
            .ok_or_else(|| self.endpoint.refused(UNRECOUNTED))?;
        recounter.events(address, pin, key, every).await
    }
}

/// Emby's items and activity log as one history, holding Emby without keeping it alive.
pub struct Happenings(pub Weak<Emby>);

impl Happenings {
    /// Emby, while the adapter holds it.
    fn emby(&self) -> Result<Arc<Emby>, Failure> {
        self.0.upgrade().ok_or_else(|| Failure::Unavailable {
            service: super::SERVICE.to_owned(),
        })
    }
}

/// The cursor: the newest item told by its added time and id, and the newest activity
/// entry told by its id.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cursor {
    added: Added,
    entry: u64,
}

/// An item's added time, written so two compare as text in the order they happened, and
/// its id.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Added {
    at: String,
    id: String,
}

impl Cursor {
    fn written(&self) -> String {
        format!("{}|{}|{}", self.added.at, self.added.id, self.entry)
    }

    fn read(cursor: &str) -> Option<Self> {
        let mut parts = cursor.split('|');
        let (at, id, entry) = (parts.next()?, parts.next()?, parts.next()?);
        parts.next().is_none().then_some(())?;
        Some(Self {
            added: Added {
                at: at.to_owned(),
                id: id.to_owned(),
            },
            entry: entry.parse().ok()?,
        })
    }
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(rename = "Items", default = "Vec::new")]
    items: Vec<T>,
}

#[derive(Deserialize)]
struct Item {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "SeriesName", default)]
    series: Option<String>,
    #[serde(rename = "DateCreated", default)]
    created: String,
}

impl Item {
    fn added(&self) -> Added {
        Added {
            at: comparable(&self.created),
            id: self.id.clone(),
        }
    }

    fn event(&self) -> Event {
        let name = match &self.series {
            Some(series) if !series.is_empty() => format!("{series}: {}", self.name),
            _ => self.name.clone(),
        };
        Event {
            event: Happened::Added,
            reference: Some(self.id.clone()),
            name,
            reason: None,
            cleared: false,
        }
    }
}

#[derive(Deserialize)]
struct Activity {
    #[serde(rename = "Id")]
    id: u64,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "ShortOverview", default)]
    overview: Option<String>,
    #[serde(rename = "Date", default)]
    date: String,
    #[serde(rename = "Severity", default)]
    severity: String,
    #[serde(rename = "UserId", default)]
    user: Option<String>,
    #[serde(rename = "Type", default)]
    kind: String,
}

impl Activity {
    fn event(&self) -> Option<Event> {
        let raised = RAISED.contains(&self.severity.as_str());
        let unnamed = self.user.as_deref().is_none_or(str::is_empty)
            && !ABOUT_MEMBERS
                .iter()
                .any(|about| self.kind.starts_with(about));
        (raised && unnamed).then(|| Event {
            event: Happened::Health,
            reference: None,
            name: self.name.clone(),
            reason: self
                .overview
                .clone()
                .filter(|overview| !overview.is_empty()),
            cleared: false,
        })
    }
}

/// An instant as Emby writes it, its fraction of a second written to seven digits, so
/// two compare as text in the order they happened.
fn comparable(instant: &str) -> String {
    let instant = instant.trim_end_matches('Z');
    let (whole, fraction) = instant.split_once('.').unwrap_or((instant, ""));
    format!("{whole}.{fraction:0<7}Z")
}

impl Emby {
    async fn items_page(&self, start: usize, most: usize) -> Result<Vec<Item>, Failure> {
        let path = format!(
            "/Items?Recursive=true&IncludeItemTypes={},{EPISODE}&SortBy=DateCreated&SortOrder=Descending&Fields=DateCreated&StartIndex={start}&Limit={most}",
            item_type(Kind::Movies)
        );
        self.read_page(&path, "a list of items").await
    }

    async fn entries_page(&self, start: usize, most: usize) -> Result<Vec<Activity>, Failure> {
        let path = format!("/System/ActivityLog/Entries?StartIndex={start}&Limit={most}");
        self.read_page(&path, "its activity log").await
    }

    async fn read_page<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<Vec<T>, Failure> {
        let response = self.as_admin(Method::Get, path, None).await?;
        self.endpoint
            .decode::<Page<T>>(&response, what)
            .map(|page| page.items)
    }

    async fn items_after(&self, after: &Added) -> Result<Vec<Item>, Failure> {
        let mut newer = Vec::new();
        loop {
            let page = self.items_page(newer.len(), PAGE).await?;
            let read = page.len();
            let before = newer.len();
            newer.extend(page.into_iter().take_while(|item| item.added() > *after));
            if read < PAGE || newer.len() - before < read {
                break;
            }
        }
        newer.reverse();
        Ok(newer)
    }

    async fn entries_after(&self, after: u64) -> Result<Vec<Activity>, Failure> {
        let mut newer = Vec::new();
        loop {
            let page = self.entries_page(newer.len(), PAGE).await?;
            let read = page.len();
            let before = newer.len();
            newer.extend(page.into_iter().take_while(|entry| entry.id > after));
            if read < PAGE || newer.len() - before < read {
                break;
            }
        }
        newer.reverse();
        Ok(newer)
    }

    async fn newest_entry(&self) -> Result<u64, Failure> {
        Ok(self
            .entries_page(0, 1)
            .await?
            .first()
            .map_or(0, |entry| entry.id))
    }
}

#[async_trait]
impl History for Happenings {
    async fn newest(&self) -> Result<String, Failure> {
        let emby = self.emby()?;
        let added = emby
            .items_page(0, 1)
            .await?
            .first()
            .map(Item::added)
            .unwrap_or_default();
        let entry = emby.newest_entry().await?;
        Ok(Cursor { added, entry }.written())
    }

    async fn after(&self, cursor: &str) -> Result<Option<Vec<Entry>>, Failure> {
        let Some(mut cursor) = Cursor::read(cursor) else {
            return Ok(None);
        };
        let emby = self.emby()?;
        if emby.newest_entry().await? < cursor.entry {
            return Ok(None);
        }
        let items = emby.items_after(&cursor.added).await?;
        let entries = emby.entries_after(cursor.entry).await?;
        let mut happened: Vec<(String, Happening)> = items
            .into_iter()
            .map(|item| (item.added().at, Happening::Added(item)))
            .chain(
                entries
                    .into_iter()
                    .map(|entry| (comparable(&entry.date), Happening::Raised(entry))),
            )
            .collect();
        happened.sort_by(|one, other| one.0.cmp(&other.0));
        Ok(Some(
            happened
                .into_iter()
                .map(|(_, happening)| {
                    let event = match happening {
                        Happening::Added(item) => {
                            cursor.added = item.added();
                            Some(item.event())
                        }
                        Happening::Raised(entry) => {
                            cursor.entry = entry.id;
                            entry.event()
                        }
                    };
                    Entry {
                        position: cursor.written(),
                        event,
                    }
                })
                .collect(),
        ))
    }
}

/// One happening read from Emby, before it is told.
enum Happening {
    Added(Item),
    Raised(Activity),
}
