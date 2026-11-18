//! C9.3, C9.5 and C12.6: `Error::request_not_sent` and `RenewalError::from_call` on real
//! connections, through each shipped middleware, on a later page, and in a refresh-token
//! exchange written as a Concord call.
#![cfg(all(
    feature = "rotating",
    feature = "sign",
    feature = "limit",
    feature = "json"
))]
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use concord::auth::{
    AccessToken, CredentialStore, Exchange, RenewalError, Renewed, Rotating, RotationError, Stored,
    TokenPair,
};
use concord::limit::Limit;
use concord::sign::{CanonicalPath, Credentials, SigV4};
use concord::{Base, Http, Middleware, Secret};
use concord_testkit::Server;
use std::collections::VecDeque;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(serde::Deserialize)]
struct Reply {
    access: String,
    refresh: String,
}

concord::api! {
    client Tokens {
        base "http://identity.test";
        POST exchange(refresh: Secret) -> Reply {
            path "token";
            body concord::form { "grant_type" = "refresh_token"; "refresh_token" = refresh; }
            read concord::Json;
        }
    }
}

concord::api! {
    client Data {
        base "http://data.test";
        read concord::Text;
        GET get() -> String { path "data"; }
    }
}

concord::api! {
    client Listing {
        base "http://listing.test";
        GET list(page?: u32) -> Vec<u32> {
            path "list";
            query { page; }
            read concord::Json;
            paginate concord::by_page(page, 1);
        }
    }
}

/// A local port with nothing listening on it.
fn closed_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

fn http() -> Http {
    Http::with(|builder| builder.no_proxy()).unwrap()
}

/// Resolves every host name to the next address of its list; the last one stays.
struct Addresses(Mutex<VecDeque<SocketAddr>>);

impl concord::reqwest::dns::Resolve for Addresses {
    fn resolve(&self, _: concord::reqwest::dns::Name) -> concord::reqwest::dns::Resolving {
        let mut addresses = self.0.lock().unwrap();
        let next = if addresses.len() > 1 {
            addresses.pop_front().unwrap()
        } else {
            addresses[0]
        };
        let found: concord::reqwest::dns::Addrs = Box::new(std::iter::once(next));
        Box::pin(std::future::ready(Ok(found)))
    }
}

/// `Http` that connects to these addresses in turn, one new connection for each request.
fn http_to(addresses: &[SocketAddr]) -> Http {
    let resolver = Arc::new(Addresses(Mutex::new(addresses.iter().copied().collect())));
    Http::with(move |builder| {
        builder
            .no_proxy()
            .pool_max_idle_per_host(0)
            .dns_resolver(resolver)
    })
    .unwrap()
}

#[derive(Clone)]
struct Memory(Arc<Mutex<Stored>>);

impl CredentialStore for Memory {
    async fn load(&self) -> Result<Stored, RenewalError> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn save(&self, record: Stored) -> Result<(), RenewalError> {
        *self.0.lock().unwrap() = record;
        Ok(())
    }
}

impl Memory {
    fn pair(&self) -> (String, String) {
        match &*self.0.lock().unwrap() {
            Stored::Ready(pair) => (
                pair.access().expose().to_string(),
                pair.refresh().expose().to_string(),
            ),
            Stored::Refreshing => panic!("the marker, not a pair"),
        }
    }
}

async fn exchange_through(middleware: Option<impl Middleware>, base: &str) -> concord::Error {
    let mut tokens = Tokens::new().with_base(base).with_transport(http());
    if let Some(middleware) = middleware {
        tokens = tokens.with_middleware(middleware);
    }
    match tokens.exchange(Secret::new("r0")).await {
        Ok(_) => panic!("nothing listens on the port"),
        Err(error) => error,
    }
}

fn assert_not_sent(error: concord::Error) {
    assert_eq!(
        error.class(),
        Some(concord::error::Class::Connect),
        "{error}"
    );
    assert_eq!(error.tries(), 1);
    assert!(error.request_not_sent(), "{error}");
    let error = error.map_problem(|()| ());
    assert!(error.request_not_sent());
    assert!(matches!(
        RenewalError::from_call(error),
        RenewalError::NotSent(_)
    ));
}

