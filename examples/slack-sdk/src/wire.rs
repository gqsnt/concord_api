//! Private declaration: public callers always pass through Slack's envelope checks.

use concord::{Json, reqwest::header::AUTHORIZATION};
use serde::Deserialize;

use crate::{Conversation, ResponseMetadata, User};

#[derive(Debug, Deserialize)]
pub struct Envelope<T> {
    pub ok: bool,
    pub error: Option<String>,
    pub needed: Option<String>,
    pub provided: Option<String>,
    pub warning: Option<String>,
    pub response_metadata: Option<ResponseMetadata>,
    #[serde(flatten)]
    pub payload: T,
}

#[derive(Debug, Deserialize)]
pub struct UsersPayload {
    pub members: Option<Vec<User>>,
    pub cache_ts: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ConversationPayload {
    pub channel: Option<Conversation>,
}

concord::api! {
    /// Declared transport for this SDK. Use [`crate::Client`] for Slack-aware results.
    pub client SlackApi(
        /// Raw bearer token, without the `Bearer ` prefix; redacted by Concord.
        #[sensitive] token: String,
    ) {
        base "https://slack.com/api";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;

        GET users_list(
            query limit: u16,
            #[sensitive] query cursor?: String,
            query include_locale?: bool,
            query team_id?: String,
        ) -> Envelope<UsersPayload> { path "users.list"; }

        GET conversations_info(
            query channel: String,
            query include_locale?: bool,
            query include_num_members?: bool,
        ) -> Envelope<ConversationPayload> { path "conversations.info"; }
    }
}
