use serde::Deserialize;

use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Astray, Dated, Failure};

use super::{carried, Emby};
use crate::shape::a_key;

const KEYS: &str = "/Auth/Keys";

pub(super) async fn filed_as(emby: &Emby, app: &str) -> Result<Vec<String>, Failure> {
    Ok(listed(emby, app)
        .await?
        .into_iter()
        .map(|key| key.access_token)
        .collect())
}

pub(super) async fn dated(emby: &Emby, app: &str) -> Result<Vec<Dated>, Failure> {
    Ok(listed(emby, app)
        .await?
        .into_iter()
        .map(|key| Dated {
            created: key.created,
            last_used: None,
        })
        .collect())
}

pub(super) async fn astray(emby: &Emby, app: &str) -> Result<Vec<Astray>, Failure> {
    listed(emby, app).await?;
    Ok(Vec::new())
}

async fn listed(emby: &Emby, app: &str) -> Result<Vec<Key>, Failure> {
    let response = emby.as_admin(Method::Get, KEYS, None).await?;
    let keys: Keys = emby
        .endpoint
        .decode(&response, "the key list could not be read")?;
    Ok(keys
        .items
        .into_iter()
        .filter(|key| key.app_name == app && !key.access_token.is_empty())
        .collect())
}

pub(super) async fn mint(emby: &Emby, app: &str) -> Result<String, Failure> {
    let before = filed_as(emby, app).await?;
    let asked = form_encoded(&[("App", app)]);
    let response = emby
        .as_admin(Method::Post, &format!("{KEYS}?{asked}"), None)
        .await?;
    emby.endpoint.expect_success(&response)?;
    let mut new: Vec<String> = filed_as(emby, app)
        .await?
        .into_iter()
        .filter(|key| !before.contains(key))
        .collect();
    match (new.pop(), new.is_empty()) {
        (Some(key), true) => Ok(key),
        _ => Err(emby.endpoint.refused(&format!(
            "a key was minted as {app} and the key list does not show exactly one new one"
        ))),
    }
}

pub(super) async fn answers_to(emby: &Emby, key: &str) -> Result<(), Failure> {
    if !a_key(key) {
        return Err(emby.misshapen("a key"));
    }
    let asking = carried(&emby.request(Method::Get, "/System/Info", None), key);
    let response = emby.endpoint.send(&asking).await?;
    emby.endpoint.expect_success(&response)
}

pub(super) async fn revoke(emby: &Emby, key: &str) -> Result<(), Failure> {
    if !a_key(key) {
        return Err(emby.misshapen("a key"));
    }
    let response = emby
        .as_admin(Method::Delete, &format!("{KEYS}/{key}"), None)
        .await?;
    emby.endpoint.expect_success(&response)
}

#[derive(Deserialize)]
struct Keys {
    #[serde(rename = "Items", default)]
    items: Vec<Key>,
}

#[derive(Deserialize)]
struct Key {
    #[serde(rename = "AppName", default)]
    app_name: String,
    #[serde(rename = "AccessToken", default)]
    access_token: String,
    #[serde(rename = "DateCreated", default)]
    created: Option<String>,
}
