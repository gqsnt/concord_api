//! CI-sized runs of every workload, asserting the invariants the large runs measure.

use std::time::Duration;

use concord_check_stream_stress::*;

fn plan(rows: u64) -> ReadPlan {
    ReadPlan {
        rows,
        ending: Ending::Complete,
        slow: None,
        cancel_after: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jsonl_read_complete_bounded_and_in_order() {
    let m = jsonl_read("smoke-read", plan(20_000)).await;
    assert_eq!(m.records_delivered, 20_000);
    assert_eq!(m.bytes_consumed, 20_000 * RECORD_BYTES as u64);
    assert!(m.peak_retained_bytes <= MAX_RECORD_BYTES + 1, "{m:?}");
    assert!(m.error_kind.is_none(), "{m:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jsonl_read_late_transport_failure_keeps_the_prefix() {
    let m = jsonl_read(
        "smoke-late-failure",
        ReadPlan {
            ending: Ending::FailAfter(1_000),
            ..plan(20_000)
        },
    )
    .await;
    assert_eq!(m.records_delivered, 1_000, "{m:?}");
    assert_eq!(m.error_kind.as_deref(), Some("Transport"), "{m:?}");
    assert_eq!(m.error_after_records, Some(1_000));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jsonl_read_cancellation_stops_the_server_body() {
    let m = jsonl_read(
        "smoke-cancel",
        ReadPlan {
            cancel_after: Some(500),
            ..plan(2_000_000)
        },
    )
    .await;
    assert_eq!(m.records_delivered, 500);
    assert!(m.notes.iter().any(|n| n.contains("promptly")), "{m:?}");
    assert_eq!(
        m.progress_at_cancel, m.progress_after_cancel,
        "generation continued: {m:?}"
    );
    // Far less than the 256 MB the request asked for was ever generated.
    assert!(m.bytes_generated < 64 * 1024 * 1024, "{m:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jsonl_upload_polls_zero_times_before_send_and_delivers_everything() {
    let m = jsonl_upload(
        "smoke-upload",
        UploadPlan {
            rows: 20_000,
            fail_at: None,
            receiver_delay: None,
            cancel_after: None,
        },
    )
    .await;
    assert_eq!(m.source_polls_before_send, Some(0));
    assert!(m.error_kind.is_none(), "{m:?}");
    assert_eq!(m.bytes_consumed, 20_000 * RECORD_BYTES as u64);
    // One pass: every record polled once, plus the final `None`.
    assert_eq!(m.source_polls_total, Some(20_000));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jsonl_upload_late_source_error_fails_the_send_after_a_prefix() {
    let m = jsonl_upload(
        "smoke-upload-late-error",
        UploadPlan {
            rows: 20_000,
            fail_at: Some(5_000),
            receiver_delay: None,
            cancel_after: None,
        },
    )
    .await;
    assert_eq!(m.source_polls_before_send, Some(0));
    assert_eq!(m.error_kind.as_deref(), Some("Transport"), "{m:?}");
    assert_eq!(
        m.source_polls_total,
        Some(5_001),
        "the source is not polled after its error"
    );
    assert!(m.bytes_consumed <= 5_000 * RECORD_BYTES as u64, "{m:?}");
    assert_eq!(
        m.bytes_consumed % RECORD_BYTES as u64,
        0,
        "only whole records were sent"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jsonl_upload_cancellation_stops_polling_the_source() {
    let m = jsonl_upload(
        "smoke-upload-cancel",
        UploadPlan {
            rows: 10_000_000,
            fail_at: None,
            receiver_delay: Some(Duration::from_millis(2)),
            cancel_after: Some(Duration::from_millis(300)),
        },
    )
    .await;
    let (at, later) = (
        m.progress_at_cancel.unwrap(),
        m.progress_after_cancel.unwrap(),
    );
    assert_eq!(
        at, later,
        "the source was polled after the send was cancelled: {m:?}"
    );
    assert!(at < 10_000_000, "{m:?}");
}