#[tokio::test]
async fn c9_5_each_shipped_middleware_passes_not_sent_on() {
    let url = format!("http://{}", closed_port());
    let base: Base = url.parse().unwrap();

    assert_not_sent(exchange_through(None::<Limit>, &url).await);
    assert_not_sent(exchange_through(Some(Limit::new()), &url).await);

    let renewed = Renewed::new(&base, || async {
        AccessToken::expires_in("a0", Duration::from_secs(3600))
    });
    assert_not_sent(exchange_through(Some(renewed), &url).await);

    let signer = SigV4::new(
        &base,
        Credentials::new("ACCESS", "secret-key"),
        "us-east-1",
        "sts",
        CanonicalPath::Preserve,
    );
    assert_not_sent(exchange_through(Some(signer), &url).await);

    let store = Memory(Arc::new(Mutex::new(Stored::Ready(TokenPair::expires_in(
        "a0",
        "r0",
        Duration::from_secs(3600),
    )))));
    let (rotating, driver) = Rotating::new(&base, store, |_: Exchange| async {
        Err::<TokenPair, _>(RenewalError::failed(std::io::Error::other("not called")))
    });
    let driver = tokio::spawn(driver);
    assert_not_sent(exchange_through(Some(rotating), &url).await);
    drop(driver);
}

#[tokio::test]
async fn c12_6_from_call_is_failed_when_the_server_may_have_read_the_request() {
    // The server reads the request and closes the connection without an answer.
    let server = Server::start().await;
    server.on("POST", "/token").close();
    let tokens = Tokens::new().with_base(server.url()).with_transport(http());
    let error = tokens.exchange(Secret::new("r0")).await.err().unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].body(),
        b"grant_type=refresh_token&refresh_token=r0"
    );
    assert_eq!(error.class(), Some(concord::error::Class::Connect));
    assert!(!error.request_not_sent());
    assert!(matches!(
        RenewalError::from_call(error),
        RenewalError::Failed(_)
    ));
}

#[tokio::test]
async fn c12_6_from_call_is_failed_for_a_later_page() {
    let server = Server::start().await;
    server
        .on("GET", "/list")
        .reply(200)
        .header("content-type", "application/json")
        .body("[1]");
    // The first page reaches the server, the second finds nothing listening.
    let listing = Listing::new().with_transport(http_to(&[server.addr(), closed_port()]));
    let mut pages = listing.list().pages();
    assert_eq!(pages.try_next().await.unwrap(), Some(vec![1]));
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(server.requests().len(), 1);
    assert_eq!(error.class(), Some(concord::error::Class::Connect));
    assert_eq!(error.tries(), 1);
    assert!(!error.request_not_sent(), "the first page was sent");
    assert!(matches!(
        RenewalError::from_call(error),
        RenewalError::Failed(_)
    ));

    // The same failure on the first page states it.
    let listing = Listing::new().with_transport(http_to(&[closed_port()]));
    let error = listing.list().pages().try_next().await.unwrap_err();
    assert!(error.request_not_sent());
}

