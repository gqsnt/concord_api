//! The request and response models of the slice: what a message request sends and what a message
//! (non-streaming, or assembled from a stream) contains.
//!
//! Field names and shapes follow the request and the `Message` object shown on the cited
//! documentation pages (see `PROVENANCE.md`). Content blocks other than `text` (tool use,
//! thinking) are kept as received and are not interpreted: they are outside this slice.

use std::fmt;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

/// Who wrote a turn of the conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The caller's turn (`"user"`).
    User,
    /// The model's turn (`"assistant"`): every returned message has this role.
    Assistant,
}

/// One input turn with text content (`{"role": "user", "content": "Hello"}`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InputMessage {
    role: Role,
    content: String,
}

impl InputMessage {
    /// A user turn.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: text.into(),
        }
    }

    /// An assistant turn (an earlier answer, in a multi-turn conversation). Some models refuse a
    /// conversation that *ends* with an assistant turn (prefill): the API answers 400
    /// `invalid_request_error`; this SDK does not decide which models accept it.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: text.into(),
        }
    }

    /// The turn's role.
    pub fn role(&self) -> Role {
        self.role
    }

    /// The turn's text.
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// Why [`MessageRequest::new`] refused its inputs. Nothing was sent.
///
/// These are this SDK's preconditions (a request that cannot mean anything), not a copy of the
/// API's validation: the API's own refusals arrive as `400 invalid_request_error`
/// ([`ApiError`](crate::ApiError)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InvalidRequest {
    /// The model name is empty.
    EmptyModel,
    /// `max_tokens` is zero.
    ZeroMaxTokens,
    /// There is no input turn.
    NoMessages,
}

impl fmt::Display for InvalidRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyModel => "the model name is empty",
            Self::ZeroMaxTokens => "max_tokens must be at least 1",
            Self::NoMessages => "a message request needs at least one input message",
        })
    }
}

impl std::error::Error for InvalidRequest {}

/// The body of a non-streaming message request: `model`, `max_tokens`, text `messages` and
/// optional `system` and `tools`. Its JSON has no `stream` field;
/// [`streaming`](Self::streaming) turns it into the body of the streaming endpoint.
///
/// ```
/// use concord_sdk_anthropic_messages::{InputMessage, MessageRequest};
///
/// let request = MessageRequest::new("claude-opus-5-5", 256, vec![InputMessage::user("Hello")])?;
/// assert_eq!(
///     serde_json::to_value(&request)?,
///     serde_json::json!({
///         "model": "claude-opus-5-5",
///         "max_tokens": 256,
///         "messages": [{"role": "user", "content": "Hello"}],
///     }),
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MessageRequest {
    #[serde(flatten)]
    inputs: CountTokensRequest,
    max_tokens: u32,
}

impl MessageRequest {
    /// A request for `model`, generating at most `max_tokens`, over `messages` (oldest first).
    ///
    /// # Errors
    ///
    /// [`InvalidRequest`] for an empty model name, a zero `max_tokens` or no message.
    pub fn new(
        model: impl Into<String>,
        max_tokens: u32,
        messages: Vec<InputMessage>,
    ) -> Result<Self, InvalidRequest> {
        let model = model.into();
        if model.is_empty() {
            return Err(InvalidRequest::EmptyModel);
        }
        if max_tokens == 0 {
            return Err(InvalidRequest::ZeroMaxTokens);
        }
        if messages.is_empty() {
            return Err(InvalidRequest::NoMessages);
        }
        Ok(Self {
            inputs: CountTokensRequest {
                model,
                messages,
                system: None,
                tools: None,
            },
            max_tokens,
        })
    }

    /// A request with one user turn: `MessageRequest::user("claude-opus-5-5", 256, "Hello")`.
    ///
    /// # Errors
    ///
    /// As [`new`](Self::new).
    pub fn user(
        model: impl Into<String>,
        max_tokens: u32,
        text: impl Into<String>,
    ) -> Result<Self, InvalidRequest> {
        Self::new(model, max_tokens, vec![InputMessage::user(text)])
    }

    /// The same request as the body of the streaming endpoint (`"stream": true`).
    pub fn streaming(self) -> StreamRequest {
        StreamRequest(self)
    }

