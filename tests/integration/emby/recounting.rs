use std::sync::Arc;

use emby_adapter::emby::{Emby, Happenings};
use lemonfiber_adapter_kit::recounting::{Entry, History};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_fixtures::scratch::Scratch;
use lemonfiber_ports::service::{Event, Failure, Happened, Recounting as _};

use super::{emby, reader, SIGNED_IN};

const NEWEST_ITEM: &str =
    r#"{"Items":[{"Id":"103","Name":"Heat","DateCreated":"2026-11-11T12:00:00.5Z"}]}"#;

const NEWEST_ENTRY: &str = r#"{"Items":[{"Id":42,"Name":"Library scan failed","Date":"2026-11-11T11:30:00.0000000Z","Severity":"Error"}]}"#;

const ITEMS: &str = r#"{"Items":[
    {"Id":"103","Name":"Heat","DateCreated":"2026-11-11T12:00:00.5Z"},
    {"Id":"102","Name":"Pilot","SeriesName":"Severance","DateCreated":"2026-11-11T11:00:00.0000000Z"},
    {"Id":"100","Name":"Dune","DateCreated":"2026-11-11T10:00:00.0000000Z"}
]}"#;

const ENTRIES: &str = r#"{"Items":[
    {"Id":45,"Name":"Failed login attempt by nobody","Date":"2026-11-11T11:55:00.0000000Z","Type":"user.authenticationfailed","Severity":"Error"},
    {"Id":44,"Name":"probe failed to sign in","Date":"2026-11-11T11:50:00.0000000Z","UserId":"1","Severity":"Error"},
    {"Id":43,"Name":"Backup failed","Date":"2026-11-11T11:40:00.0000000Z","UserId":"","Type":"system.backupfailed","Severity":"Fatal"},
    {"Id":42,"Name":"Library scan failed","ShortOverview":"The disk is full","Date":"2026-11-11T11:30:00.0000000Z","Severity":"Warn"},
    {"Id":41,"Name":"Server restarted","Date":"2026-11-11T10:30:00.0000000Z","Severity":"Info"},
    {"Id":40,"Name":"Older","Date":"2026-11-11T09:30:00.0000000Z","Severity":"Error"}
]}"#;

fn heard(emby: &Arc<Emby>) -> Happenings {
    Happenings(Arc::downgrade(emby))
}

fn read_from(fake: &Arc<Fake>) -> Arc<Emby> {
    Arc::new(reader(fake))
}

fn history(items: &str, entries: &str) -> Arc<Fake> {
    Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        ("/Items?", Answer::reply(200, items)),
        ("/System/ActivityLog/Entries", Answer::reply(200, entries)),
    ])
}

#[tokio::test]
async fn the_newest_is_the_newest_item_s_added_time_and_id_and_the_newest_entry() {
    let fake = history(NEWEST_ITEM, NEWEST_ENTRY);
    let emby = read_from(&fake);
    assert_eq!(
        heard(&emby).newest().await.ok().as_deref(),
        Some("2026-11-11T12:00:00.5000000Z|103|42")
    );
    assert!(fake.requests().iter().any(|request| {
        request.url.contains("IncludeItemTypes=Movie,Episode")
            && request
                .url
                .contains("SortBy=DateCreated&SortOrder=Descending")
            && request.url.contains("StartIndex=0&Limit=1")
    }));
    assert!(fake.asked_for("/System/ActivityLog/Entries?StartIndex=0&Limit=1"));
}

