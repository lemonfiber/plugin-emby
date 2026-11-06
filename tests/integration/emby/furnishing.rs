use super::{reader, sent, SIGNED_IN};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, FiledAt, Furnished, Furnishing as _};

const MOVIES: &str = "/data/media/movies";

const TV: &str = "/data/media/tv";

const READING_TV: &str =
    r#"[{"Name":"TV Shows","Locations":["/data/media/tv/"],"CollectionType":"tvshows"}]"#;

fn filed(media_type: &str, folder: &str) -> FiledAt {
    FiledAt {
        media_type: media_type.to_owned(),
        folder: folder.to_owned(),
    }
}

fn furnished(media_type: &str, held: bool) -> Furnished {
    Furnished {
        media_type: media_type.to_owned(),
        held,
    }
}

fn holding(libraries: &str, folder_there: u16, added: u16) -> std::sync::Arc<Fake> {
    Fake::by_route(vec![
        (
            Method::Post,
            "/Users/AuthenticateByName",
            Answer::reply(200, SIGNED_IN),
        ),
        (
            Method::Get,
            "/Library/VirtualFolders",
            Answer::reply(200, libraries),
        ),
        (
            Method::Post,
            "/Environment/ValidatePath",
            Answer::reply(folder_there, ""),
        ),
        (
            Method::Post,
            "/Library/VirtualFolders",
            Answer::reply(added, ""),
        ),
    ])
}

fn written(fake: &Fake) -> Vec<(Method, String)> {
    fake.requests()
        .into_iter()
        .filter(|asked| {
            asked.method != Method::Get && !asked.url.contains("/Users/AuthenticateByName")
        })
        .map(|asked| (asked.method, asked.url))
        .collect()
}

#[tokio::test]
async fn a_library_is_added_for_a_folder_no_library_reads_and_one_that_does_is_left_alone() {
    let fake = holding(READING_TV, 204, 204);
    assert_eq!(
        reader(&fake)
            .furnish(&[filed("movies", MOVIES), filed("tv", TV)])
            .await
            .ok(),
        Some(vec![furnished("movies", true), furnished("tv", true)])
    );
    assert_eq!(
        written(&fake),
        vec![
            (
                Method::Post,
                "http://127.0.0.1:8096/Environment/ValidatePath".to_owned()
            ),
            (
                Method::Post,
                "http://127.0.0.1:8096/Library/VirtualFolders".to_owned()
            ),
        ]
    );
    assert_eq!(
        sent(&fake, Method::Post, "/Environment/ValidatePath"),
        serde_json::json!({ "Path": MOVIES, "IsFile": false })
    );
    assert_eq!(
        sent(&fake, Method::Post, "/Library/VirtualFolders"),
        serde_json::json!({
            "Name": "Movies",
            "CollectionType": "movies",
            "Paths": [MOVIES],
            "RefreshLibrary": true,
            "LibraryOptions": {}
        })
    );
}

#[tokio::test]
async fn a_folder_that_is_not_there_is_not_held_and_nothing_is_added() {
    let fake = holding("[]", 404, 204);
    assert_eq!(
        reader(&fake).furnish(&[filed("tv", TV)]).await.ok(),
        Some(vec![furnished("tv", false)])
    );
    assert!(!written(&fake)
        .iter()
        .any(|(_, url)| url.contains("/Library/VirtualFolders")));
}

#[tokio::test]
async fn a_type_emby_does_not_take_or_a_name_already_taken_is_not_held_and_nothing_changes() {
    let fake = holding(
        r#"[{"Name":"movies","Locations":["/elsewhere"],"CollectionType":"movies"}]"#,
        204,
        204,
    );
    assert_eq!(
        reader(&fake)
            .furnish(&[
                filed("podcasts", "/data/media/podcasts"),
                filed("movies", MOVIES)
            ])
            .await
            .ok(),
        Some(vec![
            furnished("podcasts", false),
            furnished("movies", false)
        ])
    );
    assert!(written(&fake).is_empty());
}

#[tokio::test]
async fn libraries_that_cannot_be_read_a_folder_check_refused_or_an_add_refused_are_failures() {
    let unread = holding("not json", 204, 204);
    assert!(reader(&unread)
        .furnish(&[filed("movies", MOVIES)])
        .await
        .is_err());
    let unchecked = holding("[]", 500, 204);
    assert!(reader(&unchecked)
        .furnish(&[filed("movies", MOVIES)])
        .await
        .is_err());
    let refusing = holding("[]", 204, 500);
    assert!(matches!(
        reader(&refusing).furnish(&[filed("movies", MOVIES)]).await,
        Err(Failure::Refused { .. })
    ));
}
