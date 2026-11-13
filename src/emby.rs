//! Talking to Emby, the media server, which lemonfiber holds an administrator's account
//! on rather than a key.

use std::sync::Arc;

use lemonfiber_adapter_kit::endpoint::Endpoint;
use lemonfiber_ports::http::{Fetched, Http, Method, Request, Response, JSON};
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::Failure;
use serde::Deserialize;

use crate::credential::{Credential, Holding};
use crate::shape::an_account;

mod configured;
mod declared;
mod encrypting;
mod furnishing;
mod household;
mod item;
mod keys;
mod library;
mod members;
mod rating;
mod recounting;
mod screening;
mod serving;
mod sessions;
mod setup;
mod upkeep;

pub use declared::presented;
pub use recounting::Happenings;
pub use sessions::Sessions;

/// What every failure is blamed on.
pub const SERVICE: &str = "emby";

const SIGNING_IN_HEADER: &str = "X-Emby-Authorization";

const TOKEN_HEADER: &str = "X-Emby-Token";

const PUBLIC_INFO: &str = "/System/Info/Public";

const SIGN_IN: &str = "/Users/AuthenticateByName";

fn signing_in(device: &str) -> String {
    format!(r#"Emby Client="lemonfiber", Device="lemonfiber", DeviceId="{device}", Version="1""#)
}

const ADAPTER_DEVICE: &str = "lemonfiber";

fn carried(request: &Request, token: &str) -> Request {
    let mut carried = request.clone();
    carried
        .headers
        .push((TOKEN_HEADER.to_owned(), token.to_owned()));
    carried
}

/// A client for one Emby, signing in as the administrator whose credential `held`
/// supplies.
pub struct Emby {
    endpoint: Endpoint,
    http: Arc<dyn Http>,
    held: Arc<dyn Holding>,
    sessions: Sessions,
    internal_tls: Option<lemonfiber_pinned::Https>,
    recounter: std::sync::OnceLock<lemonfiber_adapter_kit::recounting::Recounter>,
}

impl Emby {
    /// The Emby reached at `base` over `http`, signed in to with what `held` supplies.
    #[must_use]
    pub fn new(http: Arc<dyn Http>, base: impl Into<String>, held: Arc<dyn Holding>) -> Self {
        let base = base.into();
        Self {
            internal_tls: encrypting::internal_tls(&base),
            endpoint: Endpoint::new(Arc::clone(&http), base, SERVICE),
            http,
            held,
            sessions: Sessions::default(),
            recounter: std::sync::OnceLock::new(),
        }
    }

    /// The same, reaching Emby's internal TLS port at `at` in place of its own host at
    /// the port it declares.
    #[must_use]
    pub fn with_internal_tls(self, at: lemonfiber_pinned::Https) -> Self {
        Self {
            internal_tls: Some(at),
            ..self
        }
    }

    /// Whether Emby itself answers its public information, naming the server and its
    /// version.
    pub async fn answers(&self) -> bool {
        self.endpoint
            .send(&self.request(Method::Get, PUBLIC_INFO, None))
            .await
            .is_ok_and(|response| {
                response.is_success()
                    && serde_json::from_str::<PublicInfo>(&response.body)
                        .is_ok_and(|info| !info.id.is_empty() && !info.version.is_empty())
            })
    }

    fn request(&self, method: Method, path: &str, body: Option<String>) -> Request {
        Request {
            method,
            url: self.endpoint.url(path),
            headers: body
                .as_ref()
                .map(|_| ("Content-Type".to_owned(), JSON.to_owned()))
                .into_iter()
                .collect(),
            body,
            pinned: None,
        }
    }

    async fn fetch(&self, request: &Request, most: usize) -> Result<Fetched, Failure> {
        self.http
            .fetch(request, most)
            .await
            .map_err(|_| Failure::Unavailable {
                service: SERVICE.to_owned(),
            })
    }

    fn credential(&self) -> Result<Credential, Failure> {
        self.held.credential().map_err(|why| {
            self.endpoint
                .refused(&format!("this adapter holds no credential for Emby: {why}"))
        })
    }

    async fn as_admin(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> Result<Response, Failure> {
        self.as_admin_naming(method, |_| path.to_owned(), body)
            .await
    }

    async fn as_admin_naming(
        &self,
        method: Method,
        path: impl Fn(&str) -> String + Send + Sync,
        body: Option<String>,
    ) -> Result<Response, Failure> {
        let credential = self.credential()?;
        if let Some(held) = self.sessions.held(&credential) {
            let request = self.request(method, &path(&held.user), body.clone());
            let response = self.endpoint.send(&carried(&request, &held.token)).await?;
            if !matches!(response.status, 401 | 403) {
                return Ok(response);
            }
            self.sessions.forget(&credential);
        }
        let session = self.signed_in(&credential, ADAPTER_DEVICE).await?;
        self.sessions.keep(&credential, &session);
        let request = self.request(method, &path(&session.user), body);
        self.endpoint.send(&carried(&request, &session.token)).await
    }

    async fn signed_in(&self, credential: &Credential, device: &str) -> Result<Opened, Failure> {
        let body =
            serde_json::json!({ "Username": credential.username, "Pw": credential.password })
                .to_string();
        let mut request = self.request(Method::Post, SIGN_IN, Some(body));
        request
            .headers
            .push((SIGNING_IN_HEADER.to_owned(), signing_in(device)));
        let response = self.endpoint.send(&request).await?;
        let session: Session = self
            .endpoint
            .decode(&response, "the sign-in was not accepted")?;
        if session.access_token.is_empty() {
            return Err(self.endpoint.refused("the sign-in opened no session"));
        }
        if !an_account(&session.user.id) {
            return Err(self
                .endpoint
                .refused("the sign-in named no account Emby could hold"));
        }
        Ok(Opened {
            token: session.access_token,
            user: session.user.id,
        })
    }

    fn misshapen(&self, what: &str) -> Failure {
        self.endpoint
            .refused(&format!("that is not {what} Emby could hold"))
    }

    fn unsupported(detail: &str) -> Failure {
        Failure::Unsupported {
            service: SERVICE.to_owned(),
            detail: detail.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Opened {
    token: String,
    user: String,
}

#[derive(Deserialize)]
struct PublicInfo {
    #[serde(rename = "Id", default)]
    id: String,
    #[serde(rename = "Version", default)]
    version: String,
}

#[derive(Deserialize)]
struct Session {
    #[serde(rename = "AccessToken", default)]
    access_token: String,
    #[serde(rename = "User", default)]
    user: SessionUser,
}

#[derive(Deserialize, Default)]
struct SessionUser {
    #[serde(rename = "Id", default)]
    id: String,
}

const fn item_type(kind: Kind) -> &'static str {
    match kind {
        Kind::Tv => "Series",
        Kind::Movies => "Movie",
    }
}
