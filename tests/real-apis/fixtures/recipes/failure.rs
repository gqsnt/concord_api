use concord::read::{RecordFail, RecordStream, Sse, SseEvent, Step};
use concord::{mismatch, Binary, Fail, Json};
use serde::Deserialize;

/// The two events needed to tell whether the caller consumed the start.
#[derive(Debug, PartialEq)]
pub enum MessageEvent {
    /// The start of a message.
    Start,
    /// The last event of a message.
    Stop,
}

/// Reads a message event and identifies the final one.
pub fn message_event(event: SseEvent) -> Result<Step<MessageEvent>, RecordFail<()>> {
    match event.event.as_str() {
        "message_start" => Ok(Step::Next(MessageEvent::Start)),
        "message_stop" => Ok(Step::Last(MessageEvent::Stop)),
        _ => Err(RecordFail::mismatch("unexpected message event")),
    }
}

concord::api! {
    client Messages {
        base "https://messages.example.test";
        GET stream() -> RecordStream<MessageEvent, messages::Error> {
            read Sse::new(8192).try_map(message_event).until_last_record();
        }
    }
}

#[derive(Debug)]
enum FinishError {
    Stream(messages::Error),
    StartAlreadyConsumed,
}

impl From<messages::Error> for FinishError {
    fn from(error: messages::Error) -> Self {
        Self::Stream(error)
    }
}

async fn finish(
    mut events: RecordStream<MessageEvent, messages::Error>,
) -> Result<(), FinishError> {
    let mut started = false;
    while let Some(event) = events.try_next().await? {
        started |= event == MessageEvent::Start;
    }
    if started {
        Ok(())
    } else {
        Err(FinishError::StartAlreadyConsumed)
    }
}

#[derive(Deserialize)]
struct Ticket {
    upload_url: Option<String>,
    file_id: Option<String>,
}

/// A complete ticket to send a file.
#[derive(Debug)]
pub struct UploadTicket {
    /// Where the bytes go.
    pub url: String,
    /// The ID to pass when completing the upload.
    pub id: String,
}

/// Decodes a ticket and checks the two fields that the next step needs.
pub fn check_ticket(body: concord::bytes::Bytes) -> Result<UploadTicket, Fail<()>> {
    let ticket: Ticket = Json::decode(&body).map_err(Fail::from_read)?;
    match (ticket.upload_url, ticket.file_id) {
        (Some(url), Some(id)) => Ok(UploadTicket { url, id }),
        _ => Err(mismatch("the upload ticket is missing its URL or file id")),
    }
}

concord::api! {
    client Files {
        base "https://files.example.test";
        GET upload_ticket() -> UploadTicket {
            read Binary;
            status { 2xx { body } => check_ticket(body)? }
        }
    }
}
