//! A bounded SDK for the Anthropic Messages API, built on Concord: create a message, or stream
//! one and consume its events through `message_stop`. Count the input tokens of an existing
//! request before choosing whether to generate.
//!
//! ```no_run
//! use concord_sdk_anthropic_messages::{Anthropic, AnthropicConfig, MessageRequest, http_client};
//!
//! async fn run() -> Result<(), Box<dyn std::error::Error>> {
//!     let api = Anthropic::new(http_client()?, AnthropicConfig { api_key: "sk-ant-...".into() });
//!     let request = MessageRequest::user("claude-opus-5-5", 256, "Hello")?;
//!
//!     let counted = api.messages().count_tokens(request.count_tokens()).send().await?;
//!     println!("estimated input tokens: {}", counted.value.input_tokens);
//!
//!     // Workflow 1: POST /v1/messages, the whole message.
//!     let created = api.messages().create(request.clone()).send().await?;
//!     println!("request id: {:?}", created.request_id());
//!     for block in &created.value.content {
//!         match block.as_text() {
//!             Some(text) => println!("text: {text}"),
//!             None => println!("{} block (not interpreted)", block.block_type()),
//!         }
//!     }
//!
//!     // Workflow 2: the same request with "stream": true.
//!     let mut stream = api.messages().stream(request.streaming()).send().await?.value;
//!     while let Some(event) = stream.next().await? {
//!         if let Some(text) = event.text_delta() {
//!             print!("{text}");
//!         }
//!     }
//!     let message = stream.final_message().await?; // `message_stop` arrived
//!     println!("\n{:?}, usage {:?}", message.stop_reason, message.usage);
//!     Ok(())
//! }
//! # let _ = run;
//! ```
//!
//! See the crate README for the failure semantics and what the slice excludes.

mod error;
mod model;
mod stream;

use concord::reqwest::{self, redirect};
use concord::{Endpoint, Json, ResponseMeta, SendWith};

pub use concord;
pub use error::{
    ApiError, ApiErrorType, Error, ErrorDocument, Incomplete, IncompleteCause, ProviderError,
};
pub use model::{
    ContentBlock, CountTokensRequest, InputMessage, InvalidRequest, Message, MessageRequest,
    OtherBlock, Role, StopReason, StreamRequest, TokenCount, Usage,
};
pub use stream::{
    Delta, MAX_EVENT_BYTES, MessageEvents, MessageStream, PartialMessage, StreamEvent, StreamState,
};

/// The `anthropic-version` sent by default: the value of every request on the cited pages
/// (retrieved 2026-11-07). Replace it with `Anthropic::set_anthropic_version`.
pub const API_VERSION: &str = "2026-10-21";

/// The largest successful JSON body read for creation or counting (8 MiB): a longer one is a
/// Concord `Limit` error and is not decoded.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

/// The largest error document read (64 KiB). The status is kept when the document is longer.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

concord::api! {
    /// The Anthropic Messages API: [`messages`](Anthropic::messages) counts input tokens, creates
    /// a message, or streams one. Every `send()` returns `Result<Response<T>, Error>`: this
    /// crate's error, with the API's error document on a non-2xx status ([`SendPolicy`]). Nothing
    /// is retried.
    ///
    /// Use a transport without redirects ([`http_client`]): the key travels in `x-api-key`, a
    /// header reqwest does not drop when a redirect changes the host.
    pub client Anthropic(
        /// The API key, sent as `x-api-key`; never printed by Concord.
        #[sensitive] header("x-api-key") mut api_key: String,
        /// The `anthropic-version` header ([`API_VERSION`] by default).
        header("anthropic-version") anthropic_version: String = API_VERSION,
    ) {
        base "https://api.anthropic.com";
        path "v1";
        format Json;
        error ErrorDocument in Json;
        send SendPolicy -> Result<Response<_>, Error>;

        /// The Messages endpoints (`/v1/messages`).
        scope messages {
            path "messages";

            /// Creates a message and returns it whole (`POST /v1/messages`).
            POST create(body request: MessageRequest) -> Message {
                limit MAX_MESSAGE_BYTES;
            }

            /// Counts input tokens without generating (`POST /v1/messages/count_tokens`). Use
            /// [`MessageRequest::count_tokens`] to reuse a request's inputs. The result has the
            /// same response metadata and API errors as creation; no fallback or retry occurs.
            #[read] POST count_tokens(body request: CountTokensRequest) -> TokenCount {
                path "count_tokens";
                limit MAX_MESSAGE_BYTES;
            }

            /// Creates a message as a stream of server-sent events (`POST /v1/messages` with
            /// `"stream": true`). `send().await` returns `Result<Response<MessageStream>, Error>`: on a 2xx
            /// it checks the `text/event-stream` head and returns the stream without reading its body;
            /// a non-2xx answer is [`Error::Api`] (its error document read, bounded); a build, transport or
            /// head-check failure before the stream opens is [`Error::Concord`]; errors in the stream are
            /// [`Error::Incomplete`].
            POST stream(body request: StreamRequest) -> stream StreamEvent {
                response MessageEvents;
            }
        }
    }
}

/// A decoded value (a [`TokenCount`], a [`Message`], a [`MessageStream`]) and the head of the
/// response it came from.
#[derive(Debug)]
pub struct Response<T> {
    /// The value.
    pub value: T,
    /// The response head: status, headers (unredacted through `headers()`; `Debug` prints names
    /// only), operation.
    pub meta: ResponseMeta,
}

impl<T> Response<T> {
    /// The `request-id` header ("Every API response includes a unique request-id header"), for
    /// support requests.
    pub fn request_id(&self) -> Option<&str> {
        error::request_id(self.meta.headers())
    }

    /// The value, without the head.
    pub fn into_value(self) -> T {
        self.value
    }
}

/// The send policy of [`Anthropic`] (`send SendPolicy -> Result<Response<_>, Error>;`): one
/// request, then a non-2xx answer becomes [`Error::Api`] with its error document read bounded
/// ([`ERROR_BODY_LIMIT`]), and a 2xx is decoded as declared (a count, a message, or a stream whose
/// body is not read yet).
///
/// It never repeats a request: a 429, 5xx or 529 is returned to the caller
/// ([`ApiError::retry_after`] reads the server's delay), and a broken stream is never restarted.
#[derive(Debug, Default)]
pub struct SendPolicy;

impl<T: Send> SendWith<T, ErrorDocument> for SendPolicy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T, ErrorDocument> + Send>(&self, endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare()?.error_body_limit(ERROR_BODY_LIMIT);
        let (value, meta) = prepared.receive().await?.decode_with_meta().await?;
        Ok(Response { value, meta })
    }
}

/// The transport this SDK documents: no redirects (the `x-api-key` header would follow a redirect
/// to another host) and no reqwest retries (each `send()` is one request on the wire). Timeouts
/// are the application's: a whole-request timeout also bounds a stream, so prefer
/// `connect_timeout` and `read_timeout` on the builder.
///
/// # Errors
///
/// reqwest's, if the client cannot be built (its TLS backend fails to initialize).
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
}

#[cfg(test)]
mod read_operation_tests {
    use super::*;
    #[test]
    fn declared_post_reads_keep_their_method_and_effect() {
        let operation = AnthropicMessagesCountTokens::operation();
        assert_eq!(operation.method(), Some("POST"));
        assert!(operation.is_read(), "{}", operation.name());
    }
}
