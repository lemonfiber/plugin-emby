const DEVICE_SHORTEST: usize = 8;

const DEVICE_LONGEST: usize = 64;

const KEY_LONGEST: usize = 128;

const ITEM_LONGEST: usize = 19;

pub(crate) const SOURCE_PREFIX: &str = "mediasource_";

const LIVE_STREAM_LONGEST: usize = 200;

const HEX_DIGITS: usize = 32;

const CLOSED_BY: char = '.';

/// Whether `id` is shaped like an id Emby files an account under: thirty-two hex
/// digits, with or without the four dashes.
#[must_use]
pub fn an_account(id: &str) -> bool {
    let digits: Vec<char> = id.chars().filter(|letter| *letter != '-').collect();
    let dashed = id.len() == 36
        && id
            .char_indices()
            .all(|(at, letter)| (letter == '-') == matches!(at, 8 | 13 | 18 | 23));
    (id.len() == 32 || dashed) && digits.len() == 32 && digits.iter().all(char::is_ascii_hexdigit)
}

/// Whether `id` is shaped like an id Emby files an item under: one to nineteen digits.
#[must_use]
pub fn an_item(id: &str) -> bool {
    (1..=ITEM_LONGEST).contains(&id.len()) && id.bytes().all(|digit| digit.is_ascii_digit())
}

/// The item a media source's id names, `mediasource_` and the item's own id, or the
/// item's own id alone.
#[must_use]
pub fn sourced(id: &str) -> Option<&str> {
    let item = id
        .get(..SOURCE_PREFIX.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(SOURCE_PREFIX))
        .and_then(|_| id.get(SOURCE_PREFIX.len()..))
        .unwrap_or(id);
    an_item(item).then_some(item)
}

fn hex(text: &str) -> bool {
    text.len() == HEX_DIGITS && text.bytes().all(|digit| digit.is_ascii_hexdigit())
}

/// Whether `id` is shaped like the id Emby serves an opened live stream's buffer under:
/// thirty-two lower-case hex digits.
#[must_use]
pub fn a_live_stream_file(id: &str) -> bool {
    hex(id) && !id.bytes().any(|digit| digit.is_ascii_uppercase())
}

/// Whether `id` is shaped like the id Emby names a play session by: thirty-two hex digits.
#[must_use]
pub fn a_play_session(id: &str) -> bool {
    hex(id)
}

fn a_live_stream_id(id: &str) -> bool {
    (1..=LIVE_STREAM_LONGEST).contains(&id.len())
        && id
            .chars()
            .all(|letter| letter.is_ascii_alphanumeric() || letter == '_' || letter == '-')
}

/// An opened live stream, as this adapter names what closes it: Emby's id for the live
/// stream, a dot, and the play session that opened it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveStream<'a> {
    live: &'a str,
    play_session: &'a str,
}

impl<'a> LiveStream<'a> {
    /// `text`, where it is shaped like a live stream's id, a dot and a play session.
    #[must_use]
    pub fn of(text: &'a str) -> Option<Self> {
        let (live, play_session) = text.split_once(CLOSED_BY)?;
        (a_live_stream_id(live) && a_play_session(play_session))
            .then_some(Self { live, play_session })
    }

    /// Emby's id for the live stream.
    #[must_use]
    pub const fn live(&self) -> &'a str {
        self.live
    }

    /// The play session that opened it.
    #[must_use]
    pub const fn play_session(&self) -> &'a str {
        self.play_session
    }
}

impl std::fmt::Display for LiveStream<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}{CLOSED_BY}{}", self.live, self.play_session)
    }
}

/// A value held to the shape of an id Emby files an account under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account<'a>(&'a str);

impl<'a> Account<'a> {
    /// `text`, where it is shaped like an account's id.
    #[must_use]
    pub fn of(text: &'a str) -> Option<Self> {
        an_account(text).then_some(Self(text))
    }
}

impl std::fmt::Display for Account<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

/// A value held to the shape of an id Emby files an item under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item<'a>(&'a str);

impl<'a> Item<'a> {
    /// `text`, where it is shaped like an item's id.
    #[must_use]
    pub fn of(text: &'a str) -> Option<Self> {
        an_item(text).then_some(Self(text))
    }
}

impl std::fmt::Display for Item<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

/// Whether `device` is shaped like the id a player keeps for the device it plays on: eight
/// to sixty-four letters, digits and dashes, not all of them digits, which Emby would
/// read as its own number for a device.
#[must_use]
pub fn a_device(device: &str) -> bool {
    (DEVICE_SHORTEST..=DEVICE_LONGEST).contains(&device.len())
        && device
            .chars()
            .all(|letter| letter.is_ascii_alphanumeric() || letter == '-')
        && !device.bytes().all(|digit| digit.is_ascii_digit())
}

/// Whether `key` is shaped like a key Emby mints: letters and digits, and not empty.
#[must_use]
pub fn a_key(key: &str) -> bool {
    (1..=KEY_LONGEST).contains(&key.len())
        && key.chars().all(|letter| letter.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests;
