//! Between waves 3 and 4: an endpoint's declared identity without naming its output.
//! Witness: the Square maintainer compared `<SquarePaymentsCreate as Endpoint<PaymentResponse>>::operation()`;
//! the declared-value experiment (b34-values) kept the inherent `operation()` in both variants.

use concord::{Endpoint, Json};
use serde::Deserialize;

/// An item.
#[derive(Debug, Deserialize)]
pub struct Item {
    /// Its id.
    pub id: u32,
}

concord::api! {
    /// A store.
    pub client Store() {
        base "http://store.invalid";
        format Json;
        /// One item.
        GET item(id: u32) -> Item { path "items/{id}"; }
        /// Removes one.
        DELETE remove(id: u32) { path "items/{id}"; }
    }
}

#[test]
fn the_inherent_operation_is_the_endpoint_identity() {
    // Same identity through the inherent method, the trait (with its output) and a prepared request.
    assert_eq!(
        StoreItem::operation(),
        <StoreItem as Endpoint<Item>>::operation()
    );
    assert_eq!(
        StoreRemove::operation(),
        <StoreRemove as Endpoint<()>>::operation()
    );
    assert_ne!(StoreItem::operation(), StoreRemove::operation());
    let store = Store::new(concord::reqwest::Client::new());
    let prepared = store.item(1).prepare().unwrap();
    assert_eq!(prepared.operation(), Some(StoreItem::operation()));
}
