//! Isolated process: compare RSS before/after encoding with a 64-byte row cap.
use concord::{
    csv::{CsvOptions, CsvWriter, HeaderMode, Rows, StreamRows},
    stream::from_iter,
};
use http_body::Body;
use std::{convert::Infallible, future::poll_fn};

fn hwm_kib() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let streaming = std::env::args().any(|a| a == "stream");
    let row = "x".repeat(16 * 1024 * 1024);
    let writer =
        CsvWriter::new(CsvOptions::new(HeaderMode::Absent, b',').max_record_bytes(64)).unwrap();
    let before = hwm_kib();
    if streaming {
        let upload = writer
            .upload(StreamRows::new(from_iter([Ok::<_, Infallible>((row,))])))
            .unwrap();
        let (body, _) = upload.into_parts();
        let mut body = Box::pin(body);
        let error = poll_fn(|cx| body.as_mut().poll_frame(cx))
            .await
            .unwrap()
            .unwrap_err();
        println!("streamed refusal: {error}");
    } else {
        let error = writer.to_vec(Rows::new([(&row,)])).unwrap_err();
        println!("finite refusal: {error}");
    }
    let after = hwm_kib();
    println!(
        "stream={streaming}, limit=64 bytes, input=16777216 bytes, before VmHWM={before} KiB, after={after} KiB, growth={} KiB",
        after.saturating_sub(before)
    );
    assert!(
        after > before + 16 * 1024,
        "library should expose its large transient allocation in this isolated process"
    );
}
