mod playing;
pub(super) mod policy;
mod shelf;

use std::collections::BTreeMap;

use async_trait::async_trait;

use super::{carried, configured, rating, signing_in, Emby, SIGNING_IN_HEADER, SIGN_IN};
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Access, Allowed, Certificate, Failure, Household, Invited, Item, Member, NamedLibrary,
    Playback, Session, Signed, Unrated,
};

use crate::shape::{a_device, a_key, an_account};

pub(super) const SESSIONS: &str = "/Sessions";

const METADATA_COUNTRY: &str = "MetadataCountryCode";

const RATINGS: &str = "/Localization/ParentalRatings";

const ACCOUNT_MADE: &str = "user.created";

const PASSWORD_MOVED: &str = "user.passwordchanged";

const AT_ONCE: u32 = 200;

const REFUSED: [u16; 2] = [401, 403];

const GONE: u16 = 404;

const SECONDS_WRITTEN: usize = "2000-01-01T00:00:00".len();

const UNSTOOD: &str = "whether the account still stands could not be read";

#[derive(serde::Deserialize)]
struct UserResource {
    #[serde(rename = "Id", default)]
    id: String,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "HasPassword", default)]
    has_password: bool,
    #[serde(rename = "Policy", default)]
    policy: Option<PolicyResource>,
    #[serde(rename = "LastActivityDate", default)]
    last_activity: Option<String>,
    #[serde(rename = "LastLoginDate", default)]
    last_login: Option<String>,
    #[serde(rename = "DateCreated", default)]
    created: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct PolicyResource {
    #[serde(rename = "EnableAllFolders", default)]
    every_library: bool,
    #[serde(rename = "EnabledFolders", default)]
    libraries: Vec<String>,
    #[serde(rename = "MaxParentalRating", default)]
    level: Option<u32>,
    #[serde(rename = "BlockUnratedItems", default)]
    unrated_blocked: Vec<String>,
    #[serde(rename = "IsAdministrator", default)]
    administrator: bool,
    #[serde(rename = "IsDisabled", default)]
    disabled: bool,
}

impl UserResource {
    fn member(self) -> Member {
        let policy = self.policy.unwrap_or_default();
        Member {
            id: self.id,
            name: self.name,
            claimed: self.has_password,
            access: Access {
                every_library: policy.every_library,
                libraries: policy.libraries,
                age_limit: policy.level.and_then(rating::age),
                unrated: if policy.unrated_blocked.is_empty() {
                    Unrated::LetThrough
                } else {
                    Unrated::HeldBack
                },
                administrator: policy.administrator,
                disabled: policy.disabled,
            },
            last_seen: self.last_activity.or(self.last_login),
        }
    }

    fn stands(&self) -> bool {
        self.has_password && self.policy.as_ref().is_none_or(|policy| !policy.disabled)
    }
}

#[derive(serde::Deserialize)]
struct SessionResource {
    #[serde(rename = "UserId", default)]
    user: String,
    #[serde(rename = "DeviceId", default)]
    device_id: String,
    #[serde(rename = "DeviceName", default)]
    device: String,
    #[serde(rename = "Client", default)]
    client: String,
    #[serde(rename = "LastActivityDate", default)]
    last_activity: Option<String>,
}

#[derive(serde::Deserialize)]
struct RatingResource {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "Value", default)]
    level: Option<u32>,
}

#[derive(serde::Deserialize)]
struct FolderResource {
    #[serde(rename = "Guid", default)]
    guid: String,
    #[serde(rename = "Name", default)]
    name: String,
}

#[derive(serde::Deserialize)]
struct FoldersResource {
    #[serde(rename = "Items", default)]
    items: Vec<FolderResource>,
}

#[derive(serde::Deserialize)]
struct EntryResource {
    #[serde(rename = "Type", default)]
    kind: String,
    #[serde(rename = "Date", default)]
    date: String,
    #[serde(rename = "UserId", default)]
    user: Option<String>,
}

#[derive(serde::Deserialize)]
struct ActivityResource {
    #[serde(rename = "Items", default)]
    items: Vec<EntryResource>,
}

#[derive(serde::Deserialize)]
struct SignedIn {
    #[serde(rename = "User", default)]
    user: SignedInUser,
    #[serde(rename = "AccessToken", default)]
    token: String,
}

#[derive(serde::Deserialize, Default)]
struct SignedInUser {
    #[serde(rename = "Id", default)]
    id: String,
}

#[async_trait]
impl Household for Emby {
    async fn household(&self) -> Result<Vec<Member>, Failure> {
        let response = self.as_admin(Method::Get, "/Users", None).await?;
        let held: Vec<UserResource> = self
            .endpoint
            .decode(&response, "the household's accounts could not be read")?;
        Ok(held.into_iter().map(UserResource::member).collect())
    }

