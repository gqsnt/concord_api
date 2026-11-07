//! Provider models: snowflake identifiers, channels, messages and the create-message body.
//!
//! Every type keeps Discord's wire spelling (snowflakes are JSON strings, `type` is an integer)
//! and models only the fields this slice reads. Unknown fields are ignored, so a new Discord
//! field never breaks decoding.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// A value refused before any request is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputError {
    /// The input that was refused (`content`, `nonce`, `limit`, a snowflake).
    pub field: &'static str,
    /// Why, as static text (never the refused value).
    pub problem: &'static str,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.field, self.problem)
    }
}

impl std::error::Error for InputError {}

macro_rules! snowflake {
    ($(#[$doc:meta])* $name:ident, $field:literal) => {
        $(#[$doc])*
        ///
        /// A Discord snowflake: a 64-bit unsigned integer, sent and received as a decimal JSON
        /// string ("always returned as strings in the HTTP API"). A JSON number is refused.
        /// Ordering is numeric, which for snowflakes is creation order.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            /// The identifier with this numeric value.
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// The numeric value.
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl FromStr for $name {
            type Err = InputError;

            /// Unsigned decimal digits only: no sign, no whitespace, no `0x`.
            fn from_str(text: &str) -> Result<Self, InputError> {
                parse_snowflake(text, $field).map(Self)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserializer.deserialize_str(SnowflakeVisitor($field)).map(Self)
            }
        }
    };
}

snowflake!(
    /// The ID of a guild (`guild.id`), the top-level resource of `/guilds/{guild.id}/..` routes.
    GuildId,
    "guild id"
);
snowflake!(
    /// The ID of a channel (`channel.id`), the top-level resource of `/channels/{channel.id}/..`
    /// routes.
    ChannelId,
    "channel id"
);
snowflake!(
    /// The ID of a message (`message.id`).
    MessageId,
    "message id"
);
snowflake!(
    /// The ID of a user (`user.id`).
    UserId,
    "user id"
);

fn parse_snowflake(text: &str, field: &'static str) -> Result<u64, InputError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(InputError {
            field,
            problem: "is not unsigned decimal digits",
        });
    }
    text.parse().map_err(|_| InputError {
        field,
        problem: "does not fit in 64 bits",
    })
}

struct SnowflakeVisitor(&'static str);

impl Visitor<'_> for SnowflakeVisitor {
    type Value = u64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a {} as a decimal string", self.0)
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<u64, E> {
        parse_snowflake(text, self.0).map_err(|e| E::custom(e.problem))
    }
}

/// A channel `type` (an integer on the wire). Unknown values are kept, not refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ChannelType(pub u16);

impl ChannelType {
    /// `GUILD_TEXT`: a text channel within a guild.
    pub const GUILD_TEXT: Self = Self(0);
    /// `DM`: a direct message.
    pub const DM: Self = Self(1);
    /// `GUILD_VOICE`: a voice channel within a guild.
    pub const GUILD_VOICE: Self = Self(2);
    /// `GROUP_DM`: a direct message between multiple users.
    pub const GROUP_DM: Self = Self(3);
    /// `GUILD_CATEGORY`: an organizational category.
    pub const GUILD_CATEGORY: Self = Self(4);
    /// `GUILD_ANNOUNCEMENT`: a channel users can follow and crosspost.
    pub const GUILD_ANNOUNCEMENT: Self = Self(5);
    /// `GUILD_STAGE_VOICE`: a voice channel for hosting events.
    pub const GUILD_STAGE_VOICE: Self = Self(13);
    /// `GUILD_FORUM`: a channel that can only contain threads.
    pub const GUILD_FORUM: Self = Self(15);

    /// Whether messages can be listed and created directly in a channel of this type within a
    /// guild (`GUILD_TEXT` or `GUILD_ANNOUNCEMENT`). Permissions are a separate question.
    pub fn is_guild_text(self) -> bool {
        self == Self::GUILD_TEXT || self == Self::GUILD_ANNOUNCEMENT
    }
}

