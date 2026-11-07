//! Between waves 2 and 3, candidate A2: `send P -> Result<_, Error>;` declares the result of
//! every generated `send()`, so its signature (and rustdoc) names `Result<Item, Error>` instead
//! of `<P as SendWith<Item>>::Output`. Witnesses: seven consumers across waves 1–2 had to find the
//! policy's `SendWith` implementation to learn what `send()` returns.

use concord::{Endpoint, Json, Received, SendWith, Sends};
use concord_example_testserver::{Reply, TestServer};
use serde::Deserialize;

/// An item.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Item {
    /// Its id.
    pub id: u32,
}

/// The SDK's error.
#[derive(Debug)]
pub enum Error {
    /// Concord's error.
    Concord(concord::Error),
    /// A non-2xx answer.
    Status(u16),
}

impl From<concord::Error> for Error {
    fn from(e: concord::Error) -> Self {
        Self::Concord(e)
    }
}

/// A policy mapping statuses to the SDK's error.
#[derive(Clone, Copy, Debug, Default)]
pub struct Policy;

impl<T: Send> SendWith<T> for Policy {
    type Output = Result<T, Error>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received: Received<T> = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            return Err(Error::Status(received.status().as_u16()));
        }
        Ok(received.decode().await?)
    }
}

concord::api! {
    /// A store with a declared send result.
    pub client Store() {
        base "http://store.invalid";
        format Json;
        send Policy -> Result<_, Error>;
        /// One item.
        GET item(id: u32) -> Item { path "items/{id}"; }
        /// Every item.
        GET items() -> Vec<Item> { path "items"; }
        /// No typed output.
        DELETE remove(id: u32) { path "items/{id}"; }
    }
}

/// The declared type is the signature's own: these compile only if `send()` returns it.
fn _names_the_result(store: &Store) {
    let _: std::pin::Pin<Box<dyn std::future::Future<Output = Result<Item, Error>> + Send>> =
        Box::pin(store.item(1).send());
    let _: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Error>> + Send>> =
        Box::pin(store.remove(1).send());
}

/// Generic SDK code still reaches the policy through `Sends` with the declared type.
async fn through_sends<E: Sends<Result<Vec<Item>, Error>>>(endpoint: E) -> usize {
    endpoint.send().await.map(|v| v.len()).unwrap_or(0)
}

#[tokio::test]
async fn the_declared_result_is_the_policy_output() {
    let server = TestServer::start(|r| {
        if r.path().ends_with("/2") {
            Reply::json(404, "{}")
        } else if r.path() == "/items" {
            Reply::json(200, r#"[{"id":1},{"id":2}]"#)
        } else {
            Reply::json(200, r#"{"id":1}"#)
        }
    })
    .await;
    let store = Store::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(store.item(1).send().await.unwrap(), Item { id: 1 });
    assert!(matches!(
        store.item(2).send().await,
        Err(Error::Status(404))
    ));
    assert_eq!(through_sends(store.items()).await, 2);
}

// Review b23-review-sol, finding 3: a template forwarded through `macro_rules!`.
macro_rules! forwarded_store {
    ($error:ty) => {
        concord::api! {
            /// A client whose send result is written by a wrapper.
            pub client Forwarded() {
                base "http://forwarded.invalid";
                format Json;
                send Policy -> Result<_, $error>;
                /// One item.
                GET item(id: u32) -> Item { path "items/{id}"; }
            }
        }
    };
}
forwarded_store!(Error);

fn _forwarded_names_the_result(store: &Forwarded) {
    let _: std::pin::Pin<Box<dyn std::future::Future<Output = Result<Item, Error>> + Send>> =
        Box::pin(store.item(1).send());
}

// Review b23-review2-sol, finding 3: a type macro's `_` is the output under every delimiter.
macro_rules! result {
    ($t:ty) => {
        Result<$t, Error>
    };
}
concord::api! {
    /// A client whose send result is a brace-delimited type macro.
    pub client Braced() {
        base "http://braced.invalid";
        format Json;
        send Policy -> result!{_};
        /// One item.
        GET item(id: u32) -> Item { path "items/{id}"; }
    }
}

fn _braced_names_the_result(store: &Braced) {
    let _: std::pin::Pin<Box<dyn std::future::Future<Output = Result<Item, Error>> + Send>> =
        Box::pin(store.item(1).send());
}

// Review b23-review-sol, finding 3 (precedence): a forwarded expression keeps its grouping inside
// a const argument of the template, so `{ $e * 2 }` with `$e = 1 + 2` is 6, not 5.
/// A result carrying a size.
pub struct Envelope<T, const N: usize>(pub T);

/// A policy whose output is an `Envelope` of size 6.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sized6;

impl<T: Send> SendWith<T> for Sized6 {
    type Output = Envelope<Result<T, Error>, 6>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        Envelope(Policy.send(endpoint).await)
    }
}

macro_rules! scaled_store {
    ($e:expr) => {
        concord::api! {
            /// A client whose template has a forwarded const expression.
            pub client Scaled() {
                base "http://scaled.invalid";
                format Json;
                send Sized6 -> Envelope<Result<_, Error>, { $e * 2 }>;
                /// One item.
                GET item(id: u32) -> Item { path "items/{id}"; }
            }
        }
    };
}
scaled_store!(1 + 2);

fn _scaled_names_the_result(store: &Scaled) {
    /// The future a `Scaled` endpoint's `send()` must return.
    type ScaledSend = std::pin::Pin<
        Box<dyn std::future::Future<Output = Envelope<Result<Item, Error>, 6>> + Send>,
    >;

    let _: ScaledSend = Box::pin(store.item(1).send());
}
