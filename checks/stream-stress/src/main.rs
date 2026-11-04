//! `cargo run --release -p concord-check-stream-stress -- [--mib N] [--out FILE]`
//!
//! Runs the N/2N read and upload workloads (N MiB of decoded records, default 256) plus the slow,
//! cancelled and failing variants, and prints one JSON array of measurements.

use std::time::Duration;

use concord_check_stream_stress::*;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let mut mib: u64 = 256;
    let mut out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mib" => mib = args.next().and_then(|v| v.parse().ok()).expect("--mib N"),
            "--out" => out = args.next(),
            other => panic!("unknown argument {other}"),
        }
    }
    let rows = |mib: u64| mib * 1024 * 1024 / RECORD_BYTES as u64;
    let read = |rows| ReadPlan {
        rows,
        ending: Ending::Complete,
        slow: None,
        cancel_after: None,
    };
    let upload = |rows| UploadPlan {
        rows,
        fail_at: None,
        receiver_delay: None,
        cancel_after: None,
    };
    let mut all = Vec::new();
    all.push(jsonl_read("read-N", read(rows(mib))).await);
    all.push(jsonl_read("read-2N", read(rows(2 * mib))).await);
    all.push(
        jsonl_read(
            "read-slow-consumer",
            ReadPlan {
                slow: Some((10_000, Duration::from_millis(5))),
                ..read(rows(mib / 4))
            },
        )
        .await,
    );
    all.push(
        jsonl_read(
            "read-cancel",
            ReadPlan {
                cancel_after: Some(100_000),
                ..read(rows(mib))
            },
        )
        .await,
    );
    all.push(
        jsonl_read(
            "read-late-failure",
            ReadPlan {
                ending: Ending::FailAfter(rows(mib) / 2),
                ..read(rows(mib))
            },
        )
        .await,
    );
    let up = (mib / 4).max(1);
    all.push(jsonl_upload("upload-N", upload(rows(up))).await);
    all.push(jsonl_upload("upload-2N", upload(rows(2 * up))).await);
    all.push(
        jsonl_upload(
            "upload-slow-receiver",
            UploadPlan {
                receiver_delay: Some(Duration::from_micros(200)),
                ..upload(rows(up / 4).max(1))
            },
        )
        .await,
    );
    all.push(
        jsonl_upload(
            "upload-cancel",
            UploadPlan {
                receiver_delay: Some(Duration::from_millis(2)),
                cancel_after: Some(Duration::from_millis(500)),
                ..upload(rows(up))
            },
        )
        .await,
    );
    all.push(
        jsonl_upload(
            "upload-late-error",
            UploadPlan {
                fail_at: Some(rows(up) / 2),
                ..upload(rows(up))
            },
        )
        .await,
    );
    let json = serde_json::to_string_pretty(&all).unwrap();
    match out {
        Some(path) => std::fs::write(path, json).unwrap(),
        None => println!("{json}"),
    }
}