/// A guild channel as returned by `GET /guilds/{guild.id}/channels` (threads are not included).
#[derive(Clone, Debug, Deserialize)]
pub struct Channel {
    /// The channel's ID.
    pub id: ChannelId,
    /// The channel type.
    #[serde(rename = "type")]
    pub kind: ChannelType,
    /// The guild, when the channel belongs to one.
    pub guild_id: Option<GuildId>,
    /// Sorting position.
    pub position: Option<i64>,
    /// The channel name (`name?: ?string`: absent and null are both `None`).
    pub name: Option<String>,
    /// The channel topic (absent and null are both `None`).
    pub topic: Option<String>,
    /// Whether the channel is age-restricted.
    pub nsfw: Option<bool>,
    /// The parent category (absent and null are both `None`).
    pub parent_id: Option<ChannelId>,
    /// The latest message ID; it may not point to an existing message.
    pub last_message_id: Option<MessageId>,
    /// Slowmode delay in seconds.
    pub rate_limit_per_user: Option<u32>,
}

/// The author of a message (the fields this slice reads).
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    /// The user's ID.
    pub id: UserId,
    /// The user's username.
    pub username: String,
    /// The legacy discriminator, when sent.
    pub discriminator: Option<String>,
    /// The display name, when set.
    pub global_name: Option<String>,
    /// Whether the user is a bot application.
    pub bot: Option<bool>,
}

/// A message (the fields this slice reads; reactions, attachments, embeds and components are
/// outside the slice and ignored).
#[derive(Clone, Debug, Deserialize)]
pub struct Message {
    /// The message's ID.
    pub id: MessageId,
    /// The channel it was sent in.
    pub channel_id: ChannelId,
    /// The author.
    pub author: User,
    /// The text; empty for an app without the `MESSAGE_CONTENT` intent.
    pub content: String,
    /// When it was sent (ISO8601 text, as sent).
    pub timestamp: String,
    /// When it was last edited (ISO8601 text); `None` if never edited.
    pub edited_timestamp: Option<String>,
    /// Whether it was a TTS message.
    pub tts: bool,
    /// Whether it mentions everyone.
    pub mention_everyone: bool,
    /// Whether it is pinned.
    pub pinned: bool,
    /// The message type (an integer; `0` is `DEFAULT`).
    #[serde(rename = "type")]
    pub kind: u16,
    /// The nonce the creator supplied, when Discord returns it.
    pub nonce: Option<Nonce>,
    /// The message flags bitfield.
    pub flags: Option<u64>,
}

/// A creation nonce: up to 25 characters, used to recognize a message after the fact and, with
/// `enforce_nonce`, to make Discord return the existing message instead of creating a second
/// one "in the past few minutes".
///
/// Discord accepts an integer or a string; this SDK always sends a string and accepts either
/// in responses (an integer is kept as its decimal text).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Nonce(String);

impl Nonce {
    /// The longest nonce Discord accepts, in characters.
    pub const MAX_CHARS: usize = 25;

    /// A nonce of 1 to 25 characters.
    ///
    /// # Errors
    /// [`InputError`] for an empty or longer value.
    pub fn new(value: impl Into<String>) -> Result<Self, InputError> {
        let value = value.into();
        let chars = value.chars().count();
        if chars == 0 || chars > Self::MAX_CHARS {
            return Err(InputError {
                field: "nonce",
                problem: "must be 1 to 25 characters",
            });
        }
        Ok(Self(value))
    }

    /// The nonce text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Nonce {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Text(String),
            Signed(i64),
            Unsigned(u64),
        }
        Ok(Self(match Wire::deserialize(deserializer)? {
            Wire::Text(text) => text,
            Wire::Signed(n) => n.to_string(),
            Wire::Unsigned(n) => n.to_string(),
        }))
    }
}

