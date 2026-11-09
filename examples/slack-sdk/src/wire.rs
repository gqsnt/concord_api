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
        error crate::wire::Envelope<serde_json::Map<String, serde_json::Value>> in concord::IgnoreContentType<concord::Json>;
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;
        limit crate::SUCCESS_BODY_LIMIT;

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

#[cfg(test)]
mod tests {
    use super::*;
    use concord_example_testserver::{Reply, TestServer};

    #[tokio::test]
    async fn native_send_keeps_the_declared_success_body_limit() {
        let server =
            TestServer::start(|_| Reply::json(200, vec![b' '; crate::SUCCESS_BODY_LIMIT + 1]))
                .await;
        let api = SlackApi::new(
            concord::reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap(),
            SlackApiConfig {
                token: "synthetic-token".into(),
            },
        )
        .with_base_url(server.url())
        .unwrap();
        let error = api
            .users_list(1u16)
            .prepare()
            .unwrap()
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.kind(), concord::ErrorKind::Limit);
        assert_eq!(error.head().unwrap().status().as_u16(), 200);
        server.shutdown().await;
    }
}
