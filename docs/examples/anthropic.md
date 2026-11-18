# Anthropic

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [Elasticsearch](elasticsearch.md) · next: [Google Drive](drive.md)*

This page is for a Rust developer who calls the Anthropic API, an API of large language models. It gives a client with 5 endpoints that you can copy, and shows a reply streamed as server-sent events, typed by your own function, and an error that arrives inside the stream after a `200`.

Here is the Anthropic API as Concord sees it. There is one host, `api.anthropic.com`, under `/v1`. Authentication is an API key in the header `x-api-key`, a `Secret`, and every request names the version of the API in `anthropic-version`. A message is a `POST` with a JSON body: the model, the most tokens that the reply may use, and the turns of the conversation. With `"stream": true` in the same body, the reply arrives as server-sent events: `message_start`, then for each block of content `content_block_start`, `content_block_delta` events and `content_block_stop`, then `message_delta` (which holds the reason why the reply stopped) and `message_stop`; `ping` events may appear anywhere, and the API may add event types. The API rejects a request with an error status and `{"type": "error", "error": {"type", "message"}}`, read into `ApiProblem`; inside a stream, the same shape arrives as an `error` event. The list of models is paginated by the ID of its last item. Anthropic's own reference is [docs.anthropic.com](https://docs.anthropic.com). `(unsure)` marks a detail about the API that has not been checked.

## The description

```rust
use concord::read::{RecordStream, Sse};
use concord::{by_cursor, json, Json, Secret};
use std::time::Duration;

concord::api! {
    /// The Anthropic API: a message, whole or streamed as server-sent events; tokens; models.
    pub client Anthropic(api_key: Secret) {
        base "https://api.anthropic.com/v1";
        headers {
            "x-api-key" = api_key;
            "anthropic-version" = "2023-06-01";
        }
        read Json;
        problem ApiProblem;

        part request(model: String, max_tokens: u32, messages: Vec<Turn>, system?: String) {
            body json { model; max_tokens; messages; system; }
        }

        /// The whole reply in one response.
        POST create(..request) -> Message { path "messages"; }

        /// The same request with `"stream": true`: the reply arrives as events, read one by one.
        POST stream(..request) -> RecordStream<Streamed, anthropic::Error> {
            path "messages";
            body json { "stream" = true; }
            read Sse::new(64 * concord::KIB).try_map(streamed).until_last_record();
            idle_timeout Duration::from_secs(120);
        }

        POST count_tokens(model: String, messages: Vec<Turn>, system?: String) -> TokenCount {
            path "messages/count_tokens";
            body json { model; messages; system; }
        }

        GET models(limit?: u32, after_id?: String) -> ModelPage {
            path "models";
            query { limit; after_id; }
            paginate by_cursor(after_id, ModelPage::next);
        }
        GET model(model_id: String) -> ModelInfo { path "models/{model_id}"; }
    }
}
```

`stream` is the same request as `create`, plus `"stream": true`, and a different return type: a typed stream, `RecordStream<Streamed, anthropic::Error>`, read by `Sse` with a bound of 64 KiB on one event and mapped by `streamed`, below. `until_last_record()` says that the reply ends with a record of its own, `message_stop`: a body that ends before it is an error, not a whole reply. The bound is this example's policy; a delta of text is far smaller (unsure for the largest event that the API sends). The idle timeout of two minutes is this example's margin for a long silence before the first event (unsure how often the API sends `ping` events). No call deadline (`with_timeout`) is set, so a long reply is not cut.

## Beside the description

The data types are plain `serde` types, named here but not shown: `Message` (`content`, a list of blocks with `type` and `text`), `TokenCount` (`input_tokens`) and `ModelInfo` (`id`, `display_name`).

`Sse` frames the events and gives them no meaning: each one is an `SseEvent` with its type (`event`) and its data as text (`data`). Your own function says what each event is: a value of the stream (`Step::Next`), an event to skip (`Step::Skip`), the end of the reply (`Step::End`), or the API's own error (`RecordFail::problem`). It reads the data into the type that its event type names, and `?` on `event.json()` makes data that does not decode an error of the stream:

```rust
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

/// A page of models. For `by_cursor(after_id, …)`: the last ID, while there are more.
#[derive(Debug, Deserialize)]
pub struct ModelPage {
    pub data: Vec<ModelInfo>,
    pub has_more: bool,
    pub last_id: Option<String>,
}

impl ModelPage {
    pub fn next(&self) -> Option<String> {
        self.last_id.clone().filter(|_| self.has_more)
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
```

## Calling it

```rust
type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client, and a reply in one response. The model is the caller's choice.
pub async fn ask() -> Result<(), BoxError> {
    let api = Anthropic::new(std::env::var("ANTHROPIC_API_KEY")?);
    let model = std::env::var("ANTHROPIC_MODEL")?;
    let messages = vec![Turn::user("Name three rivers.")];
    let tokens = api.count_tokens(&model, &messages).await?;
    println!("{} input tokens", tokens.input_tokens);
    let reply = api.create(&model, 1024, messages).system("Answer briefly.").await?;
    for block in reply.content {
        println!("{}", block.text.unwrap_or_default());
    }
    Ok(())
}

/// The text of a streamed reply, printed as it arrives. The stream's error is the client's own:
/// the API's `error` event is a Refused error with `problem()`, and a reply cut before
/// `message_stop` is a Mismatch.
pub async fn stream(api: &Anthropic, model: &str, prompt: &str) -> anthropic::Result<String> {
    let mut events = api.stream(model, 1024, vec![Turn::user(prompt)]).await?;
    let mut text = String::new();
    while let Some(event) = events.try_next().await? {
        if let Streamed::Text(piece) = event {
            print!("{piece}");
            text.push_str(&piece);
        }
    }
    Ok(text)
}

/// Every model, page after page.
pub async fn models(api: &Anthropic) -> anthropic::Result<()> {
    let mut models = api.models().limit(100).pages().items();
    while let Some(model) = models.try_next().await? {
        println!("{} {}", model.id, model.display_name);
    }
    Ok(())
}
```

## What does not fit well, and the workaround

| The API wants | What to write |
|---|---|
| One path for two shapes of response: a whole message, or a stream of events, chosen by a field of the body. The return type is fixed. | Two endpoints, `create` and `stream`, on the same path; the part `request` keeps their inputs the same, and `stream` adds `"stream" = true`. |
| Events whose data has a different shape for each event type. `Sse` gives no meaning to the data. | Your own function, `streamed`, given to `Sse::new(…).try_map(streamed)`: it matches `event.event` and decodes `event.data` with `SseEvent::json`. An event type that it does not know is `Other`; `ping` is skipped and never reaches the loop. |
| An error after the `200`, as an `error` event. | `RecordFail::problem` in `streamed`: the stream ends with the client's own error, Refused, its `problem()` the `ApiProblem` of the event, as for an error before the stream (`refused: status 200, in anthropic.stream`). The loop's `?` passes it on. |
| A stream that ends before `message_stop`: the connection was closed early. | `until_last_record()`: `try_next` returns the Mismatch `the body ended before its last record`. A broken connection is the client's Transport error instead. The reader never reconnects and never sends the request again. |
| Events after `message_stop`. | None are read: `Step::End` releases the body at once. |
| The model, chosen by each application. | An input; the example reads it from the environment and names none. |
