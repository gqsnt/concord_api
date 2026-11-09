#![doc = include_str!("../README.md")]

mod auth;
mod error;
mod models;

pub use auth::{AuthError, BasicAuth};
pub use error::{
    Error, ErrorDocumentProblem, MAX_ERROR_BODY, ProviderError, Response, ResponseMeta, TwilioError,
};
pub use models::{CreateMessage, Message, MessageStatus};

use concord::{Endpoint, Prepared, reqwest};

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
        error crate::ProviderError in concord::Json;
            path "2010-04-01/Accounts/{account_sid}";
            headers { AUTHORIZATION = authorization; }
            response Json;

            /// Creates a single text message.
            POST create_message(body message: CreateMessage) -> Message {
                path "Messages.json";
                request Form;
            }

            /// Fetches a message's current status and error fields.
            ///
            /// The message SID is an identifier, not a secret: unlike the account SID it is not
            /// marked sensitive, so request `Debug` shows it, as `Message`'s `Debug` does.
            GET get_message(sid: String) -> Message {
                path "Messages/{sid}.json";
            }
        }
    }
}

/// Sends `authorization: Basic <base64(username:password)>` from the supplied `BasicAuth`.
///
/// Configured client for this Messaging slice; cloning preserves its account and auth context.
///
/// The caller supplies the transport (timeouts, redirect policy, proxy and TLS). Use
/// [`Self::with_base_url`] to direct all requests to the offline fixture. Each operation
/// executes once, without retries or polling, and returns typed errors plus response metadata.
///
/// # Names in diagnostics
///
/// The operations are declared on a generated client named `TwilioApi`, held inside this type
/// (it is not exported: its configuration takes a raw header, while [`Twilio::new`] takes
/// validated [`BasicAuth`]). That is the name diagnostics use: [`concord::Operation`] names
/// `TwilioApi.create_message` and `TwilioApi.get_message` (for example in
/// [`ResponseMeta::operation`] and [`TwilioError::operation`]), errors' `Display` starts with
/// `POST TwilioApi.create_message:` or `GET TwilioApi.get_message:`, and `Debug` shows
/// `Twilio { api: TwilioApi { base: .., authorization: <redacted>, account_sid: <redacted> } }`.
#[derive(Clone, Debug)]
pub struct Twilio {
    api: wire::TwilioApi,
}

impl Twilio {
    /// Configures the path account separately from Basic credentials.
    ///
    /// The base defaults to `https://api.twilio.com`; no request occurs here. This fixture SDK
    /// does not validate SID syntax (it only refuses an empty message SID). Concord encodes every
    /// supplied identifier as one segment.
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
    ///
    /// # Errors
    ///
    /// Concord's own `concord::Error` (kind `Build`, the URL is not shown) for a base that is not
    /// an absolute http(s) URL or carries user info, a query or a fragment. This is the only
    /// method that returns it; `?` converts it into [`TwilioError::Concord`].
    pub fn with_base_url(mut self, base: impl AsRef<str>) -> Result<Self, concord::Error> {
        self.api = self.api.with_base_url(base)?;
        Ok(self)
    }

    /// Submits one message and returns the complete decoded resource and response metadata.
    ///
    /// Success means the HTTP resource was decoded, not that an SMS was delivered. On a
    /// transport or decode failure the write may already have happened; do not blindly retry.
    pub async fn create_message(
        &self,
        message: CreateMessage,
    ) -> Result<Response<Message>, TwilioError> {
        error::send(self.prepare_create_message(message)?).await
    }

    /// Retrieves one message. Processing failure is data in the returned [`Message`].
    ///
    /// An empty `sid` is [`TwilioError::EmptySid`], before anything is sent.
    pub async fn get_message(
        &self,
        sid: impl Into<String>,
    ) -> Result<Response<Message>, TwilioError> {
        error::send(self.prepare_get_message(sid)?).await
    }

    /// Builds a create request without I/O, for inspection or Concord's native escape.
    ///
    /// Calling `Prepared::send` directly uses Concord errors without this SDK's provider
    /// decoding or metadata envelope. Prefer [`Self::create_message`] for normal use. A request
    /// that cannot be built is [`TwilioError::Concord`] (kind `Build`, no metadata).
    pub fn prepare_create_message(
        &self,
        message: CreateMessage,
    ) -> Result<Prepared<Message, crate::ProviderError>, TwilioError> {
        Ok(self.api.create_message(message).prepare()?)
    }

    /// Builds a fetch request without I/O. See [`Self::prepare_create_message`] for limitations.
    /// An empty `sid` is [`TwilioError::EmptySid`].
    pub fn prepare_get_message(
        &self,
        sid: impl Into<String>,
    ) -> Result<Prepared<Message, crate::ProviderError>, TwilioError> {
        let sid = sid.into();
        if sid.is_empty() {
            return Err(
                TwilioError::EmptySid {
                    operation: <wire::TwilioApiGetMessage as Endpoint<
                        Message,
                        crate::ProviderError,
                    >>::operation(),
                },
            );
        }
        Ok(self.api.get_message(sid).prepare()?)
    }
}
