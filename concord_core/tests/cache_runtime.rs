mod support;

#[test]
fn cache_runtime_harness_is_available() {
    let cache = support::FakeCache::default();
    cache.events.record("cache_lookup");
    support::assert_event_order(&cache.events.snapshot(), &["cache_lookup"]);
}
