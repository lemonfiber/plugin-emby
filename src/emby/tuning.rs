use async_trait::async_trait;

use super::Emby;
use lemonfiber_ports::service::{Channel, Failure, Tuned, Tuning};

const NO_LIVE_TV: &str =
    "Emby runs live TV only under an Emby Premiere subscription, so this adapter answers no channel and opens no live stream";

#[async_trait]
impl Tuning for Emby {
    async fn channels(&self, _member: &str) -> Result<Vec<Channel>, Failure> {
        Err(Self::unsupported(NO_LIVE_TV))
    }

    async fn tune(&self, _member: &str, _channel: &str) -> Result<Option<Tuned>, Failure> {
        Err(Self::unsupported(NO_LIVE_TV))
    }

    async fn untune(&self, _closes_by: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_LIVE_TV))
    }
}
