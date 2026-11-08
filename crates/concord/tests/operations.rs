//! `operations Name;` (H08): one variant per endpoint, named by the endpoint's path below the
//! client in UpperCamelCase, under the endpoint's own and inherited `#[cfg]`s; `ALL`,
//! `operation()` and `from_operation()`; and each generated `send()` passing its endpoint's
//! variant to the client's `SendOperation` policy. No request is sent.

use concord::{Endpoint, Operation, SendOperation, Sends};

/// Returns the operation it was handed, and the endpoint's own identity, without sending.
#[derive(Default)]
pub struct Recorder;

impl<T: Send> SendOperation<T, StoreOperation> for Recorder {
    type Output = (StoreOperation, Operation);
    async fn send<E: Endpoint<T> + Send>(
        &self,
        operation: StoreOperation,
        _endpoint: E,
    ) -> Self::Output {
        (operation, E::operation())
    }
}

concord::api! {
    /// A store.
    pub client Store {
        base "http://store.invalid";
        send Recorder;
        /// The store's operations, as its policy classifies them.
        operations StoreOperation;
        GET ping();
        scope items {
            path "items";
            GET list();
            scope item(id: u64) {
                path "{id}";
                GET get();
                DELETE remove();
                #[cfg(any())]
                PATCH never();
            }
            #[cfg(all())]
            POST create();
        }
        #[cfg(any())]
        scope hidden {
            GET gone();
        }
    }
}

concord::api! {
    pub client Other {
        base "http://other.invalid";
        operations OtherOperation;
        GET ping();
    }
}

// The rule the statement serves: classify without a wildcard arm.
fn is_write(operation: StoreOperation) -> bool {
    match operation {
        StoreOperation::Ping | StoreOperation::ItemsList | StoreOperation::ItemsItemGet => false,
        StoreOperation::ItemsItemRemove | StoreOperation::ItemsCreate => true,
    }
}

#[test]
fn one_variant_per_compiled_endpoint_in_declaration_order() {
    assert_eq!(
        StoreOperation::ALL,
        &[
            StoreOperation::Ping,
            StoreOperation::ItemsList,
            StoreOperation::ItemsItemGet,
            StoreOperation::ItemsItemRemove,
            StoreOperation::ItemsCreate,
        ]
    );
    let names: Vec<String> = StoreOperation::ALL
        .iter()
        .map(|op| format!("{op:?}"))
        .collect();
    assert_eq!(
        names,
        [
            "Ping",
            "ItemsList",
            "ItemsItemGet",
            "ItemsItemRemove",
            "ItemsCreate"
        ]
    );
    assert_eq!(OtherOperation::ALL, &[OtherOperation::Ping]);
}

#[test]
fn variants_convert_to_and_from_the_endpoint_identity() {
    assert_eq!(StoreOperation::Ping.operation(), StorePing::operation());
    assert_eq!(
        StoreOperation::ItemsItemRemove.operation(),
        StoreItemsItemRemove::operation()
    );
    assert_eq!(
        StoreOperation::ItemsCreate.operation().name(),
        "Store.items.create"
    );
    for &op in StoreOperation::ALL {
        assert_eq!(StoreOperation::from_operation(op.operation()), Some(op));
    }
    // Another client's endpoint of the same path, and a label of the same name, are not Store's.
    assert_eq!(StoreOperation::from_operation(OtherPing::operation()), None);
    assert_eq!(
        StoreOperation::from_operation(Operation::label("Store.ping")),
        None
    );
    assert_eq!(
        OtherOperation::from_operation(OtherPing::operation()),
        Some(OtherOperation::Ping)
    );
}

async fn via_sends<E: Sends<(StoreOperation, Operation)>>(endpoint: E) -> StoreOperation {
    endpoint.send().await.0
}

#[tokio::test]
async fn send_passes_the_endpoint_variant_to_the_policy() {
    let store = Store::new(concord::reqwest::Client::new());
    let (op, identity) = store.items().item(7).remove().send().await;
    assert_eq!(op, StoreOperation::ItemsItemRemove);
    assert_eq!(identity, StoreItemsItemRemove::operation());
    assert!(is_write(op));
    assert_eq!(store.ping().send().await.0, StoreOperation::Ping);
    assert_eq!(
        via_sends(store.items().item(1).get()).await,
        StoreOperation::ItemsItemGet
    );
    assert_eq!(
        via_sends(store.items().create()).await,
        StoreOperation::ItemsCreate
    );
    // `prepare()` is unchanged: the declared request, without the policy.
    let prepared = store.items().item(7).remove().prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "http://store.invalid/items/7"
    );
    assert_eq!(
        prepared.operation(),
        Some(StoreItemsItemRemove::operation())
    );
}
