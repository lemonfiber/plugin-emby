use async_trait::async_trait;

use super::household::policy::{rewritten_as, Edit};
use super::{carried, Emby, ADAPTER_DEVICE};
use crate::credential::Credential;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, MediaServer};

const WIZARD: &str = "/Startup/Configuration";

const FIRST_USER: &str = "/Startup/User";

const REMOTE_ACCESS: &str = "/Startup/RemoteAccess";

const COMPLETE: &str = "/Startup/Complete";

const WIZARD_DONE: u16 = 401;

#[async_trait]
impl MediaServer for Emby {
    async fn startup_completed(&self) -> Result<bool, Failure> {
        let response = self
            .endpoint
            .send(&self.request(Method::Get, WIZARD, None))
            .await?;
        if response.status == WIZARD_DONE {
            return Ok(true);
        }
        self.endpoint.expect_success(&response).map(|()| false)
    }

    async fn replace_admin_password(&self, replacement: &str) -> Result<(), Failure> {
        replaced(self, replacement).await
    }

    async fn create_admin(&self, name: &str, password: &str) -> Result<(), Failure> {
        created(self, name, password).await
    }
}

async fn created(emby: &Emby, name: &str, password: &str) -> Result<(), Failure> {
    let waiting = emby
        .endpoint
        .send(&emby.request(Method::Get, FIRST_USER, None))
        .await?;
    emby.endpoint.expect_success(&waiting)?;
    for (path, body) in [
        (
            FIRST_USER,
            Some(serde_json::json!({ "Name": name, "Password": password }).to_string()),
        ),
        (
            REMOTE_ACCESS,
            Some(serde_json::json!({ "EnableAutomaticPortMapping": false }).to_string()),
        ),
        (COMPLETE, None),
    ] {
        let response = emby
            .endpoint
            .send(&emby.request(Method::Post, path, body))
            .await?;
        emby.endpoint.expect_success(&response)?;
    }
    let made = Credential {
        username: name.to_owned(),
        password: password.to_owned(),
    };
    let session = emby.signed_in(&made, ADAPTER_DEVICE).await?;
    rewritten_as(emby, &session, &Edit::Hide).await
}

async fn replaced(emby: &Emby, replacement: &str) -> Result<(), Failure> {
    if replacement.is_empty() {
        return Err(emby.endpoint.refused("no replacement password was given"));
    }
    let in_force = emby.credential()?;
    let session = emby.signed_in(&in_force, ADAPTER_DEVICE).await?;
    let body = serde_json::json!({ "NewPw": replacement }).to_string();
    let changing = carried(
        &emby.request(
            Method::Post,
            &format!("/Users/{}/Password", session.user),
            Some(body),
        ),
        &session.token,
    );
    let response = emby.endpoint.send(&changing).await?;
    emby.endpoint.expect_success(&response)?;
    let proving = Credential {
        username: in_force.username,
        password: replacement.to_owned(),
    };
    match emby.signed_in(&proving, ADAPTER_DEVICE).await {
        Ok(_) => Ok(()),
        Err(Failure::Unauthorised { .. }) => Err(emby
            .endpoint
            .refused("Emby did not take the new password at a sign-in")),
        Err(failure) => Err(failure),
    }
}
