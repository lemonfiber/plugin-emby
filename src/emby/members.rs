use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Answered, Failure};

use super::declared::SPACE;
use super::item::ItemsResource;
use super::{carried, Emby};
use crate::shape::sourced;

const REFUSED_THE_TOKEN: u16 = 401;

const NOT_FROM_THERE: u16 = 403;

const REFUSED_THE_ASK: std::ops::Range<u16> = 400..500;

const UNREAD: &str = "Emby would not say whether the member may have the item";

impl Emby {
    /// Whether the member whose token is `token` may have the bytes of `item`, an id in
    /// `space`, as Emby answers that member's own query for it.
    ///
    /// # Errors
    ///
    /// Where Emby does not answer, or answers in a way no question to it expects.
    pub async fn may(&self, space: &str, item: &str, token: &str) -> Result<Answered, Failure> {
        let Some(item) = sourced(item).filter(|_| space == SPACE) else {
            return Ok(Answered::MayNot);
        };
        let asking = carried(
            &self.request(Method::Get, &format!("/Items?Ids={item}"), None),
            token,
        );
        let response = self.endpoint.send(&asking).await?;
        match response.status {
            REFUSED_THE_TOKEN => return Ok(Answered::NoSession),
            NOT_FROM_THERE => return Ok(Answered::NotFromThere),
            status if REFUSED_THE_ASK.contains(&status) => return Ok(Answered::MayNot),
            _ => {}
        }
        if !response.is_success() {
            return Err(self.endpoint.refused(UNREAD));
        }
        let shown: ItemsResource = self.endpoint.decode(&response, UNREAD)?;
        Ok(if shown.items.iter().any(|shown| shown.id == item) {
            Answered::May
        } else {
            Answered::MayNot
        })
    }
}