    /// Sets the system prompt: a string or a JSON array of text blocks, as accepted by the API.
    /// The JSON is passed through without local schema validation.
    pub fn with_system(mut self, system: impl Into<Value>) -> Self {
        self.inputs.system = Some(system.into());
        self
    }

    /// Sets tool definitions (JSON objects with `name`, `input_schema`, and optionally
    /// `description`). They are passed through without interpretation or execution; the API
    /// validates them. Some server tools accepted by creation are refused by token counting.
    pub fn with_tools(mut self, tools: Vec<Value>) -> Self {
        self.inputs.tools = Some(tools);
        self
    }

    /// A snapshot of this request's model, messages, system and tools for
    /// [`count_tokens`](crate::AnthropicMessages::count_tokens). Clones only those inputs;
    /// the original request remains available for creation or streaming. No `max_tokens` or
    /// `stream` can appear in this body's JSON.
    pub fn count_tokens(&self) -> CountTokensRequest {
        self.inputs.clone()
    }

    /// The model name.
    pub fn model(&self) -> &str {
        &self.inputs.model
    }

    /// The `max_tokens` bound.
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
    }

    /// The input turns, oldest first.
    pub fn messages(&self) -> &[InputMessage] {
        &self.inputs.messages
    }

    /// The system prompt, if set.
    pub fn system(&self) -> Option<&Value> {
        self.inputs.system.as_ref()
    }

    /// The tool definitions, if set.
    pub fn tools(&self) -> Option<&[Value]> {
        self.inputs.tools.as_deref()
    }
}

/// The input-only body of `POST /v1/messages/count_tokens`, obtained with
/// [`MessageRequest::count_tokens`]. This shares the input field definitions with creation;
/// generation settings are kept exclusively in [`MessageRequest`] and [`StreamRequest`].
///
/// A counting body cannot be used to create a message:
///
/// ```compile_fail
/// use concord_sdk_anthropic_messages::{Anthropic, MessageRequest};
/// fn cannot_generate(api: &Anthropic, request: &MessageRequest) {
///     api.messages().create(request.count_tokens());
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CountTokensRequest {
    model: String,
    messages: Vec<InputMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
}

/// The token-counting response. A count is an estimate of input usage, not a generated message.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct TokenCount {
    /// Total input tokens for the supplied model, messages, system and tools.
    pub input_tokens: u64,
}

/// The body of the streaming endpoint: a [`MessageRequest`] whose JSON also carries
/// `"stream": true`. A distinct type, so the non-streaming endpoint cannot be sent a streaming
/// body (its response would be an event stream) and the other way round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamRequest(MessageRequest);

impl StreamRequest {
    /// The underlying request.
    pub fn request(&self) -> &MessageRequest {
        &self.0
    }

    /// The underlying request, taken back.
    pub fn into_request(self) -> MessageRequest {
        self.0
    }
}

impl From<MessageRequest> for StreamRequest {
    fn from(request: MessageRequest) -> Self {
        Self(request)
    }
}

impl Serialize for StreamRequest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Flattened, so a field added to `MessageRequest` reaches both bodies.
        #[derive(Serialize)]
        struct Wire<'a> {
            #[serde(flatten)]
            request: &'a MessageRequest,
            stream: bool,
        }
        Wire {
            request: &self.0,
            stream: true,
        }
        .serialize(serializer)
    }
}

/// A `Message` object: the response of a non-streaming request, or the message a
/// [`MessageStream`](crate::MessageStream) assembled from its events (the cited errors page states
/// that the two are identical).
///
/// `usage` is optional because the documented thinking stream's `message_start` carries none.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct Message {
    /// The message id (`msg_...`).
    pub id: String,
    /// The object type, `"message"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Always [`Role::Assistant`] for a returned message.
    pub role: Role,
    /// The content blocks, in order (a stream's block `index` is the position here).
    pub content: Vec<ContentBlock>,
    /// The model that produced the message.
    pub model: String,
    /// Why generation stopped; `None` while a stream has not said yet.
    pub stop_reason: Option<StopReason>,
    /// The stop sequence that ended generation, if any.
    pub stop_sequence: Option<String>,
    /// Token counts, if the API sent them.
    pub usage: Option<Usage>,
}

impl Message {
    /// The text of every `text` block, concatenated in order.
    pub fn text(&self) -> String {
        text_of(&self.content)
    }
}

pub(crate) fn text_of(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(ContentBlock::as_text)
        .collect::<Vec<_>>()
        .concat()
}

