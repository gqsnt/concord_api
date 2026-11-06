//! F3 follow-up from the first-use probe: SDK code written once over several endpoints must send
//! through the client's policy. Baseline: a helper bound on `Endpoint<T>` reaches only
//! `prepare()`, so it re-implements or is handed the policy (with `send signer;`, the caller has
//! to pass the client's signer in, and nothing ties it to the client). Candidate: generated
//! `Sends<O>`, whose `send()` is the endpoint's own policy-selected `send()`.

use concord::{Endpoint, Json, SendWith, Sends};
use concord_example_testserver::{Reply, TestServer};
use serde::Deserialize;

/// One page.
#[derive(Debug, Clone, Deserialize)]
pub struct Page {
    /// Item ids.
    pub items: Vec<u64>,
    /// The next cursor.
    pub next: Option<String>,
}

/// The SDK's result: the value and the request id.
#[derive(Debug)]
pub struct Response<T> {
    /// The decoded value.
    pub value: T,
    /// `x-request-id`.
    pub request_id: Option<String>,
}

/// The SDK's error (boxed Concord error).
#[derive(Debug)]
pub struct SdkError(pub Box<concord::Error>);

impl From<concord::Error> for SdkError {
    fn from(error: concord::Error) -> Self {
        Self(Box::new(error))
    }
}

/// A value policy: a key held by the client, applied to every request it sends.
#[derive(Clone)]
pub struct Signer {
    key: &'static str,
}

impl<T: Send> SendWith<T> for Signer {
    type Output = Result<Response<T>, SdkError>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let (client, mut request, decoder) = endpoint.prepare()?.into_parts();
        let mut signature = concord::reqwest::header::HeaderValue::from_static(self.key);
        signature.set_sensitive(true);
        request.headers_mut().insert("x-signature", signature);
        let received = decoder.execute(&client, request).await?;
        let request_id = received
            .head()
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let value = received.decode().await?;
        Ok(Response { value, request_id })
    }
}

concord::api! {
    /// A signed listing API.
    pub client Ledger(signer: Signer) {
        base "http://ledger.invalid";
        format Json;
        send signer;

        params Paging(query cursor?: String, query limit?: u32);

        GET entries(..Paging) -> Page { path "entries"; }
        GET archived(query("year") year: u32, ..Paging) -> Page { path "archive"; }
    }
}

/// Written once for every paged endpoint: sends through the client's policy (the signer comes
/// with the endpoint), no policy argument.
async fn all_pages<E>(first: E, cap: usize) -> Result<(Vec<u64>, Vec<Option<String>>), SdkError>
where
    E: Sends<Result<Response<Page>, SdkError>> + Paging + Clone,
{
    let mut items = Vec::new();
    let mut ids = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..cap {
        let page = first.clone().maybe_cursor(cursor.take()).send().await?;
        items.extend(page.value.items);
        ids.push(page.request_id);
        match page.value.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok((items, ids))
}

fn pages(r: &concord_example_testserver::Recorded) -> Reply {
    assert_eq!(r.header("x-signature"), Some("k-1"), "every page is signed");
    match r.query_pairs().iter().find(|(k, _)| k == "cursor") {
        None => Reply::json(200, br#"{"items":[1,2],"next":"c2"}"#).header("x-request-id", "a"),
        Some((_, c)) if c == "c2" => {
            Reply::json(200, br#"{"items":[3],"next":null}"#).header("x-request-id", "b")
        }
        Some(_) => Reply::new(400),
    }
}

#[tokio::test]
async fn a_generic_helper_sends_every_page_through_the_policy() {
    let server = TestServer::start(pages).await;
    let ledger = Ledger::new(
        concord::reqwest::Client::new(),
        LedgerConfig {
            signer: Signer { key: "k-1" },
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let (items, ids) = all_pages(ledger.entries().set_limit(2u32), 10)
        .await
        .unwrap();
    assert_eq!(items, [1, 2, 3]);
    assert_eq!(ids, [Some("a".to_owned()), Some("b".to_owned())]);
    let (items, _) = all_pages(ledger.archived(2025), 10).await.unwrap();
    assert_eq!(items, [1, 2, 3]);
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[1].query(), Some("cursor=c2&limit=2"));
    assert_eq!(requests[3].query(), Some("year=2025&cursor=c2"));
}

/// The helper's future is `Send` in generic code (a spawned traversal).
#[test]
fn the_generic_future_is_send() {
    fn assert_send<F: Send>(_: F) {}
    fn check<E>(first: E)
    where
        E: Sends<Result<Response<Page>, SdkError>> + Paging + Clone + Send + Sync,
    {
        assert_send(all_pages(first, 1));
    }
    let ledger = Ledger::new(
        concord::reqwest::Client::new(),
        LedgerConfig {
            signer: Signer { key: "k" },
        },
    );
    check(ledger.entries());
}
