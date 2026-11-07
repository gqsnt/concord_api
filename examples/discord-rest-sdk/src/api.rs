//! Discord declaration and public handles; wrappers protect the exclusive cursor invariant.

use concord::reqwest::header::{AUTHORIZATION, USER_AGENT};
use concord::{Endpoint, Json};
use concord_pagination::{Page, Pages, Resume};

use crate::error::{Error, ProtocolError, ProtocolProblem, Resource};
use crate::limiter::{RateLimiter, Route};
use crate::model::{
    Anchor, Channel, ChannelId, CreateMessage, GuildId, Message, MessageId, MessageLimit,
};

/// The `User-Agent` this SDK sends unless replaced (`DiscordBot ($url, $versionNumber)`).
pub const DEFAULT_USER_AGENT: &str = concat!(
    "DiscordBot (https://github.com/gqsnt/concord_api, ",
    env!("CARGO_PKG_VERSION"),
    ")"
);

fn default_user_agent() -> String {
    DEFAULT_USER_AGENT.to_owned()
}

mod raw {
    use super::*;

    concord::api! {
        /// Discord's REST API (v10) for one bot: guild channels, channel messages, message creation.
        ///
        /// Construct with `Discord::new(http, DiscordConfig { token, limiter })`. Transport settings
        /// (timeouts, proxies, TLS, redirects) belong to the `reqwest::Client` you pass in.
        ///
        /// Every operation's `send()` returns `Result<T, `[`Error`]`>` through the client's send
        /// policy, the [`RateLimiter`]: it refuses a request locally while its rate-limit scope is
        /// exhausted, executes it once, records Discord's rate-limit headers, and maps statuses to
        /// typed errors. `with_base_url` and `prepare()` return [`concord::Error`] (nothing is sent);
        /// this crate's [`Error`] implements `From<concord::Error>`, so `?` converts.
        pub client Discord(
            /// The bot token, without the `Bot ` prefix. Sent as `Authorization: Bot {token}`;
            /// never printed by Concord.
            #[sensitive] token: String,
            /// The bot's rate state and the send policy of every operation (one per bot token).
            limiter: RateLimiter,
            /// `User-Agent`, which Discord requires as `DiscordBot ($url, $versionNumber)`.
            header(USER_AGENT) user_agent: String = default_user_agent(),
        ) {
            base "https://discord.com/api/v10";
            headers { AUTHORIZATION = "Bot {token}"; }
            format Json;
            send limiter;

            /// One guild: `/guilds/{guild.id}` (the route's top-level resource).
            scope guild(guild_id: GuildId) {
                path "guilds/{guild_id}";

                /// `GET /guilds/{guild.id}/channels`: the guild's channels, threads excluded.
                /// Discord documents that from 2026-11-16 channels the bot cannot view
                /// (`VIEW_CHANNEL`) are omitted.
                GET channels() -> Vec<Channel> { path "channels"; }
            }

            /// One channel: `/channels/{channel.id}` (the route's top-level resource).
            scope channel(channel_id: ChannelId) {
                path "channels/{channel_id}";

                /// `GET /channels/{channel.id}/messages`: up to `limit` (1–100, Discord's default
                /// 50) messages, newest to oldest. Needs `VIEW_CHANNEL`; without
                /// `READ_MESSAGE_HISTORY` no messages are returned. Discord accepts at most one of
                /// `around`, `before` and `after`: set one with [`anchor`](crate::DiscordChannelMessages::anchor).
                GET messages(
                    /// Messages around this message ID.
                    query around?: MessageId,
                    /// Messages before this message ID.
                    query before?: MessageId,
                    /// Messages after this message ID.
                    query after?: MessageId,
                    /// Maximum number of messages (1–100).
                    query limit?: MessageLimit,
                ) -> Vec<Message> { path "messages"; }

                /// `GET /channels/{channel.id}/messages/{message.id}`: one message. Needs
                /// `VIEW_CHANNEL` and `READ_MESSAGE_HISTORY`.
                GET message(message_id: MessageId) -> Message { path "messages/{message_id}"; }

                /// `POST /channels/{channel.id}/messages`: create a text message (needs
                /// `SEND_MESSAGES`). A failure after the request may have been sent is
                /// [`Error::AmbiguousWrite`](crate::Error::AmbiguousWrite): never resent.
                POST create_message(body message: CreateMessage) -> Message { path "messages"; }
            }
        }
    }
}

