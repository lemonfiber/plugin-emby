use lemonfiber_fixtures::http::Fake;
use lemonfiber_ports::service::{Failure, Tuning as _};

use super::{reader, MEMBER};

const CHANNEL: &str = "12";

const STREAM: &str = "a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3_a7f3a7f3a7f3a7f3a7f3a7f3a7f3a7f3";

fn premiere(answer: &Result<impl Sized, Failure>) -> bool {
    matches!(answer, Err(Failure::Unsupported { detail, .. }) if detail.contains("Emby Premiere"))
}

#[tokio::test]
async fn live_tv_is_answered_unsupported_without_asking_emby_since_it_needs_emby_premiere() {
    let fake = Fake::silent();
    let emby = reader(&fake);
    assert!(premiere(&emby.channels(MEMBER).await));
    assert!(premiere(&emby.tune(MEMBER, CHANNEL).await));
    assert!(premiere(&emby.untune(STREAM).await));
    assert!(fake.requests().is_empty());
}
