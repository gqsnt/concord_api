//! Runtime operation lookup (H08): one variant per endpoint, named by the endpoint's path below the
//! client in UpperCamelCase, under the endpoint's own and inherited `#[cfg]`s; `ALL`,
//! `operation()` and `from_operation()`; and each generated `send()` passing its endpoint's
//! identity to the client's `SendWith` policy. No request is sent.

use concord::{Endpoint, Operation, SendWith, Sends};

/// Returns the operation it was handed, and the endpoint's own identity, without sending.
#[derive(Default)]
struct Recorder;

impl<T: Send> SendWith<T> for Recorder {
    type Output = (__StoreOperation, Operation);
    async fn send<E: Endpoint<T> + Send>(&self, _endpoint: E) -> Self::Output {
        let operation = __StoreOperation::from_operation(E::operation()).unwrap();
        (operation, E::operation())
    }
}

concord::api! {
    /// A store.
    client Store {
        base "http://store.invalid";
        send Recorder;
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
        GET ping();
    }
}

// The rule the statement serves: classify without a wildcard arm.
fn is_write(operation: __StoreOperation) -> bool {
    match operation {
        __StoreOperation::Ping | __StoreOperation::ItemsList | __StoreOperation::ItemsItemGet => {
            false
        }
        __StoreOperation::ItemsItemRemove | __StoreOperation::ItemsCreate => true,
    }
}

#[test]
fn one_variant_per_compiled_endpoint_in_declaration_order() {
    assert_eq!(
        __StoreOperation::ALL,
        &[
            __StoreOperation::Ping,
            __StoreOperation::ItemsList,
            __StoreOperation::ItemsItemGet,
            __StoreOperation::ItemsItemRemove,
            __StoreOperation::ItemsCreate,
        ]
    );
    let names: Vec<String> = __StoreOperation::ALL
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
    assert_eq!(__OtherOperation::ALL, &[__OtherOperation::Ping]);
}

#[test]
fn variants_convert_to_and_from_the_endpoint_identity() {
    assert_eq!(__StoreOperation::Ping.operation(), StorePing::operation());
    assert_eq!(
        __StoreOperation::ItemsItemRemove.operation(),
        StoreItemsItemRemove::operation()
    );
    assert_eq!(
        __StoreOperation::ItemsCreate.operation().name(),
        "Store.items.create"
    );
    for &op in __StoreOperation::ALL {
        assert_eq!(__StoreOperation::from_operation(op.operation()), Some(op));
    }
    // Another client's endpoint of the same path, and a label of the same name, are not Store's.
    assert_eq!(
        __StoreOperation::from_operation(OtherPing::operation()),
        None
    );
    assert_eq!(
        __StoreOperation::from_operation(Operation::label("Store.ping")),
        None
    );
    assert_eq!(
        __OtherOperation::from_operation(OtherPing::operation()),
        Some(__OtherOperation::Ping)
    );
}

async fn via_sends<E: Sends<(__StoreOperation, Operation)>>(endpoint: E) -> __StoreOperation {
    endpoint.send().await.0
}

#[tokio::test]
async fn send_policy_looks_up_the_endpoint_variant_at_runtime() {
    let store = Store::new(concord::reqwest::Client::new());
    let (op, identity) = store.items().item(7).remove().send().await;
    assert_eq!(op, __StoreOperation::ItemsItemRemove);
    assert_eq!(identity, StoreItemsItemRemove::operation());
    assert!(is_write(op));
    assert_eq!(store.ping().send().await.0, __StoreOperation::Ping);
    assert_eq!(
        via_sends(store.items().item(1).get()).await,
        __StoreOperation::ItemsItemGet
    );
    assert_eq!(
        via_sends(store.items().create()).await,
        __StoreOperation::ItemsCreate
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

// Two clients with the same name, in different modules.
mod first {
    concord::api! {
        pub client Twin {
            base "http://first.invalid";
            GET ping();
        }
    }
}

mod second {
    concord::api! {
        pub client Twin {
            base "http://second.invalid";
            GET ping();
            POST other();
        }
    }
}

/// `from_operation` compares the declared identity textually (`Operation`'s `==`: endpoint,
/// dotted name, method): an operation of another client with the same dotted name and method is
/// indistinguishable, as documented; another name or method is not.
#[test]
fn from_operation_compares_the_declared_name_and_method_only() {
    assert_eq!(
        first::__TwinOperation::from_operation(second::TwinPing::operation()),
        Some(first::__TwinOperation::Ping)
    );
    assert_eq!(
        second::__TwinOperation::from_operation(first::TwinPing::operation()),
        Some(second::__TwinOperation::Ping)
    );
    assert_eq!(
        first::__TwinOperation::from_operation(second::TwinOther::operation()),
        None
    );
}