pub use raw::{
    DiscordChannelCreateMessage, DiscordChannelMessage, DiscordConfig, DiscordGuild,
    DiscordGuildChannels,
};

/// The Discord client. Its channel handle exposes a single message cursor at a time.
#[derive(Clone, Debug)]
pub struct Discord(raw::Discord);

impl Discord {
    /// Construct a client using the supplied transport and bot configuration.
    pub fn new(http: concord::reqwest::Client, config: DiscordConfig) -> Self {
        Self(raw::Discord::new(http, config))
    }
    /// Replace the base URL, for example to use a local fixture.
    pub fn with_base_url(self, url: impl AsRef<str>) -> Result<Self, concord::Error> {
        self.0.with_base_url(url).map(Self)
    }
    /// Replace the bot token.
    pub fn set_token(self, token: impl concord::IntoValue<String>) -> Self {
        Self(self.0.set_token(token))
    }
    /// Replace the rate limiter.
    pub fn set_limiter(self, limiter: impl concord::IntoValue<RateLimiter>) -> Self {
        Self(self.0.set_limiter(limiter))
    }
    /// Replace the user agent.
    pub fn set_user_agent(self, user_agent: impl concord::IntoValue<String>) -> Self {
        Self(self.0.set_user_agent(user_agent))
    }
    /// Select a guild.
    pub fn guild(&self, id: impl concord::IntoValue<GuildId>) -> DiscordGuild {
        self.0.guild(id)
    }
    /// Select a channel.
    pub fn channel(&self, id: impl concord::IntoValue<ChannelId>) -> DiscordChannel {
        DiscordChannel(self.0.channel(id))
    }
}

/// One channel in the Discord API.
#[derive(Clone, Debug)]
pub struct DiscordChannel(raw::DiscordChannel);

impl DiscordChannel {
    /// Select a different channel.
    pub fn set_channel_id(self, id: impl concord::IntoValue<ChannelId>) -> Self {
        Self(self.0.set_channel_id(id))
    }
    /// List channel messages newest first. Set one cursor with [`DiscordChannelMessages::anchor`].
    /// Guild reads require `VIEW_CHANNEL` and `READ_MESSAGE_HISTORY`; voice channels also require
    /// `CONNECT`. Without `READ_MESSAGE_HISTORY`, Discord returns no messages.
    pub fn messages(&self) -> DiscordChannelMessages {
        DiscordChannelMessages(self.0.messages())
    }
    /// Fetch one message.
    pub fn message(&self, id: impl concord::IntoValue<MessageId>) -> DiscordChannelMessage {
        self.0.message(id)
    }
    /// Create a message.
    pub fn create_message(
        &self,
        message: impl concord::IntoValue<CreateMessage>,
    ) -> DiscordChannelCreateMessage {
        self.0.create_message(message)
    }
}

/// A message-listing request with at most one cursor.
/// See [Discord Get Channel Messages](https://docs.discord.com/developers/resources/message#get-channel-messages).
///
/// Individual cursor setters are deliberately unavailable:
/// ```compile_fail
/// # use concord_sdk_discord_rest::{DiscordChannelMessages, MessageId};
/// # fn invalid(request: DiscordChannelMessages, id: MessageId) {
/// request.set_before(id);
/// # }
/// ```
/// ```compile_fail
/// # use concord_sdk_discord_rest::{DiscordChannelMessages, MessageId};
/// # fn invalid(request: DiscordChannelMessages, id: MessageId) {
/// request.set_after(id);
/// # }
/// ```
/// ```compile_fail
/// # use concord_sdk_discord_rest::{DiscordChannelMessages, MessageId};
/// # fn invalid(request: DiscordChannelMessages, id: MessageId) {
/// request.set_around(id);
/// # }
/// ```
#[derive(Clone, Debug)]
#[must_use = "a request does nothing until send or prepare is called"]
pub struct DiscordChannelMessages(raw::DiscordChannelMessages);

