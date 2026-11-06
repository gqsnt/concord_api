//! KU-B watch decoding without a network: the events fixture through the SDK's typed watch stream
//! over an in-memory byte source, split at **every** byte offset and fed one byte at a time.
//! Network behaviour (chunked transfer, connection loss, cancellation) is covered by
//! `ku_b_list_watch.rs`; this file covers framing at arbitrary boundaries.

mod support;

use concord::jsonl::JsonValuesReader;
use concord::stream::{Chunks, ReadErrorKind};
use concord_example_kubernetes_sdk::model::Pod;
use concord_example_kubernetes_sdk::watch::{WatchEvent, WatchStream};
use support::fixture;

async fn decode(chunks: Chunks) -> (Vec<WatchEvent<Pod>>, Option<ReadErrorKind>) {
    let reader = JsonValuesReader::new().max_value_bytes(64 * 1024);
    // The SDK's watch stream is the JSON values reader itself, here over an in-memory source.
    let mut stream: WatchStream<Pod, Chunks> = reader.read(chunks);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => events.push(event),
            Err(error) => return (events, Some(error.kind())),
        }
    }
    (events, None)
}

#[tokio::test]
async fn every_split_offset_decodes_the_same_events() {
    let bytes = fixture("ku_b/watch_pods_team_a_1.json-values");
    let (whole, error) = decode(Chunks::new([bytes.clone()])).await;
    assert_eq!(error, None);
    let types: Vec<_> = whole.iter().map(WatchEvent::type_name).collect();
    assert_eq!(types, ["MODIFIED", "ADDED", "BOOKMARK", "DELETED"]);

    for at in 1..bytes.len() {
        let (events, error) =
            decode(Chunks::new([bytes[..at].to_vec(), bytes[at..].to_vec()])).await;
        assert_eq!(error, None, "split at {at}");
        assert_eq!(events, whole, "split at {at}");
    }
    let bytewise: Vec<Vec<u8>> = bytes.iter().map(|b| vec![*b]).collect();
    let (events, error) = decode(Chunks::new(bytewise)).await;
    assert_eq!(error, None);
    assert_eq!(events, whole);
}

#[tokio::test]
async fn events_need_no_newline_and_an_unfinished_event_is_not_an_end() {
    // Values back to back without separators decode; input ending inside a value is a format
    // error, not a clean end of the stream.
    let bytes = fixture("ku_b/watch_pods_team_a_2.json-values");
    let compact = String::from_utf8(bytes).unwrap().trim().to_owned();
    let twice = format!("{compact}{compact}");
    let (events, error) = decode(Chunks::new([twice.into_bytes()])).await;
    assert_eq!((events.len(), error), (2, None));

    let truncated = compact[..compact.len() - 3].to_owned();
    let (events, error) = decode(Chunks::new([truncated.into_bytes()])).await;
    assert_eq!((events.len(), error), (0, Some(ReadErrorKind::Format)));

    // A source failure mid-event is a transport error.
    let (events, error) =
        decode(Chunks::new([compact[..40].to_owned()]).then_fail("connection reset")).await;
    assert_eq!((events.len(), error), (0, Some(ReadErrorKind::Transport)));
}