#[tokio::test]
async fn an_empty_emby_s_newest_is_where_its_history_begins() {
    let fake = history(r#"{"Items":[]}"#, r#"{"Items":[]}"#);
    let emby = read_from(&fake);
    assert_eq!(heard(&emby).newest().await.ok().as_deref(), Some("||0"));
}

fn added(reference: &str, name: &str) -> Event {
    Event {
        event: Happened::Added,
        reference: Some(reference.to_owned()),
        name: name.to_owned(),
        reason: None,
        cleared: false,
    }
}

fn raised(name: &str, reason: Option<&str>) -> Event {
    Event {
        event: Happened::Health,
        reference: None,
        name: name.to_owned(),
        reason: reason.map(str::to_owned),
        cleared: false,
    }
}

#[tokio::test]
async fn what_followed_the_cursor_comes_oldest_first_each_with_where_it_stands() {
    let fake = history(ITEMS, ENTRIES);
    let emby = read_from(&fake);
    let after = heard(&emby)
        .after("2026-11-11T10:00:00.0000000Z|100|40")
        .await
        .ok()
        .flatten();
    let at = |added: &str, id: &str, entry: u64| format!("{added}|{id}|{entry}");
    let ten = "2026-11-11T10:00:00.0000000Z";
    let eleven = "2026-11-11T11:00:00.0000000Z";
    assert_eq!(
        after,
        Some(vec![
            Entry {
                position: at(ten, "100", 41),
                event: None,
            },
            Entry {
                position: at(eleven, "102", 41),
                event: Some(added("102", "Severance: Pilot")),
            },
            Entry {
                position: at(eleven, "102", 42),
                event: Some(raised("Library scan failed", Some("The disk is full"))),
            },
            Entry {
                position: at(eleven, "102", 43),
                event: Some(raised("Backup failed", None)),
            },
            Entry {
                position: at(eleven, "102", 44),
                event: None,
            },
            Entry {
                position: at(eleven, "102", 45),
                event: None,
            },
            Entry {
                position: at("2026-11-11T12:00:00.5000000Z", "103", 45),
                event: Some(added("103", "Heat")),
            },
        ])
    );
}

fn page_of(entries: impl Iterator<Item = String>) -> String {
    format!(r#"{{"Items":[{}]}}"#, entries.collect::<Vec<_>>().join(","))
}

#[tokio::test]
async fn a_full_page_is_followed_by_the_next_until_the_cursor_is_reached() {
    let at = "2026-11-11T09:00:00Z";
    let items = page_of((0..100).map(|n| format!(r#"{{"Id":"j{n}","DateCreated":"{at}"}}"#)));
    let entries = page_of((0..100).map(|n| {
        format!(
            r#"{{"Id":{},"Date":"2026-11-11T11:00:00Z","Severity":"Info"}}"#,
            300 - n
        )
    }));
    let fake = Fake::by_path(vec![
        ("/Users/AuthenticateByName", Answer::reply(200, SIGNED_IN)),
        (
            "DateCreated&StartIndex=100&Limit=100",
            Answer::reply(
                200,
                format!(
                    r#"{{"Items":[{{"Id":"k","DateCreated":"{at}"}},{{"Id":"old","DateCreated":"2026-11-10T00:00:00Z"}}]}}"#
                ),
            ),
        ),
        (
            "DateCreated&StartIndex=0&Limit=100",
            Answer::reply(200, items),
        ),
        (
            "/System/ActivityLog/Entries?StartIndex=100&Limit=100",
            Answer::reply(
                200,
                r#"{"Items":[{"Id":200,"Date":"2026-11-11T11:00:00Z"},{"Id":199}]}"#,
            ),
        ),
        (
            "/System/ActivityLog/Entries?StartIndex=0&Limit=100",
            Answer::reply(200, entries),
        ),
        (
            "/System/ActivityLog/Entries?StartIndex=0&Limit=1",
            Answer::reply(200, r#"{"Items":[{"Id":300}]}"#),
        ),
    ]);
    let emby = read_from(&fake);
    let after = heard(&emby)
        .after("2026-11-10T00:00:00.0000000Z|old|199")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let added: Vec<Option<String>> = after
        .iter()
        .filter_map(|entry| entry.event.as_ref())
        .map(|event| event.reference.clone())
        .collect();
    let wanted: Vec<Option<String>> = std::iter::once("k".to_owned())
        .chain((0..100).rev().map(|n| format!("j{n}")))
        .map(Some)
        .collect();
    assert_eq!(added, wanted);
    let positions: Vec<&str> = after
        .iter()
        .skip(added.len())
        .map(|entry| entry.position.as_str())
        .collect();
    let told: Vec<String> = (200..=300)
        .map(|id| format!("2026-11-11T09:00:00.0000000Z|j0|{id}"))
        .collect();
    assert_eq!(positions, told);
}

#[tokio::test]
async fn a_cursor_emby_no_longer_holds_or_cannot_be_read_is_not_held() {
    let fake = history(ITEMS, ENTRIES);
    let emby = read_from(&fake);
    let heard = heard(&emby);
    assert_eq!(heard.after("||50").await.ok(), Some(None));
    assert_eq!(heard.after("not a cursor").await.ok(), Some(None));
    assert_eq!(heard.after("a|b|c").await.ok(), Some(None));
    assert_eq!(heard.after("a|b|1|2").await.ok(), Some(None));
}

#[tokio::test]
async fn an_emby_the_adapter_no_longer_holds_is_not_answering() {
    let fake = Fake::silent();
    let heard = heard(&read_from(&fake));
    assert!(matches!(
        heard.newest().await,
        Err(Failure::Unavailable { service }) if service == "emby"
    ));
    assert!(matches!(
        heard.after("||0").await,
        Err(Failure::Unavailable { .. })
    ));
}

#[tokio::test]
async fn asked_with_nowhere_to_keep_a_cursor_it_refuses_and_asks_emby_nothing() {
    let fake = Fake::silent();
    let said = emby(&fake).events(None, None, None, None).await;
    assert!(
        matches!(&said, Err(Failure::Refused { service, .. }) if service == "emby"),
        "{said:?}"
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn the_recounting_kit_answers_events_through_its_recounter() {
    let dir = Scratch::new("emby-recounting-kit");
    let fake = Fake::silent();
    let emby = read_from(&fake);
    let _kit = emby_adapter::adapter::recounting_kit(
        "key".to_owned(),
        Arc::clone(&emby),
        dir.path(),
        Arc::new(lemonfiber_ports::narration::Silent),
    );
    assert_eq!(emby.events(None, None, None, None).await.ok(), Some(None));
    let partial = emby
        .events(
            Some("https://core.invalid/api/notify/events"),
            None,
            None,
            None,
        )
        .await;
    assert!(
        matches!(&partial, Err(Failure::Disallowed { service, .. }) if service == "emby"),
        "{partial:?}"
    );
}
