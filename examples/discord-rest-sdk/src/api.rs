//! Discord declaration and public handles.

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

concord::api! {
    /// Discord's REST API (v10) for one bot: guild channels, channel messages, message creation.
    ///
    /// Construct with `Discord::new(http, DiscordConfig { token, limiter })`. Transport settings
    /// (timeouts, proxies, TLS, redirects) belong to the `reqwest::Client` you pass in; turn redirects
    /// and retries off, or a write's status can describe a later request.
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
        limit crate::SUCCESS_BODY_LIMIT;
        send limiter -> Result<_, Error>;

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
            /// `READ_MESSAGE_HISTORY` no messages are returned; voice channels also require
            /// `CONNECT`. Discord accepts at most one of
            /// `around`, `before` and `after`: set one with [`anchor`](crate::DiscordChannelMessages::anchor).
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
            GET messages(
                /// The single message cursor.
                query(..) anchor?: Anchor,
                /// Maximum number of messages (1–100), after the cursor on the wire.
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

impl DiscordChannelMessages {
    /// Sets the message cursor, replacing any previous cursor.
    pub fn anchor(self, anchor: Anchor) -> Self {
        self.set_anchor(anchor)
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