    async fn whoever(
        &self,
        name: &str,
        password: &str,
        device: &str,
    ) -> Result<Option<Signed>, Failure> {
        whoever(self, name, password, device).await
    }

    async fn standing(&self, signed: &Signed) -> Result<bool, Failure> {
        standing(self, signed).await
    }

    async fn invite(&self, name: &str) -> Result<Member, Failure> {
        let body = serde_json::json!({ "Name": name }).to_string();
        let response = self
            .as_admin(Method::Post, "/Users/New", Some(body))
            .await?;
        let made: UserResource = self
            .endpoint
            .decode(&response, "the account could not be read back")?;
        policy::rewritten(self, &made.id, &policy::Edit::Hide).await?;
        Ok(made.member())
    }

    async fn claim(&self, name: &str, password: &str, device: &str) -> Result<bool, Failure> {
        claim(self, name, password, device).await
    }

    async fn unclaim(&self, id: &str) -> Result<(), Failure> {
        if !an_account(id) {
            return Err(self.misshapen("an account"));
        }
        let body = serde_json::json!({ "ResetPassword": true }).to_string();
        let response = self
            .as_admin(Method::Post, &format!("/Users/{id}/Password"), Some(body))
            .await?;
        self.endpoint.expect_success(&response)
    }

    async fn withdraw(&self, id: &str) -> Result<(), Failure> {
        if !an_account(id) {
            return Err(self.misshapen("an account"));
        }
        let response = self
            .as_admin(Method::Delete, &format!("/Users/{id}"), None)
            .await?;
        self.endpoint.expect_success(&response)
    }

    async fn when_invited(&self, since: &str) -> Result<Vec<Invited>, Failure> {
        when_invited(self, since).await
    }

    async fn ratings(&self) -> Result<Vec<Certificate>, Failure> {
        let response = self.as_admin(Method::Get, RATINGS, None).await?;
        let held: Vec<RatingResource> = self
            .endpoint
            .decode(&response, "the server's own ratings could not be read")?;
        Ok(held
            .into_iter()
            .filter_map(|rating| {
                rating.level.and_then(rating::age).map(|age| Certificate {
                    name: rating.name,
                    age,
                })
            })
            .collect())
    }

    async fn country(&self) -> Result<Option<String>, Failure> {
        let configuration =
            configured::document(self, configured::CONFIGURATION, "the country").await?;
        Ok(configuration
            .get(METADATA_COUNTRY)
            .and_then(serde_json::Value::as_str)
            .filter(|code| code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_alphabetic()))
            .map(str::to_ascii_uppercase))
    }

    async fn allow(&self, id: &str, allowed: &Allowed) -> Result<(), Failure> {
        policy::rewritten(self, id, &policy::Edit::Allow(allowed)).await
    }

    async fn claimable(&self, id: &str, allowed: &Allowed) -> Result<(), Failure> {
        policy::rewritten(self, id, &policy::Edit::Claimable(allowed)).await
    }

    async fn suspend(&self, id: &str) -> Result<(), Failure> {
        policy::rewritten(self, id, &policy::Edit::Suspend).await
    }

    async fn sessions(&self, member: &str) -> Result<Vec<Session>, Failure> {
        let response = self.as_admin(Method::Get, SESSIONS, None).await?;
        let listed: Vec<SessionResource> = self
            .endpoint
            .decode(&response, "the media server's sessions could not be read")?;
        Ok(listed
            .into_iter()
            .filter(|session| session.user == member)
            .map(|session| Session {
                device_id: session.device_id,
                device: session.device,
                client: session.client,
                last_seen: session.last_activity,
            })
            .collect())
    }

    async fn signs_devices_in(&self) -> Result<bool, Failure> {
        Ok(false)
    }

    async fn libraries(&self) -> Result<Vec<NamedLibrary>, Failure> {
        let response = self
            .as_admin(Method::Get, "/Library/MediaFolders?Fields=Guid", None)
            .await?;
        let held: FoldersResource = self
            .endpoint
            .decode(&response, "the server's libraries could not be read")?;
        Ok(held
            .items
            .into_iter()
            .map(|folder| NamedLibrary {
                id: folder.guid,
                name: folder.name,
            })
            .collect())
    }
}

async fn standing(emby: &Emby, signed: &Signed) -> Result<bool, Failure> {
    if !a_key(&signed.token) || !an_account(&signed.id) {
        return Ok(false);
    }
    let asking = carried(&emby.request(Method::Get, SESSIONS, None), &signed.token);
    let response = emby.endpoint.send(&asking).await?;
    if REFUSED.contains(&response.status) {
        return Ok(false);
    }
    let sessions: Vec<SessionResource> = emby.endpoint.decode(&response, UNSTOOD)?;
    if !sessions.iter().any(|session| session.user == signed.id) {
        return Ok(false);
    }
    let asking = carried(
        &emby.request(Method::Get, &format!("/Users/{}", signed.id), None),
        &signed.token,
    );
    let response = emby.endpoint.send(&asking).await?;
    if REFUSED.contains(&response.status) || response.status == GONE {
        return Ok(false);
    }
    let held: UserResource = emby.endpoint.decode(&response, UNSTOOD)?;
    Ok(held.id == signed.id && held.stands())
}

