mod support;

#[test]
fn pagination_runtime_harness_records_page_events() {
    let events = support::EventRecorder::default();
    events.record("page_start");
    events.record("page_decoded");
    support::assert_event_order(&events.snapshot(), &["page_start", "page_decoded"]);
}
