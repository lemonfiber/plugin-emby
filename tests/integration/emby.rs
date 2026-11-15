use lemonfiber_fixtures::http::Fake;
use std::sync::Arc;

use emby_adapter::credential::{Credential, Holding};
use emby_adapter::emby::Emby;
use lemonfiber_ports::http::{Http, Method, Request};

const BASE: &str = "http://127.0.0.1:8096";

const ADMIN: &str = "a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3";

const MEMBER: &str = "4f53cda18c2baa0c0354bb5f9a3ecbe5";

const SIGNED_IN: &str =
    r#"{"AccessToken":"token","User":{"Id":"a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3"}}"#;

struct Nothing;

impl Holding for Nothing {
    fn credential(&self) -> Result<Credential, String> {
        Err("upstream.json could not be opened: not found".to_owned())
    }
}

fn a_word() -> String {
    ('g'..='w').collect()
}

fn emby(fake: &Arc<Fake>) -> Emby {
    let http: Arc<dyn Http> = fake.clone();
    Emby::new(http, BASE, Arc::new(Nothing))
}

fn administrator() -> Credential {
    Credential {
        username: "admin".to_owned(),
        password: ('a'..='p').collect(),
    }
}

fn reader(fake: &Arc<Fake>) -> Emby {
    let http: Arc<dyn Http> = fake.clone();
    Emby::new(http, BASE, Arc::new(administrator()))
}

fn asked(fake: &Fake, method: Method, ending: &str) -> Option<Request> {
    fake.requests()
        .into_iter()
        .find(|asked| asked.method == method && asked.url.ends_with(ending))
}

fn sent(fake: &Fake, method: Method, ending: &str) -> serde_json::Value {
    asked(fake, method, ending)
        .and_then(|asked| asked.body)
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or_default()
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers
        .iter()
        .find(|(held, _)| held.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

mod accounts;
mod declaring;
mod encrypting;
mod furnishing;
mod keys;
mod library;
mod members;
mod policies;
mod recounting;
mod screening;
mod serving;
mod sessions;
mod setting_up;
mod tuning;
mod upkeep;
