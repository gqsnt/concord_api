//! E1: whether a declared operation only reads, on `Operation::is_read()`. `GET`, `HEAD` and
//! `OPTIONS` read; other methods do not, unless the endpoint or an enclosing scope is `#[read]`.
//! The fact is on both forms of a declared identity (the type's `operation()` and the endpoint
//! metadata every carrier holds) and survives `Prepared`, `Decoder`, `Received`, `Incoming`,
//! `ResponseMeta` and errors, and policies of either kind read it.

mod support;

use std::cell::Cell;

use concord::{Endpoint, ErrorKind, Operation, SendWith, reqwest};
use support::{Reply, Server};

concord::api! {
    pub client Shop {
        base "http://shop.invalid";
        GET get();
        HEAD head();
        OPTIONS options();
        POST create();
        PUT put();
        PATCH patch();
        DELETE delete();
        #[read] POST search();
        #[read]
        scope account {
            path "account";
            POST balance();
            GET status();
            scope history {
                POST trades();
            }
        }
        scope orders {
            path "orders";
            POST place();
            #[cfg(any())]
            #[read] POST hidden();
        }
        #[cfg(all())]
        #[read]
        scope reports {
            POST run(header("x-tag") tag: String);
        }
    }
}

fn reads<E: Endpoint<T>, T>(_: &E) -> bool {
    E::operation().is_read()
}

#[test]
fn methods_give_the_default_and_read_marks_the_rest() {
    use __ShopOperation as Op;
    let expected = [
        (Op::Get, true),
        (Op::Head, true),
        (Op::Options, true),
        (Op::Create, false),
        (Op::Put, false),
        (Op::Patch, false),
        (Op::Delete, false),
        (Op::Search, true),
        (Op::AccountBalance, true),
        (Op::AccountStatus, true),
        (Op::AccountHistoryTrades, true),
        (Op::OrdersPlace, false),
        (Op::ReportsRun, true),
    ];
    // The `cfg(any())` endpoint has no variant; the `cfg(all())` scope keeps its fact.
    assert_eq!(Op::ALL.len(), expected.len());
    for (variant, read) in expected {
        assert_eq!(variant.operation().is_read(), read, "{variant:?}");
        assert_eq!(Op::from_operation(variant.operation()), Some(variant));
    }
    // A label claims nothing.
    assert!(!Operation::label("native download").is_read());
}

/// Both representations agree: the type's `operation()` (no request built) and the endpoint
/// metadata a `Prepared` carries, through `into_parts()` to the `Decoder`.
#[test]
fn the_declared_identity_and_the_prepared_metadata_agree() {
    let shop = Shop::new(reqwest::Client::new());
    fn check<T, E: Endpoint<T>>(endpoint: E, read: bool) {
        assert_eq!(E::operation().is_read(), read);
        let prepared = endpoint.prepare().unwrap();
        assert_eq!(prepared.operation(), Some(E::operation()));
        assert_eq!(prepared.operation().unwrap().is_read(), read);
        let (_, request, decoder) = prepared.into_parts();
        assert_eq!(decoder.operation().unwrap().is_read(), read);
        // `#[read]` changes no request byte: the method stays as written.
        assert_eq!(request.method().as_str(), E::operation().method().unwrap());
    }
    check(shop.get(), true);
    check(shop.create(), false);
    check(shop.search(), true);
    check(shop.account().balance(), true);
    check(shop.account().history().trades(), true);
    check(shop.orders().place(), false);
    check(shop.reports().run(String::from("t")), true);
    assert!(reads(&shop.search()));
}

/// A build failure happens before any carrier exists but still names the operation with its fact.
#[test]
fn a_build_error_of_a_read_post_carries_the_fact() {
    let shop = Shop::new(reqwest::Client::new());
    let error = shop
        .reports()
        .run(String::from("bad\nvalue"))
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(error.operation().unwrap().is_read());
    assert_eq!(error.operation(), Some(ShopReportsRun::operation()));
}

#[tokio::test]
async fn the_fact_survives_receiving_metadata_and_errors() {
    let server = Server::start(|request| match request.path() {
        // `account.balance` has no path of its own: `/account`.
        "/account" => Reply::text(200, "ok"),
        _ => Reply::text(500, "no"),
    })
    .await;
    let shop = Shop::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();

    // A read POST: received, its owned metadata, its unread body.
    let received = shop
        .account()
        .balance()
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap();
    assert!(received.operation().unwrap().is_read());
    assert!(received.meta().operation().unwrap().is_read());
    let incoming = received.into_incoming().unwrap();
    assert!(incoming.operation().unwrap().is_read());

    // A write POST answered 500: the status error and its metadata.
    let error = shop.orders().place().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert!(!error.operation().unwrap().is_read());
    assert!(!error.meta().unwrap().operation().unwrap().is_read());

    // A read POST answered 500.
    let error = shop.search().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert!(error.operation().unwrap().is_read());
    assert!(error.meta().unwrap().operation().unwrap().is_read());
    assert_eq!(server.requests().len(), 3);

    // A transport failure: no response, the fact is still there.
    let refused = Shop::new(reqwest::Client::new())
        .with_base_url(format!("http://{}", support::refused_addr()))
        .unwrap();
    let error = refused
        .account()
        .history()
        .trades()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.operation().unwrap().is_read());
}

// A `SendWith` policy (here not `Sync`) reads the fact from `E::operation()`; the private
// inventory lets another policy classify it exhaustively.

/// Not `Sync` (a `Cell`): nothing new is required of a policy.
#[derive(Default)]
pub struct Plain {
    _not_sync: Cell<u8>,
}

impl<T: Send> SendWith<T> for Plain {
    type Output = (bool, &'static str);
    fn send<E>(&self, _endpoint: E) -> impl Future<Output = Self::Output> + Send
    where
        E: Endpoint<T> + Send,
    {
        let operation = E::operation();
        async move { (operation.is_read(), operation.name()) }
    }
}

concord::api! {
    pub client Simple {
        base "http://simple.invalid";
        send Plain;
        GET one();
        #[read] POST two();
        DELETE three();
    }
}

/// Classifies the runtime endpoint identity.
#[derive(Default)]
struct Classifying;

impl<T: Send> SendWith<T> for Classifying {
    type Output = (__TaggedOperation, bool);
    async fn send<E: Endpoint<T> + Send>(&self, _endpoint: E) -> Self::Output {
        let operation = __TaggedOperation::from_operation(E::operation()).unwrap();
        (operation, E::operation().is_read())
    }
}

concord::api! {
    client Tagged {
        base "http://tagged.invalid";
        send Classifying;
        GET one();
        POST two();
    }
}

concord::api! {
    pub client Bare {
        base "http://bare.invalid";
        GET one();
    }
}

#[tokio::test]
async fn both_runtime_policy_styles_read_the_fact() {
    let simple = Simple::new(reqwest::Client::new());
    assert_eq!(simple.one().send().await, (true, "Simple.one"));
    assert_eq!(simple.two().send().await, (true, "Simple.two"));
    assert_eq!(simple.three().send().await, (false, "Simple.three"));

    let tagged = Tagged::new(reqwest::Client::new());
    assert_eq!(tagged.one().send().await, (__TaggedOperation::One, true));
    assert_eq!(tagged.two().send().await, (__TaggedOperation::Two, false));

    // Without a send policy `send()` is one ordinary execution; the fact is on the type.
    assert!(BareOne::operation().is_read());
    let server = Server::reply(Reply::new(204)).await;
    let bare = Bare::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    bare.one().send().await.unwrap();
    assert_eq!(server.requests().len(), 1);
}
