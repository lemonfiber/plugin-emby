//! What this adapter says it is, and the kit that serves its contracts.

use std::sync::Arc;

use lemonfiber_adapter_kit::members::Asking;
use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::{identity, media};

use crate::emby::{presented, Emby};

/// Where Emby answers, by its service id and the port it listens on.
pub const ADDRESS: &str = "http://emby:8096";

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Emby Server";

/// The Emby Server release this adapter's recordings were taken from.
pub const RELEASE: &str = "4.10.2.0";

/// The digest of the image that release's recordings were taken from.
pub const DIGEST: &str = "sha256:7082c63b10a2fcb3805a7eaebba407d5723bc57c085c9f0b29b7d0a702765cc1";

/// What this adapter says it is: the contracts it speaks and the release it supports.
#[must_use]
pub fn about() -> About {
    About {
        speaks: vec![
            lemonfiber_contract::spoken(media::serve::CAPABILITY, media::serve::MAJOR),
            lemonfiber_contract::spoken(identity::source::CAPABILITY, identity::source::MAJOR),
        ],
        upstream: UPSTREAM.to_owned(),
        releases: vec![Release {
            version: RELEASE.to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// Both contracts, served from `emby` under `key`, ready while Emby answers, and the
/// guard's question about an item answered by asking Emby with the member's token.
#[must_use]
pub fn kit(key: String, emby: Arc<Emby>) -> Kit {
    let asked = Arc::clone(&emby);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    let members = Arc::clone(&emby);
    let asking: Asking = Arc::new(move |space, item, token| {
        let members = Arc::clone(&members);
        Box::pin(async move { members.may(&space, &item, &token).await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .asked_by_members(presented(), asking)
        .serving(media::serve::served(Arc::clone(&emby)))
        .serving(identity::source::served(emby))
}

#[cfg(test)]
mod tests;
