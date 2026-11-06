use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::Failure;

use super::Emby;

pub(super) const CONFIGURATION: &str = "/System/Configuration";

pub(super) async fn document(
    emby: &Emby,
    path: &str,
    what: &str,
) -> Result<serde_json::Value, Failure> {
    let response = emby.as_admin(Method::Get, path, None).await?;
    emby.endpoint.decode(
        &response,
        &format!("the document holding {what} could not be read"),
    )
}
