use concord::read::{RecordFail, RecordStream, Sse, SseEvent, Step};

/// The start and stop events of a message.
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