async fn whoever(
    emby: &Emby,
    name: &str,
    password: &str,
    device: &str,
) -> Result<Option<Signed>, Failure> {
    if !a_device(device) {
        return Err(emby.misshapen("a device"));
    }
    let body = serde_json::json!({ "Username": name, "Pw": password }).to_string();
    let mut request = emby.request(Method::Post, SIGN_IN, Some(body));
    request
        .headers
        .push((SIGNING_IN_HEADER.to_owned(), signing_in(device)));
    let response = emby.endpoint.send(&request).await?;
    if REFUSED.contains(&response.status) {
        return Ok(None);
    }
    let signed: SignedIn = emby
        .endpoint
        .decode(&response, "the sign-in was not accepted")?;
    Ok(Some(Signed {
        id: signed.user.id,
        token: signed.token,
    })
    .filter(|signed| an_account(&signed.id) && a_key(&signed.token)))
}

async fn claim(emby: &Emby, name: &str, password: &str, device: &str) -> Result<bool, Failure> {
    let unclaimed = String::new();
    let Some(signed) = whoever(emby, name, &unclaimed, device).await? else {
        return Ok(false);
    };
    let body = serde_json::json!({ "NewPw": password }).to_string();
    let changing = carried(
        &emby.request(
            Method::Post,
            &format!("/Users/{}/Password", signed.id),
            Some(body),
        ),
        &signed.token,
    );
    let response = emby.endpoint.send(&changing).await?;
    emby.endpoint.expect_success(&response)?;
    let leaving = carried(
        &emby.request(Method::Post, "/Sessions/Logout", None),
        &signed.token,
    );
    let _ = emby.endpoint.send(&leaving).await;
    Ok(true)
}

fn to_the_second(moment: &str) -> &str {
    moment.get(..SECONDS_WRITTEN).unwrap_or(moment)
}

async fn when_invited(emby: &Emby, since: &str) -> Result<Vec<Invited>, Failure> {
    let response = emby.as_admin(Method::Get, "/Users", None).await?;
    let held: Vec<UserResource> = emby
        .endpoint
        .decode(&response, "the household's accounts could not be read")?;
    let mut latest: Vec<(String, String)> = held
        .into_iter()
        .filter_map(|user| Some((user.id, user.created?)))
        .filter(|(id, created)| an_account(id) && to_the_second(created) >= to_the_second(since))
        .collect();
    let asked = form_encoded(&[("MinDate", since), ("Limit", &AT_ONCE.to_string())]);
    let path = format!("/System/ActivityLog/Entries?{asked}");
    let response = emby.as_admin(Method::Get, &path, None).await?;
    let recorded: ActivityResource = emby.endpoint.decode(
        &response,
        "what the media server recorded could not be read",
    )?;
    let mut accounts: BTreeMap<String, Option<String>> = BTreeMap::new();
    for entry in recorded.items {
        let Some(user) = entry
            .user
            .filter(|_| entry.kind == ACCOUNT_MADE || entry.kind == PASSWORD_MOVED)
            .filter(|user| crate::shape::an_item(user))
        else {
            continue;
        };
        let account = if let Some(account) = accounts.get(&user) {
            account.clone()
        } else {
            let account = held_as(emby, &user).await?;
            accounts.insert(user, account.clone());
            account
        };
        let Some(member) = account else {
            continue;
        };
        match latest.iter_mut().find(|(id, _)| *id == member) {
            Some((_, at)) if *at < entry.date => *at = entry.date,
            Some(_) => {}
            None => latest.push((member, entry.date)),
        }
    }
    Ok(latest
        .into_iter()
        .map(|(member, at)| Invited { member, at })
        .collect())
}

async fn held_as(emby: &Emby, internal: &str) -> Result<Option<String>, Failure> {
    let response = emby
        .as_admin(Method::Get, &format!("/Users/{internal}"), None)
        .await?;
    if response.status == GONE {
        return Ok(None);
    }
    let held: UserResource = emby.endpoint.decode(
        &response,
        "an account the media server recorded could not be read",
    )?;
    Ok(Some(held.id).filter(|id| an_account(id)))
}

pub(super) async fn holdings(
    emby: &Emby,
    member: Option<&str>,
    most: u32,
) -> Result<Vec<Item>, Failure> {
    shelf::holdings(emby, member, most).await
}

pub(super) async fn now_playing(
    emby: &Emby,
    member: Option<&str>,
) -> Result<Vec<Playback>, Failure> {
    playing::playing(emby, member).await
}