/// The `limit` of `GET /channels/{channel.id}/messages`: 1 to 100 (Discord's default is 50).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MessageLimit(u8);

impl MessageLimit {
    /// Discord's default page size.
    pub const DEFAULT: Self = Self(50);
    /// The largest page Discord returns.
    pub const MAX: Self = Self(100);

    /// A limit from 1 to 100.
    ///
    /// # Errors
    /// [`InputError`] for 0 or a value above 100.
    pub fn new(value: u8) -> Result<Self, InputError> {
        if (1..=100).contains(&value) {
            Ok(Self(value))
        } else {
            Err(InputError {
                field: "limit",
                problem: "must be 1 to 100",
            })
        }
    }

    /// The numeric limit.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for MessageLimit {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for MessageLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Where a message listing starts. Discord accepts at most one of `around`, `before` and
/// `after` per request; [`DiscordChannelMessages::anchor`](crate::DiscordChannelMessages::anchor)
/// sets exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// Messages around this message ID.
    Around(MessageId),
    /// Messages before (older than) this message ID.
    Before(MessageId),
    /// Messages after (newer than) this message ID.
    After(MessageId),
}

impl concord::QueryPairs for Anchor {
    fn query_pairs(&self, out: &mut concord::QueryPairsWriter<'_>) {
        match self {
            Self::Around(id) => out.pair("around", id),
            Self::Before(id) => out.pair("before", id),
            Self::After(id) => out.pair("after", id),
        }
    }
}

impl concord::QueryPairs for MessageLimit {
    fn query_pairs(&self, out: &mut concord::QueryPairsWriter<'_>) {
        out.pair("limit", self);
    }
}

/// The JSON body of `POST /channels/{channel.id}/messages`, limited to this slice: text
/// `content`, an optional `nonce` (with `enforce_nonce`) and `tts`. Embeds, components,
/// stickers, files, polls and replies are outside the slice.
///
/// `Debug` prints the content's length, never its text.
#[derive(Clone, Serialize)]
pub struct CreateMessage {
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    nonce: Option<Nonce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enforce_nonce: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tts: Option<bool>,
}

impl CreateMessage {
    /// Discord's content limit, in characters.
    pub const MAX_CONTENT_CHARS: usize = 2000;

    /// A text message of 1 to 2000 characters (counted as Unicode scalar values).
    ///
    /// # Errors
    /// [`InputError`] for empty content (Discord refuses it with code 50006) or content longer
    /// than 2000 characters.
    pub fn text(content: impl Into<String>) -> Result<Self, InputError> {
        let content = content.into();
        let chars = content.chars().count();
        if chars == 0 || chars > Self::MAX_CONTENT_CHARS {
            return Err(InputError {
                field: "content",
                problem: "must be 1 to 2000 characters",
            });
        }
        Ok(Self {
            content,
            nonce: None,
            enforce_nonce: None,
            tts: None,
        })
    }

    /// Sends `nonce` with `enforce_nonce: true`: if the same author created a message with this
    /// nonce in the past few minutes, Discord returns that message instead of creating another.
    /// This is what makes a deliberate resend after an ambiguous outcome safe within that window.
    pub fn with_nonce(mut self, nonce: Nonce) -> Self {
        self.nonce = Some(nonce);
        self.enforce_nonce = Some(true);
        self
    }

    /// Sends `tts` (needs `SEND_TTS_MESSAGES` when true).
    pub fn with_tts(mut self, tts: bool) -> Self {
        self.tts = Some(tts);
        self
    }

    /// The content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// The nonce, if one is sent.
    pub fn nonce(&self) -> Option<&Nonce> {
        self.nonce.as_ref()
    }
}

impl fmt::Debug for CreateMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateMessage")
            .field("content_chars", &self.content.chars().count())
            .field("nonce", &self.nonce.is_some())
            .field("tts", &self.tts)
            .finish()
    }
}
