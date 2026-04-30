mod support;

#[test]
fn retry_runtime_harness_can_queue_transport_responses() {
    let transport = support::MockTransport::default();
    transport.push(support::MockResponse::text(500, Vec::new()));
    assert_eq!(transport.next().expect("queued response").status, 500);
}

#[tokio::test]
async fn deterministic_sleeper_records_without_waiting() {
    let sleeper = support::DeterministicSleeper::default();
    sleeper.sleep(std::time::Duration::from_millis(250)).await;
    support::assert_event_order(&sleeper.events.snapshot(), &["sleep_ms:250"]);
}
