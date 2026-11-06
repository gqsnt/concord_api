#![doc = include_str!("../README.md")]

mod auth;
mod error;
mod models;

pub use auth::{AuthError, BasicAuth};
pub use error::{
    Error, ErrorDocumentProblem, MAX_ERROR_BODY, ProviderError, Response, ResponseMeta,
};
pub use models::{CreateMessage, Message, MessageStatus};

use concord::{Prepared, reqwest};

mod wire {
    use concord::reqwest::header::{AUTHORIZATION, HeaderValue};
    use concord::{Form, Json};

    use crate::{CreateMessage, Message};

    concord::api! {
        /// The two declared Twilio Messaging operations.
        pub client TwilioApi(
            #[sensitive] authorization: HeaderValue,
            #[sensitive] account_sid: String,
        ) {
            base "https://api.twilio.com";
            path "2010-04-01/Accounts/{account_sid}";
            headers { AUTHORIZATION = authorization; }
            response Json;

            /// Creates a single text message.
            POST create_message(body message: CreateMessage) -> Message {
                path "Messages.json";
                request Form;
            }

            /// Fetches a message's current status and error fields.
            GET get_message(#[sensitive] sid: String) -> Message {
                path "Messages/{sid}.json";
            }
        }
    }
}

/// Configured client for this Messaging slice; cloning preserves its account and auth context.
///
/// The caller supplies the transport (timeouts, redirect policy, proxy and TLS). Use
/// [`Self::with_base_url`] to direct all requests to the offline fixture. Each operation
/// executes once, without retries or polling, and returns typed errors plus response metadata.
#[derive(Clone, Debug)]
pub struct Twilio {
    api: wire::TwilioApi,
}

impl Twilio {
    /// Configures the path account separately from Basic credentials.
    ///
    /// The base defaults to `https://api.twilio.com`; no request occurs here. This fixture SDK
    /// does not validate SID syntax. Concord encodes every supplied identifier as one segment.
    pub fn new(http: reqwest::Client, account_sid: impl Into<String>, auth: BasicAuth) -> Self {
        Self {
            api: wire::TwilioApi::new(
                http,
                wire::TwilioApiConfig {
                    authorization: auth.into_header(),
                    account_sid: account_sid.into(),
                },
            ),
        }
    }

    /// Replaces the API origin/base prefix, keeping the declared `2010-04-01/Accounts` path.
    ///
    /// Use the loopback test server's URL, without an API version suffix. The caller is
    /// responsible for trusting this destination: configured credentials will be sent there.
    pub fn with_base_url(mut self, base: impl AsRef<str>) -> Result<Self, concord::Error> {
        self.api = self.api.with_base_url(base)?;
        Ok(self)
    }

    /// Submits one message and returns the complete decoded resource and response metadata.
    ///
    /// Success means the HTTP resource was decoded, not that an SMS was delivered. On a
    /// transport or decode failure the write may already have happened; do not blindly retry.
    pub async fn create_message(&self, message: CreateMessage) -> Result<Response<Message>, Error> {
        error::send(self.prepare_create_message(message)?).await
    }

    /// Retrieves one message. Processing failure is data in the returned [`Message`].
    pub async fn get_message(&self, sid: impl Into<String>) -> Result<Response<Message>, Error> {
        error::send(self.prepare_get_message(sid)?).await
    }

    /// Builds a create request without I/O, for inspection or Concord's native escape.
    ///
    /// Calling `Prepared::send` directly uses Concord errors without this SDK's provider
    /// decoding or metadata envelope. Prefer [`Self::create_message`] for normal use.
    pub fn prepare_create_message(
        &self,
        message: CreateMessage,
    ) -> Result<Prepared<Message>, concord::Error> {
        self.api.create_message(message).prepare()
    }

    /// Builds a fetch request without I/O. See [`Self::prepare_create_message`] for limitations.
    pub fn prepare_get_message(
        &self,
        sid: impl Into<String>,
    ) -> Result<Prepared<Message>, concord::Error> {
        self.api.get_message(sid.into()).prepare()
    }
}