/// One content block of a [`Message`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ContentBlock {
    /// A `text` block.
    Text {
        /// The text (in a stream: the start value followed by every `text_delta`).
        text: String,
    },
    /// Any other block type (`tool_use`, `thinking`, a type added later), kept as received and
    /// not interpreted by this SDK.
    Other(OtherBlock),
}

impl ContentBlock {
    /// The block's `type` (`"text"`, `"tool_use"`, ...).
    pub fn block_type(&self) -> &str {
        match self {
            Self::Text { .. } => "text",
            Self::Other(other) => &other.block_type,
        }
    }

    /// The text of a `text` block.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            Self::Other(_) => None,
        }
    }
}

impl<'de> Deserialize<'de> for ContentBlock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let block_type = raw
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| de::Error::custom("a content block has no string `type`"))?;
        if block_type == "text" {
            let text = raw
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| de::Error::custom("a text block has no string `text`"))?;
            return Ok(Self::Text {
                text: text.to_owned(),
            });
        }
        Ok(Self::Other(OtherBlock {
            block_type: block_type.to_owned(),
            raw,
            deltas: Vec::new(),
        }))
    }
}

/// A content block this SDK does not interpret: its JSON object as received and, in an assembled
/// stream, the deltas that targeted it, uninterpreted and in order. Tool-use input and thinking
/// text are therefore **not** reconstructed: a `tool_use` block's `raw` keeps the `input` of its
/// `content_block_start` (`{}` in the documented stream), and its `input_json_delta` events are in
/// [`deltas`](Self::deltas).
#[derive(Clone, Debug, PartialEq)]
pub struct OtherBlock {
    block_type: String,
    raw: Value,
    deltas: Vec<Value>,
}

impl OtherBlock {
    /// The block's `type`.
    pub fn block_type(&self) -> &str {
        &self.block_type
    }

    /// The block's JSON object (the `content_block_start` value, in a stream).
    pub fn raw(&self) -> &Value {
        &self.raw
    }

    /// The `delta` objects of the stream's `content_block_delta` events for this block, in order;
    /// empty for a non-streaming message.
    pub fn deltas(&self) -> &[Value] {
        &self.deltas
    }

    pub(crate) fn push_delta(&mut self, delta: Value) {
        self.deltas.push(delta);
    }
}

/// Why generation stopped. The cited pages show `end_turn` and `tool_use`; any other value is
/// kept as sent.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StopReason {
    /// `end_turn`.
    EndTurn,
    /// `tool_use`.
    ToolUse,
    /// Another value, verbatim.
    Other(String),
}

impl StopReason {
    /// The wire value.
    pub fn as_str(&self) -> &str {
        match self {
            Self::EndTurn => "end_turn",
            Self::ToolUse => "tool_use",
            Self::Other(other) => other,
        }
    }
}

impl<'de> Deserialize<'de> for StopReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "end_turn" => Self::EndTurn,
            "tool_use" => Self::ToolUse,
            _ => Self::Other(value),
        })
    }
}

/// Token counts. Every field is optional: the documented events carry different subsets.
///
/// In a stream, the counts of a `message_delta` are **cumulative** (the streaming page's
/// warning): an assembled message takes each count from the latest event that carried it; counts
/// are never added up.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Usage {
    /// Input tokens.
    pub input_tokens: Option<u64>,
    /// Output tokens.
    pub output_tokens: Option<u64>,
    /// Input tokens written to the prompt cache.
    pub cache_creation_input_tokens: Option<u64>,
    /// Input tokens read from the prompt cache.
    pub cache_read_input_tokens: Option<u64>,
}

impl Usage {
    /// Applies a later, cumulative report: each count it carries replaces the current one.
    pub(crate) fn update(&mut self, later: &Usage) {
        let Usage {
            input_tokens,
            output_tokens,
            cache_creation_input_tokens,
            cache_read_input_tokens,
        } = later;
        for (current, later) in [
            (&mut self.input_tokens, input_tokens),
            (&mut self.output_tokens, output_tokens),
            (
                &mut self.cache_creation_input_tokens,
                cache_creation_input_tokens,
            ),
            (&mut self.cache_read_input_tokens, cache_read_input_tokens),
        ] {
            if later.is_some() {
                *current = *later;
            }
        }
    }
}
