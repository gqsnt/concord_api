concord::api! {
client Messages {
        base "https://messages.example.test";
        GET stream() -> RecordStream<MessageEvent, messages::Error> {
            read Sse::new(8192).try_map(message_event).until_last_record();
        }
    }
}
