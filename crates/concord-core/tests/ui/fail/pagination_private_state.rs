use concord_core::contracts::{CallOptions, CollectPrefix, Pages, RequestControls};

// A lazy traversal carries captured controls: it exposes neither its state
// nor any timeout/cancellation/caps setter, and does not implement
// `RequestControls` (collectors do).
fn lazy_traversal_has_no_controls(pages: Pages<u64>) {
    let _ = &pages.traversal;
    let _ = pages.timeout(std::time::Duration::from_secs(1));
    let _ = pages.with_options(&CallOptions::new());
}

fn collector_state_is_private(collector: CollectPrefix<u64>) {
    let _ = &collector.collection;
    requires_controls(collector);
}

fn requires_controls<R: RequestControls>(_: R) {}

fn main() {}