impl DiscordChannelMessages {
    /// Set the maximum number of messages.
    pub fn set_limit(self, limit: impl concord::IntoValue<MessageLimit>) -> Self {
        Self(self.0.set_limit(limit))
    }
    /// Set or clear the maximum number of messages.
    pub fn maybe_limit(self, limit: Option<MessageLimit>) -> Self {
        Self(self.0.maybe_limit(limit))
    }
    /// Prepare the request without sending it.
    pub fn prepare(self) -> Result<concord::Prepared<Vec<Message>>, concord::Error> {
        self.0.prepare()
    }
    /// Send the request through the client's rate limiter.
    pub async fn send(self) -> Result<Vec<Message>, Error> {
        self.0.send().await
    }
}

impl Endpoint<Vec<Message>> for DiscordChannelMessages {
    fn prepare(self) -> Result<concord::Prepared<Vec<Message>>, concord::Error> {
        self.prepare()
    }
    fn operation() -> concord::Operation {
        <raw::DiscordChannelMessages as Endpoint<Vec<Message>>>::operation()
    }
}

impl concord::Sends<Result<Vec<Message>, Error>> for DiscordChannelMessages {
    async fn send(self) -> Result<Vec<Message>, Error> {
        self.send().await
    }
}

impl DiscordChannelMessages {
    /// Set or clear the cursor.
    pub fn maybe_anchor(self, anchor: Option<Anchor>) -> Self {
        match anchor {
            Some(anchor) => self.anchor(anchor),
            None => Self(
                self.0
                    .maybe_around(None)
                    .maybe_before(None)
                    .maybe_after(None),
            ),
        }
    }

    /// Sets exactly one of `around`, `before` and `after` (clearing the other two).
    pub fn anchor(self, anchor: Anchor) -> Self {
        let (around, before, after) = match anchor {
            Anchor::Around(id) => (Some(id), None, None),
            Anchor::Before(id) => (None, Some(id), None),
            Anchor::After(id) => (None, None, Some(id)),
        };
        Self(
            self.0
                .maybe_around(around)
                .maybe_before(before)
                .maybe_after(after),
        )
    }
}

/// A lazy walk of a channel's history, newest to oldest, one `GET .../messages` per page. See
/// [`DiscordChannel::history`].
pub type MessageHistory = Pages<Message, MessageId, (), Error>;

impl DiscordChannel {
    /// The channel's history from the newest message, in pages of `limit`.
    ///
    /// Each page is `GET /channels/{channel.id}/messages?before={oldest id of the previous
    /// page}&limit={limit}` through the client's `send()` (rate admission included). The walk
    /// ends after a page shorter than `limit` (an empty one included). A page out of order, or
    /// longer than `limit`, is [`Error::Protocol`]. A failed page ends the walk with
    /// [`PagesError::Provider`](concord_pagination::PagesError::Provider): its `failed`
    /// position resumes it with [`history_from`](Self::history_from) (after a
    /// [`Error::Gated`] delay, for example). Nothing is retried.
    pub fn history(&self, limit: MessageLimit) -> MessageHistory {
        self.history_from(Resume::Start, limit)
    }

    /// [`history`](Self::history) from a position: `Resume::At(id)` lists messages before `id`.
    pub fn history_from(&self, from: Resume<MessageId>, limit: MessageLimit) -> MessageHistory {
        let channel = self.clone();
        Pages::new(from, move |before: Option<MessageId>| {
            let request = channel
                .messages()
                .maybe_anchor(before.map(Anchor::Before))
                .set_limit(limit);
            async move {
                let messages = request.send().await?;
                history_page(messages, before, limit)
            }
        })
    }
}

fn history_page(
    messages: Vec<Message>,
    before: Option<MessageId>,
    limit: MessageLimit,
) -> Result<Page<Message, MessageId>, Error> {
    let route = |problem| {
        let resource = messages.first().map(|m| Resource::Channel(m.channel_id));
        let operation = <DiscordChannelMessages as Endpoint<Vec<Message>>>::operation();
        Error::Protocol(Box::new(ProtocolError {
            route: Route::new(operation, resource),
            problem,
        }))
    };
    if messages.len() > usize::from(limit.get()) {
        return Err(route(ProtocolProblem::OversizedPage));
    }
    let mut bound = before;
    for message in &messages {
        if bound.is_some_and(|bound| message.id >= bound) {
            return Err(route(ProtocolProblem::UnorderedPage));
        }
        bound = Some(message.id);
    }
    let next = if messages.len() < usize::from(limit.get()) {
        None
    } else {
        bound
    };
    Ok(Page::new(messages, next))
}
