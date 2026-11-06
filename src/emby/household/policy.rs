use super::super::{carried, rating, Emby, Opened};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Allowed, Failure, Unrated};

const EVERY_LIBRARY: &str = "EnableAllFolders";

const CHOSEN_LIBRARIES: &str = "EnabledFolders";

const AGE_LIMIT: &str = "MaxParentalRating";

const UNRATED_KINDS: [&str; 10] = [
    "Movie",
    "Trailer",
    "Series",
    "Music",
    "Game",
    "Book",
    "LiveTvChannel",
    "LiveTvProgram",
    "ChannelContent",
    "Other",
];

const UNRATED: &str = "BlockUnratedItems";

const DISABLED: &str = "IsDisabled";

const HIDDEN: &str = "IsHidden";

const WRONG_SO_FAR: &str = "InvalidLoginAttemptCount";

const LOCKED_OUT_AT: &str = "LockedOutDate";

const POLICY: &str = "Policy";

pub(in super::super) enum Edit<'a> {
    Allow(&'a Allowed),
    Claimable(&'a Allowed),
    Suspend,
    Hide,
}

pub(in super::super) async fn rewritten(
    emby: &Emby,
    id: &str,
    edit: &Edit<'_>,
) -> Result<(), Failure> {
    if !crate::shape::an_account(id) {
        return Err(emby.misshapen("an account"));
    }
    let response = emby
        .as_admin(Method::Get, &format!("/Users/{id}"), None)
        .await?;
    let policy = edited(emby, &response, edit)?;
    let response = emby
        .as_admin(Method::Post, &format!("/Users/{id}/Policy"), Some(policy))
        .await?;
    emby.endpoint.expect_success(&response)
}

pub(in super::super) async fn rewritten_as(
    emby: &Emby,
    session: &Opened,
    edit: &Edit<'_>,
) -> Result<(), Failure> {
    let path = format!("/Users/{}", session.user);
    let response = emby
        .endpoint
        .send(&carried(
            &emby.request(Method::Get, &path, None),
            &session.token,
        ))
        .await?;
    let policy = edited(emby, &response, edit)?;
    let writing = carried(
        &emby.request(Method::Post, &format!("{path}/Policy"), Some(policy)),
        &session.token,
    );
    let response = emby.endpoint.send(&writing).await?;
    emby.endpoint.expect_success(&response)
}

fn edited(
    emby: &Emby,
    response: &lemonfiber_ports::http::Response,
    edit: &Edit<'_>,
) -> Result<String, Failure> {
    let held: serde_json::Value = emby
        .endpoint
        .decode(response, "what the account is allowed could not be read")?;
    let Some(mut policy) = held
        .get(POLICY)
        .and_then(serde_json::Value::as_object)
        .cloned()
    else {
        return Err(emby.endpoint.refused("the account carries no policy"));
    };
    match edit {
        Edit::Allow(allowed) => narrowed(&mut policy, allowed),
        Edit::Claimable(allowed) => {
            narrowed(&mut policy, allowed);
            policy.insert(DISABLED.to_owned(), false.into());
            policy.insert(WRONG_SO_FAR.to_owned(), 0.into());
            policy.insert(LOCKED_OUT_AT.to_owned(), 0.into());
        }
        Edit::Suspend => {
            policy.insert(DISABLED.to_owned(), true.into());
        }
        Edit::Hide => {
            policy.insert(HIDDEN.to_owned(), true.into());
        }
    }
    Ok(serde_json::Value::Object(policy).to_string())
}

fn narrowed(policy: &mut serde_json::Map<String, serde_json::Value>, allowed: &Allowed) {
    if let Some(libraries) = &allowed.libraries {
        policy.insert(EVERY_LIBRARY.to_owned(), false.into());
        policy.insert(CHOSEN_LIBRARIES.to_owned(), libraries.clone().into());
    }
    if let Some(limit) = allowed.age_limit {
        policy.insert(AGE_LIMIT.to_owned(), rating::level(limit).into());
    }
    if let Some(unrated) = allowed.unrated {
        let kinds = match unrated {
            Unrated::HeldBack => UNRATED_KINDS.to_vec(),
            Unrated::LetThrough => Vec::new(),
        };
        policy.insert(UNRATED.to_owned(), kinds.into());
    }
}
