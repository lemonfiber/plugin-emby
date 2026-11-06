//! lemonfiber's adapter for Emby: the `media.serve` and `identity.source` contracts,
//! spoken in front of one Emby server and translated to its own API.

pub mod adapter;
pub mod credential;
pub mod emby;
mod shape;