#[tokio::test]
async fn c12_6_an_exchange_that_cannot_connect_keeps_the_refresh_token() {
    // The token endpoint is a Concord call over `Http`: its first connection is refused,
    // its second reaches the server.
    let identity = Server::start().await;
    identity
        .on("POST", "/token")
        .reply(200)
        .header("content-type", "application/json")
        .body(r#"{ "access": "a1", "refresh": "r1" }"#);
    let api = Server::start().await;
    api.on("GET", "/data").reply(200).body("ok");

    let tokens = Tokens::new().with_transport(http_to(&[closed_port(), identity.addr()]));
    let store = Memory(Arc::new(Mutex::new(Stored::Ready(TokenPair::new(
        "a0", "r0", None,
    )))));
    let base: Base = api.url().parse().unwrap();
    let (rotating, driver) = Rotating::new(&base, store.clone(), move |exchange: Exchange| {
        let tokens = tokens.clone();
        async move {
            let reply = tokens
                .exchange(exchange.refresh())
                .await
                .map_err(RenewalError::from_call)?;
            Ok(TokenPair::expires_in(
                reply.access,
                reply.refresh,
                Duration::from_secs(3600),
            ))
        }
    });
    let driver = tokio::spawn(driver);
    let data = Data::new()
        .with_base(api.url())
        .with_transport(http())
        .with_middleware(rotating);

    // The exchange is not sent: the call fails, and the store holds the same pair.
    let error = data.get().await.unwrap_err();
    match RotationError::find(&error) {
        Some(RotationError::Renewal(RenewalError::NotSent(cause))) => {
            let call = cause.downcast_ref::<concord::Error>().unwrap();
            assert!(call.request_not_sent());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(store.pair(), ("a0".into(), "r0".into()));
    assert!(identity.requests().is_empty());
    assert!(api.requests().is_empty());

    // The next call exchanges the same refresh token, once, and uses the new access token.
    assert_eq!(data.get().await.unwrap(), "ok");
    let exchanges = identity.requests();
    assert_eq!(exchanges.len(), 1);
    assert_eq!(
        exchanges[0].body(),
        b"grant_type=refresh_token&refresh_token=r0"
    );
    let calls = api.requests();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].header("authorization"), Some("Bearer a1"));
    assert_eq!(store.pair(), ("a1".into(), "r1".into()));
    drop(data);
    driver.await.unwrap().unwrap();
}

#[tokio::test]
async fn c12_6_an_exchange_that_the_server_read_leaves_the_marker() {
    // The token endpoint reads the exchange and closes the connection without an answer:
    // the refresh token may be consumed.
    let identity = Server::start().await;
    identity.on("POST", "/token").close();
    let api = Server::start().await;
    api.on("GET", "/data").reply(200).body("ok");

    let tokens = Tokens::new()
        .with_base(identity.url())
        .with_transport(http());
    let store = Memory(Arc::new(Mutex::new(Stored::Ready(TokenPair::new(
        "a0", "r0", None,
    )))));
    let base: Base = api.url().parse().unwrap();
    let (rotating, driver) = Rotating::new(&base, store.clone(), move |exchange: Exchange| {
        let tokens = tokens.clone();
        async move {
            let reply = tokens
                .exchange(exchange.refresh())
                .await
                .map_err(RenewalError::from_call)?;
            Ok(TokenPair::expires_in(
                reply.access,
                reply.refresh,
                Duration::from_secs(3600),
            ))
        }
    });
    let driver = tokio::spawn(driver);
    let data = Data::new()
        .with_base(api.url())
        .with_transport(http())
        .with_middleware(rotating);

    let error = data.get().await.unwrap_err();
    assert!(
        matches!(
            RotationError::find(&error),
            Some(RotationError::Renewal(RenewalError::Failed(_)))
        ),
        "{error}"
    );
    assert!(matches!(*store.0.lock().unwrap(), Stored::Refreshing));
    let error = data.get().await.unwrap_err();
    assert!(matches!(
        RotationError::find(&error),
        Some(RotationError::NeedsReauthorization)
    ));
    assert!(matches!(*store.0.lock().unwrap(), Stored::Refreshing));
    // One exchange reached the server in all, and no call reached the API.
    let exchanges = identity.requests();
    assert_eq!(exchanges.len(), 1);
    assert_eq!(
        exchanges[0].body(),
        b"grant_type=refresh_token&refresh_token=r0"
    );
    assert!(api.requests().is_empty());
    drop(data);
    assert!(driver.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_6_from_call_is_failed_after_more_than_one_attempt() {
    // Two attempts, neither of which was sent: the call states it, and `from_call` still asks
    // for one attempt.
    let script = concord_testkit::Script::new();
    script.fail(concord::transport::TransportError::connect().not_sent());
    script.fail(concord::transport::TransportError::connect().not_sent());
    let data = Data::new()
        .with_transport(script.clone())
        .with_retry(concord::Retry::standard().tries(2));
    let error = data.get().await.unwrap_err();
    assert_eq!(script.requests().len(), 2);
    assert_eq!(error.tries(), 2);
    assert!(error.request_not_sent());
    assert!(matches!(
        RenewalError::from_call(error),
        RenewalError::Failed(_)
    ));
}
