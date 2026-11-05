//! A JSON-record full-job consumer of `concord-pagination` (PAG-08): each job requests a JSON
//! Lines body, reads it record by record with `concord::jsonl` and completes at its end, so the
//! job holds its window slot through the whole body, not only until the response head.
//!
//! Setup: four jobs (`a`..`d`) through `independent` at window 2, ordered. `a`'s body delivers
//! one record, then trickles the start of a second record one byte every 25 ms (the second record
//! never completes within the test: the reader waits inside it, bounded by its 1 KiB record
//! limit). The trickle is what lets the server observe the client closing the stalled connection
//! (its next write fails); a body that only sleeps would keep the server task alive until
//! shutdown aborts it. `b`, `c` and `d` are complete bodies of 3, 2 and 1 records.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::time::Duration;

use concord::bytes::Bytes;
use concord::jsonl::{JsonLinesOptions, JsonLinesReader};
use concord::reqwest;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_pagination::{Budget, Options, Outcome, Unfinished, independent};

fn line(i: u32) -> String {
    format!("{{\"id\":{i}}}\n")
}

fn jsonl(body: String) -> Reply {
    Reply::text(200, "application/jsonl", body)
}

#[tokio::test]
async fn jsonl_jobs_hold_their_slot_through_a_stalled_body() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/a" => {
            let mut sent = 0u32;
            let trickle = Box::new(move || {
                sent += 1;
                Some(Bytes::from(match sent {
                    1 => line(1) + "{\"id\":2,\"pad\":\"",
                    _ => "x".to_owned(),
                }))
            });
            Reply::new(200)
                .header("content-type", "application/jsonl")
                .with_body(Body::Generate(trickle, Some(Duration::from_millis(25))))
        }
        "/b" => jsonl(line(1) + &line(2) + &line(3)),
        "/c" => jsonl(line(1) + &line(2)),
        "/d" => jsonl(line(1)),
        _ => Reply::new(404),
    })
    .await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let base = server.url();
    let reader = JsonLinesReader::new(JsonLinesOptions::json_lines().max_record_bytes(1024));
    // Records each job has read so far (the jobs run on this task: no `Send` needed).
    let read: RefCell<BTreeMap<&'static str, u64>> = RefCell::default();
    let options = Options::new(Budget::new(NonZeroUsize::new(10).unwrap()))
        .window(NonZeroUsize::new(2).unwrap());
    let mut jobs = independent(["a", "b", "c", "d"], options, |name: &'static str| {
        let request = http.get(format!("{base}/{name}"));
        let reader = reader.clone();
        let read = &read;
        async move {
            let response = request.send().await.map_err(|_| "transport")?;
            let mut records = reader
                .open_for::<serde_json::Value>(response, "check: jsonl job")
                .map_err(|_| "open")?;
            let mut count = 0u64;
            while let Some(record) = records.next().await {
                record.map_err(|_| "read")?;
                count += 1;
                read.borrow_mut().insert(name, count);
            }
            Ok::<u64, &'static str>(count)
        }
    });

    // Drive the window until `b` has completed and is retained behind the stalled `a`. Nothing
    // may be emitted meanwhile (ordered emission).
    let mut polls = 0;
    loop {
        let polled = tokio::time::timeout(Duration::from_millis(20), jobs.next()).await;
        assert!(
            polled.is_err(),
            "nothing is emitted while the first body stalls"
        );
        if jobs.progress().retained == 1 && read.borrow().get("a") == Some(&1) {
            break;
        }
        polls += 1;
        assert!(polls < 250, "b never completed: {:?}", jobs.progress());
    }
    let progress = jobs.progress();
    assert_eq!(
        (progress.in_flight, progress.retained),
        (1, 1),
        "{progress:?}"
    );
    assert_eq!(progress.admitted, 2);
    // The stalled job keeps its slot: `c` and `d` were never requested.
    let mut paths: Vec<String> = server
        .requests()
        .iter()
        .map(|r| r.path().to_owned())
        .collect();
    paths.sort();
    assert_eq!(paths, ["/a", "/b"]);
    assert_eq!(server.active_bodies(), 1, "a's body is still being written");
    assert_eq!(
        *read.borrow(),
        BTreeMap::from([("a", 1), ("b", 3)]),
        "a stalled after one record"
    );

    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.progress.cancelled, 1, "a's job was dropped");
    assert_eq!(report.progress.emitted, 0);
    assert_eq!(
        report
            .retained
            .iter()
            .map(|e| (e.key, e.value))
            .collect::<Vec<_>>(),
        [("b", 3)]
    );
    assert_eq!(
        report
            .unfinished
            .iter()
            .map(|e| (e.key, e.value))
            .collect::<Vec<_>>(),
        [("a", Unfinished::Cancelled), ("b", Unfinished::Retained)]
    );
    assert!(report.unemitted_errors.is_empty());
    let (retained, rerun) = report.into_rerun();
    assert_eq!(retained.len(), 1);
    assert_eq!(rerun.collect::<Vec<_>>(), ["a", "c", "d"]);

    // Dropping a's job dropped its response: the client closed that connection, and the server's
    // next trickle write ended a's body. Dropping the client closes b's pooled connection.
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.active_bodies() == 0)
            .await,
        "a's body is still being written after finish"
    );
    drop(http);
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.live_connections() == 0)
            .await,
        "{} connection(s) still open",
        server.live_connections()
    );
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.connections, 2);
    assert_eq!(cleanup.open_at_shutdown, 0, "{cleanup:?}");
    assert_eq!(cleanup.open_after, 0, "{cleanup:?}");
}
