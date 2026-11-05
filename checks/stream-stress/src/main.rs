//! `cargo run --release -p concord-check-stream-stress -- [--mib N] [--format jsonl|csv|both] [--out FILE]`
//!
//! Runs, per format, the N/2N read workloads (N MiB of decoded records, default 256), the N/2N
//! upload workloads (N/4 and N/2 MiB), and the slow, cancelled and failing variants, and prints
//! one JSON array of measurements (with the source commit when `CONCORD_SOURCE_SHA` is set).

use std::time::Duration;

use concord_check_stream_stress::*;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let mut mib: u64 = 256;
    let mut formats = vec![Format::Jsonl, Format::Csv];
    let mut out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mib" => mib = args.next().and_then(|v| v.parse().ok()).expect("--mib N"),
            "--format" => {
                formats = match args.next().as_deref() {
                    Some("jsonl") => vec![Format::Jsonl],
                    Some("csv") => vec![Format::Csv],
                    Some("both") => vec![Format::Jsonl, Format::Csv],
                    other => panic!("--format jsonl|csv|both, not {other:?}"),
                }
            }
            "--out" => out = args.next(),
            other => panic!("unknown argument {other}"),
        }
    }
    let rows = |mib: u64| mib * 1024 * 1024 / RECORD_BYTES as u64;
    let plan = |rows| ReadPlan {
        rows,
        ending: Ending::Complete,
        slow: None,
        cancel_after: None,
    };
    let up = |rows| UploadPlan {
        rows,
        fail_at: None,
        receiver_delay: None,
        cancel_after: None,
    };
    let mut all = Vec::new();
    for format in formats {
        let tag = |s: &str| format!("{}-{s}", format.name());
        all.push(read(format, &tag("read-N"), plan(rows(mib))).await);
        all.push(read(format, &tag("read-2N"), plan(rows(2 * mib))).await);
        let slow = ReadPlan {
            slow: Some((10_000, Duration::from_millis(5))),
            ..plan(rows(mib / 4))
        };
        all.push(read(format, &tag("read-slow-consumer"), slow).await);
        let cancel = ReadPlan {
            cancel_after: Some(100_000),
            ..plan(rows(mib))
        };
        all.push(read(format, &tag("read-cancel"), cancel).await);
        let failing = ReadPlan {
            ending: Ending::FailAfter(rows(mib) / 2),
            ..plan(rows(mib))
        };
        all.push(read(format, &tag("read-late-failure"), failing).await);
        let n = (mib / 4).max(1);
        all.push(upload(format, &tag("upload-N"), up(rows(n))).await);
        all.push(upload(format, &tag("upload-2N"), up(rows(2 * n))).await);
        let slow = UploadPlan {
            receiver_delay: Some(Duration::from_micros(200)),
            ..up(rows((n / 4).max(1)))
        };
        all.push(upload(format, &tag("upload-slow-receiver"), slow).await);
        let cancel = UploadPlan {
            receiver_delay: Some(Duration::from_millis(2)),
            cancel_after: Some(Duration::from_millis(500)),
            ..up(rows(n))
        };
        all.push(upload(format, &tag("upload-cancel"), cancel).await);
        let failing = UploadPlan {
            fail_at: Some(rows(n) / 2),
            ..up(rows(n))
        };
        all.push(upload(format, &tag("upload-late-error"), failing).await);
    }
    let doc = serde_json::json!({
        "source_sha": std::env::var("CONCORD_SOURCE_SHA").ok(),
        "mib": mib,
        "record_bytes": RECORD_BYTES,
        "max_record_bytes": MAX_RECORD_BYTES,
        "generator_chunk_bytes": CHUNK_BYTES,
        "platform": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "measurements": all,
    });
    let json = serde_json::to_string_pretty(&doc).unwrap();
    match out {
        Some(path) => std::fs::write(path, json).unwrap(),
        None => println!("{json}"),
    }
}
