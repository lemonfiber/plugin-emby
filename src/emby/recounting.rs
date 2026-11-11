use async_trait::async_trait;
use lemonfiber_ports::service::{Failure, Recounting};

use super::Emby;

const NOT_YET: &str =
    "Emby sends what happened only through its webhooks, which need Emby Premiere, so this adapter does not set them up yet";

#[async_trait]
impl Recounting for Emby {
    async fn events(
        &self,
        _address: Option<&str>,
        _pin: Option<&str>,
        _key: Option<&str>,
    ) -> Result<(), Failure> {
        Err(Self::unsupported(NOT_YET))
    }
}
