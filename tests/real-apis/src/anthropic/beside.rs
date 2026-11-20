use concord::read::{RecordFail, SseEvent, Step};
use serde::{Deserialize, Serialize};

/// Anthropic's error response, and the data of an `error` event:
/// `{"type": "error", "error": {"type": …, "message": …}}`.
#[derive(Debug, Deserialize)]
pub struct ApiProblem {
    pub error: ApiError,
}

#[derive(Debug, Deserialize)]
pub struct ApiError {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

/// One turn of the conversation, as the request sends it.
#[derive(Clone, Debug, Serialize)]
pub struct Turn {
    pub role: &'static str,
    pub content: String,
}

impl Turn {
    pub fn user(content: impl Into<String>) -> Self {
        Turn { role: "user", content: content.into() }
    }
}

/// What a streamed reply means to this application, read from the untyped events.
#[derive(Debug)]
pub enum Streamed {
    /// A piece of the text of the reply.
    Text(String),
    /// The reply stopped: `end_turn`, `max_tokens`, and so on.
    Stopped { stop_reason: Option<String> },
    /// `message_start`, a delta that is not text, or an event type added later.
    Other,
}

/// What one server-sent event is: the event's type chooses the shape of its data. `ping` is
/// skipped, `message_stop` ends the reply, and an `error` event is the API's own error.
pub fn streamed(event: SseEvent) -> Result<Step<Streamed>, RecordFail<ApiProblem>> {
    #[derive(Deserialize)]
    struct BlockDelta {
        delta: Delta,
    }
    #[derive(Deserialize)]
    #[serde(tag = "type")]
    enum Delta {
        #[serde(rename = "text_delta")]
        Text { text: String },
        #[serde(other)]
        Other,
    }
    #[derive(Deserialize)]
    struct MessageDelta {
        delta: Stop,
    }
    #[derive(Deserialize)]
    struct Stop {
        stop_reason: Option<String>,
    }
    Ok(match event.event.as_str() {
        "content_block_delta" => Step::Next(match event.json::<BlockDelta>()?.delta {
            Delta::Text { text } => Streamed::Text(text),
            Delta::Other => Streamed::Other,
        }),
        "message_delta" => {
            let stop_reason = event.json::<MessageDelta>()?.delta.stop_reason;
            Step::Next(Streamed::Stopped { stop_reason })
        }
        "message_stop" => Step::End,
        "error" => return Err(RecordFail::problem(event.json()?)),
        "ping" => Step::Skip,
        _ => Step::Next(Streamed::Other),
    })
}

/// A page of models.
#[derive(Debug, Deserialize)]
pub struct ModelPage {
    pub data: Vec<ModelInfo>,
    pub has_more: bool,
    pub last_id: Option<String>,
}

impl ModelPage {
    /// The last ID, used as the cursor for the next page.
    pub fn next(&self) -> Option<String> {
        self.last_id.clone()
    }
}

impl concord::page::IntoItems for ModelPage {
    type Item = ModelInfo;
    fn into_items(self) -> Vec<ModelInfo> {
        self.data
    }
    fn count(&self) -> usize {
        self.data.len()
    }
}
