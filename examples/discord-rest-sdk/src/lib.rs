//! A bounded Discord REST (v10) slice built with Concord: list a guild's channels, page a
//! channel's messages, fetch one message, create a text message.
//!
//! The public client [`Discord`] wraps the declared client: navigate `discord.guild(id)` /
//! `discord.channel(id)`, call an operation, and `send()` it. Every `send()` goes through the
//! client's send policy, the [`RateLimiter`] (Discord's per-route buckets keyed with the channel
//! or guild, and the global limit), and returns `Result<T, `[`Error`]`>`.
//!
//! ```no_run
//! use concord_sdk_discord_rest::{
//!     ChannelId, CreateMessage, Discord, DiscordConfig, MessageLimit, Nonce, RateLimiter,
//! };
//!
//! # async fn run(token: String) -> Result<(), Box<dyn std::error::Error>> {
//! let http = concord_sdk_discord_rest::concord::reqwest::Client::builder()
//!     .timeout(std::time::Duration::from_secs(10))
//!     .build()?;
//! let discord = Discord::new(http, DiscordConfig { token, limiter: RateLimiter::new() });
//!
//! let channel = discord.channel(ChannelId::new(41771983423143937));
//! let newest = channel.messages().set_limit(MessageLimit::new(10)?).send().await?;
//! let created = channel
//!     .create_message(CreateMessage::text("hello")?.with_nonce(Nonce::new("job-7-1")?))
//!     .send()
//!     .await?;
//! let again = channel.message(created.id).send().await?;
//! # let _ = (newest, again);
//! # Ok(()) }
//! ```
//!
//! The README states the slice, the failure semantics and what is excluded.

mod api;
mod error;
mod limiter;
mod model;

pub use concord;
pub use concord_pagination as pagination;

pub use api::{
    DEFAULT_USER_AGENT, Discord, DiscordChannel, DiscordChannelCreateMessage,
    DiscordChannelMessage, DiscordChannelMessages, DiscordConfig, DiscordGuild,
    DiscordGuildChannels, MessageHistory,
};
pub use error::{
    AmbiguousWrite, ApiError, Error, ErrorClass, ErrorDocument, GateScope, Gated, JsonErrorCode,
    ProtocolError, ProtocolProblem, RateLimitBody, RateLimitScope, RateLimited, Resource,
};
pub use limiter::{BucketHash, RateLimiter, Route};
pub use model::{
    Anchor, Channel, ChannelId, ChannelType, CreateMessage, GuildId, InputError, Message,
    MessageId, MessageLimit, Nonce, User, UserId,
};

/// The most bytes of a 2xx body this SDK reads (4 MiB): an SDK bound against a body that never
/// ends, not a Discord-documented size (a page of 100 messages is far smaller). A longer body is
/// `ErrorKind::Limit` (for a `POST`, inside [`Error::AmbiguousWrite`]).
pub const SUCCESS_BODY_LIMIT: usize = 4 * 1024 * 1024;

/// The most bytes of an error document (non-2xx, 429 included) this SDK reads (64 KiB); a longer
/// one keeps its status and is reported as an `ErrorKind::Limit` document error.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// Compile the README's examples.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
