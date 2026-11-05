//! Probe (review): a JSON-record full-job consumer of `concord-pagination` (PAG-08 names "CSV and
//! JSON-record full-job consumers"; the repository only has the CSV one). Each job requests a
//! JSON Lines body, reads it record by record with `concord::jsonl`, and completes at EOF.

use std::num::NonZeroUsize;
use std::time::Duration;

use bytes::Bytes;
use concord::jsonl::{JsonLinesOptions, JsonLinesReader};
use concord::reqwest;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_pagination::{Budget, Options, Outcome, Unfinished, independent};

fn line(i: u32) -> String {
    format!("{{\"id\":{i}}}\n")
}

#[tokio::test]
async fn jsonl_jobs_hold_their_slot_through_a_stalled_body() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        // One record at once, the second only after 30 s: the body stalls mid-stream.
        "/a" => Reply::new(200)
            .header("content-type", "application/jsonl")
            .with_body(Body::Chunks(vec![
                (Duration::ZERO, Bytes::from(line(1))),
                (Duration::from_secs(30), Bytes::from(line(2))),
            ])),
        "/b" => Reply::text(200, "application/jsonl", line(1) + &line(2) + &line(3)),
        "/c" => Reply::text(200, "application/jsonl", line(1) + &line(2)),
        "/d" => Reply::text(200, "application/jsonl", line(1)),
        _ => Reply::new(404),
    })
    .await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let base = server.url();
    let reader = JsonLinesReader::new(JsonLinesOptions::json_lines().max_record_bytes(1024));
    let options = Options::new(Budget::new(NonZeroUsize::new(10).unwrap()))
        .window(NonZeroUsize::new(2).unwrap());
    let mut jobs = independent(["a", "b", "c", "d"], options, |name: &'static str| {
        let request = http.get(format!("{base}/{name}"));
        let reader = reader.clone();
        async move {
            let response = request.send().await.map_err(|_| "transport")?;
            let mut records = reader
                .open_for::<serde_json::Value>(response, "probe: jsonl part")
                .map_err(|_| "open")?;
            let mut count = 0u64;
            while let Some(record) = records.next().await {
                record.map_err(|_| "read")?;
                count += 1;
            }
            Ok::<u64, &'static str>(count)
        }
    });
    // `a` stalls after one record; `b` completes and is retained behind it (ordered).
    let pending = tokio::time::timeout(Duration::from_millis(300), jobs.next()).await;
    assert!(pending.is_err(), "nothing emitted while the first body stalls");
    let progress = jobs.progress();
    println!(
        "in_flight={} retained={} requests={:?}",
        progress.in_flight,
        progress.retained,
        server.requests().iter().map(|r| r.path().to_owned()).collect::<Vec<_>>()
    );
    assert_eq!((progress.in_flight, progress.retained), (1, 1));
    assert_eq!(server.requests().len(), 2, "c and d not admitted");
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.retained.len(), 1);
    assert_eq!(report.retained[0].value, 3);
    assert_eq!(
        report.unfinished.iter().map(|e| (e.key, e.value)).collect::<Vec<_>>(),
        [("a", Unfinished::Cancelled), ("b", Unfinished::Retained)]
    );
    let cleanup = server.shutdown().await;
    println!("{cleanup:?}");
}
