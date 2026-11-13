use async_trait::async_trait;

use super::{keys, Emby};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Allowing, AppKeys, Astray, Dated, Failure, Fronted, OutsideAccount, OutsideServer,
};

const RESTART: &str = "/System/Restart";

const NO_PROXIES: &str =
    "Emby holds no list of addresses trusted to name the client, only which kinds of address it believes";

const NO_ORIGINS: &str =
    "Emby holds no list of origins a browser may read it from, and answers every origin";

const NO_OUTSIDE: &str = "No outside service holds the accounts of an Emby set up on its own";

#[async_trait]
impl Fronted for Emby {
    async fn known_proxies(&self) -> Result<Vec<String>, Failure> {
        Err(Self::unsupported(NO_PROXIES))
    }

    async fn trust_only(&self, _address: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_PROXIES))
    }

    async fn allowed_origins(&self) -> Result<Vec<String>, Failure> {
        Err(Self::unsupported(NO_ORIGINS))
    }

    async fn allow_only(&self, _origin: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_ORIGINS))
    }

    async fn restart(&self) -> Result<(), Failure> {
        let response = self.as_admin(Method::Post, RESTART, None).await?;
        self.endpoint.expect_success(&response)
    }
}

#[async_trait]
impl Allowing for Emby {
    async fn allow_origins(&self, _origins: &[String]) -> Result<(), Failure> {
        Err(Self::unsupported(NO_ORIGINS))
    }
}

#[async_trait]
impl AppKeys for Emby {
    async fn filed_as(&self, app: &str) -> Result<Vec<String>, Failure> {
        keys::filed_as(self, app).await
    }

    async fn dated(&self, app: &str) -> Result<Vec<Dated>, Failure> {
        keys::dated(self, app).await
    }

    async fn mint(&self, app: &str) -> Result<String, Failure> {
        keys::mint(self, app).await
    }

    async fn answers_to(&self, key: &str) -> Result<(), Failure> {
        keys::answers_to(self, key).await
    }

    async fn revoke(&self, key: &str) -> Result<(), Failure> {
        keys::revoke(self, key).await
    }

    async fn astray(&self, app: &str) -> Result<Vec<Astray>, Failure> {
        keys::astray(self, app).await
    }

    async fn account(&self, _app: &str) -> Result<OutsideAccount, Failure> {
        Err(Self::unsupported(NO_OUTSIDE))
    }

    async fn listed_as(&self) -> Result<OutsideServer, Failure> {
        Err(Self::unsupported(NO_OUTSIDE))
    }
}
