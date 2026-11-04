//! Incremental behavior observable from a byte source: a slow (or stopped) consumer does not drive
//! the source, and a generated input much larger than any buffer is read with at most one chunk of
//! read-ahead, for both the JSON Lines and the consecutive-values readers.
#![cfg(feature = "jsonl")]

mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use concord_extras::jsonl::{JsonLinesOptions, JsonLinesReader, JsonValuesReader};
use serde::Deserialize;
use support::{Counting, Generated};

#[derive(Debug, Deserialize)]
struct Item {
    id: u64,
    name: String,
    tags: Vec<String>,
}

/// Nothing runs ahead of the caller: between two `next()` calls (here 50 ms apart, then a
/// consumer that stops) the source is not polled at all.
#[tokio::test]
async fn a_slow_consumer_does_not_drive_the_source() {
    let chunks =
        (0..100).map(|i| format!("{{\"id\":{i},\"name\":\"n\",\"tags\":[]}}\n").into_bytes());
    let (source, polls) = Counting::new(chunks);
    let mut records = JsonLinesReader::new(JsonLinesOptions::json_lines()).read::<Item, _>(source);
    assert_eq!(records.next().await.unwrap().unwrap().id, 0);
    assert_eq!(polls.load(Ordering::SeqCst), 1, "one chunk for one record");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        polls.load(Ordering::SeqCst),
        1,
        "no progress while the consumer waits"
    );
    assert_eq!(records.next().await.unwrap().unwrap().id, 1);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        polls.load(Ordering::SeqCst),
        2,
        "a stopped consumer stops the reads"
    );
    drop(records);
}

fn line(id: u64) -> Vec<u8> {
    format!(
        "{{\"id\":{id},\"name\":\"item-{id:08}\",\"tags\":[\"alpha\",\"beta\",\"{}\"]}}\n",
        "g".repeat((id % 50) as usize)
    )
    .into_bytes()
}

/// About 8.7 MB generated one chunk at a time (4093-byte chunks cut records anywhere): every
/// record arrives in order, and at every record the reader has pulled at most one chunk beyond
/// what it consumed; the generator never produced more than that either. The line limit (4 KiB)
/// is far below the input size, so whole-body buffering would fail.
#[tokio::test]
async fn generated_input_is_read_with_one_chunk_of_read_ahead() {
    const RECORDS: u64 = 100_000;
    const CHUNK: usize = 4093;
    let (source, polls, produced) = Generated::new(RECORDS, CHUNK, line);
    let mut records = JsonLinesReader::new(JsonLinesOptions::json_lines().max_record_bytes(4096))
        .read::<Item, _>(source);
    let mut next = 0;
    let mut bytes = 0u64;
    while let Some(record) = records.next().await {
        let record = record.unwrap();
        assert_eq!(record.id, next);
        assert_eq!(record.name, format!("item-{next:08}"));
        assert_eq!(record.tags.len(), 3);
        next += 1;
        let consumed = records.position().bytes;
        let pulled = produced.load(Ordering::SeqCst) as u64;
        assert!(
            pulled <= consumed + CHUNK as u64,
            "read ahead {} bytes",
            pulled - consumed
        );
        assert!(polls.load(Ordering::SeqCst) as u64 <= consumed / CHUNK as u64 + 2);
        bytes = consumed;
    }
    assert_eq!(next, RECORDS);
    assert_eq!(bytes, produced.load(Ordering::SeqCst) as u64);
    // Library-owned memory: one unfinished line at a time, never the input.
    let peak = records.peak_retained_bytes();
    assert!(peak > 0 && peak <= 4097, "peak {peak}");
    assert_eq!(records.retained_bytes(), 0);
    assert_eq!(records.pending_input_bytes(), 0);
    assert!(bytes > 8_000_000, "{bytes} bytes");
}

/// The same for consecutive values: multiline objects without separators, generated lazily.
#[tokio::test]
async fn generated_values_are_read_with_one_chunk_of_read_ahead() {
    const VALUES: u64 = 50_000;
    const CHUNK: usize = 1021;
    let (source, polls, produced) = Generated::new(VALUES, CHUNK, |id| {
        format!(
            "{{\n  \"type\": \"MODIFIED\",\n  \"object\": {{\"id\": {id}, \"note\": \"}}{{\\\"\"}}\n}}"
        )
        .into_bytes()
    });
    let mut values = JsonValuesReader::new()
        .max_value_bytes(256)
        .read::<serde_json::Value, _>(source);
    let mut next = 0;
    while let Some(value) = values.next().await {
        let value = value.unwrap();
        assert_eq!(value["object"]["id"], next);
        assert_eq!(value["object"]["note"], "}{\"");
        next += 1;
        let consumed = values.position().bytes;
        assert!(produced.load(Ordering::SeqCst) as u64 <= consumed + CHUNK as u64);
        assert!(polls.load(Ordering::SeqCst) as u64 <= consumed / CHUNK as u64 + 2);
        assert!(values.retained_bytes() <= 256);
    }
    assert_eq!(next, VALUES);
    let peak = values.peak_retained_bytes();
    assert!(peak > 0 && peak <= 256, "peak {peak}");
}

/// The accessors count what they document: a line copied across a chunk boundary is retained
/// until parsed; the chunk remainder is pending input; the source handed back is partially
/// consumed (it resumes after the chunks the reader took).
#[tokio::test]
async fn retained_pending_and_into_source() {
    use concord_extras::stream::ByteSource;
    let chunks = vec![
        b"{\"id\":1".to_vec(),
        b"2,\"name\":\"n\",\"tags\":[]}\n{\"id\":3,\"name\":\"n\",\"tags\":[]}\n".to_vec(),
        b"{\"id\":4,\"name\":\"n\",\"tags\":[]}\n".to_vec(),
    ];
    let (source, _) = Counting::new(chunks);
    let mut records = JsonLinesReader::new(JsonLinesOptions::json_lines()).read::<Item, _>(source);
    assert_eq!(
        (records.retained_bytes(), records.pending_input_bytes()),
        (0, 0)
    );
    assert_eq!(records.next().await.unwrap().unwrap().id, 12);
    assert_eq!(records.retained_bytes(), 0);
    let first_line = br#"{"id":12,"name":"n","tags":[]}"#;
    assert_eq!(
        records.peak_retained_bytes(),
        first_line.len(),
        "the whole first line, copied"
    );
    let second_line = b"{\"id\":3,\"name\":\"n\",\"tags\":[]}\n";
    assert_eq!(
        records.pending_input_bytes(),
        second_line.len(),
        "not framed yet"
    );
    let mut source = records.into_source();
    let rest = source.next_chunk().await.unwrap().unwrap();
    assert!(
        rest.starts_with(b"{\"id\":4"),
        "the reader took chunks 1 and 2"
    );
}
