use super::super::item::{ItemResource, ItemsResource, LISTED};
use super::super::{item_type, Emby};
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Failure, Item};

pub(super) async fn holdings(
    emby: &Emby,
    member: Option<&str>,
    most: u32,
) -> Result<Vec<Item>, Failure> {
    if member.is_some_and(|member| !crate::shape::an_account(member)) {
        return Err(emby.misshapen("an account"));
    }
    let response = emby
        .as_admin(Method::Get, &asked(member, most), None)
        .await?;
    let held: ItemsResource = emby
        .endpoint
        .decode(&response, "what the household holds could not be read")?;
    Ok(held.items.into_iter().map(ItemResource::item).collect())
}

fn asked(member: Option<&str>, most: u32) -> String {
    let kinds = Kind::ALL.map(item_type).join(",");
    let most = most.to_string();
    let mut fields = vec![
        ("Recursive", "true"),
        ("IncludeItemTypes", kinds.as_str()),
        ("SortBy", "DateCreated"),
        ("SortOrder", "Descending"),
        ("Limit", most.as_str()),
        ("Fields", LISTED),
    ];
    if let Some(member) = member {
        fields.push(("UserId", member));
    }
    format!("/Items?{}", form_encoded(&fields))
}
