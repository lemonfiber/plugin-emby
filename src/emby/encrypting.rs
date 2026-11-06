use std::time::Duration;

use async_trait::async_trait;
use lemonfiber_pinned::Https;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Applied, Encrypting, Failure};
use serde_json::{json, Value};

use super::configured::{document, CONFIGURATION};
use super::declared::INTERNAL_TLS;
use super::Emby;

const WHAT: &str = "the server's configuration";

const HANDSHAKE_WITHIN: Duration = Duration::from_secs(5);

const PIN_DIGITS: usize = 64;

pub(super) fn internal_tls(base: &str) -> Option<Https> {
    reqwest::Url::parse(base)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .and_then(|host| Https::parse(&format!("https://{host}:{INTERNAL_TLS}")))
}

fn pinned(pin: &str) -> bool {
    pin.len() == PIN_DIGITS
        && pin
            .bytes()
            .all(|digit| digit.is_ascii_digit() || (b'a'..=b'f').contains(&digit))
}

fn serving(path: &str) -> [(&'static str, Value); 3] {
    [
        ("CertificatePath", json!(path)),
        ("CertificatePassword", json!("")),
        ("HttpsPortNumber", json!(INTERNAL_TLS)),
    ]
}

fn served(settings: &Value, path: &str) -> bool {
    serving(path)
        .iter()
        .all(|(field, value)| match settings.get(*field) {
            Some(Value::Null) | None => value.as_str() == Some(""),
            Some(held) => held == value,
        })
}

async fn serve_tls(emby: &Emby, path: &str, pin: &str) -> Result<Applied, Failure> {
    if path.trim().is_empty() {
        return Err(emby
            .endpoint
            .refused("no certificate path was given to serve the internal TLS port with"));
    }
    if !pinned(pin) {
        return Err(emby.endpoint.refused(&format!(
            "{pin:?} is not a certificate's SHA-256 in lower-case hexadecimal"
        )));
    }
    let mut settings = document(emby, CONFIGURATION, WHAT).await?;
    if served(&settings, path) {
        let presented = match &emby.internal_tls {
            Some(at) => lemonfiber_pinned::presents(at, pin, HANDSHAKE_WITHIN).await,
            None => false,
        };
        return Ok(if presented {
            Applied::Now
        } else {
            Applied::OnRestart
        });
    }
    let Some(fields) = settings.as_object_mut() else {
        return Err(emby.endpoint.refused(&format!("{WHAT} is not an object")));
    };
    for (field, value) in serving(path) {
        fields.insert(field.to_owned(), value);
    }
    let response = emby
        .as_admin(Method::Post, CONFIGURATION, Some(settings.to_string()))
        .await?;
    emby.endpoint.expect_success(&response)?;
    if !served(&document(emby, CONFIGURATION, WHAT).await?, path) {
        return Err(emby.endpoint.refused(&format!(
            "{WHAT} was written to serve the internal TLS port with {path} and read back otherwise"
        )));
    }
    Ok(Applied::OnRestart)
}

#[async_trait]
impl Encrypting for Emby {
    async fn serve_tls(&self, path: &str, pin: &str) -> Result<Applied, Failure> {
        serve_tls(self, path, pin).await
    }
}
