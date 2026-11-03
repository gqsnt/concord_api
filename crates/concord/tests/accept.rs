//! A format's `Accept` suggestion (M01–M06, M11, C18): sent only as a fallback, after the authored
//! headers are composed, never as part of their composition. The truth table on prepared requests,
//! then the boundary with a `reqwest::Client`'s own default headers on the wire.

mod support;

use concord::reqwest::header::{ACCEPT, HeaderValue};
use concord::{Binary, Json, Prepared, Text, reqwest};
use serde::Deserialize;
use support::{Reply, Server};

/// An item.
#[derive(Debug, Deserialize)]
pub struct Item {
    /// Identifier.
    pub id: u64,
}

/// A format that suggests nothing: how an author disables only the suggestion (a wrapper of
/// theirs, not a `remove` of a header nobody set).
pub struct NoSuggestion;

impl concord::Decode<Item> for NoSuggestion {
    type Context = ();
    fn context(_: &concord::ResponseHead<'_>) -> Result<(), concord::CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: concord::bytes::Bytes) -> Result<Item, concord::CodecError> {
        <Json as concord::Decode<Item>>::decode_body((), body)
    }
}

concord::api! {
    /// No authored `Accept` on the client.
    pub client Plain(header(ACCEPT) accept?: HeaderValue) {
        base "http://accept.invalid";
        response Json;

        /// A typed output, nothing authored: `Json`'s suggestion.
        GET suggested() -> Item { path "a"; }
        /// No typed output: no suggestion, whatever the inherited format.
        GET untyped() { path "a"; }
        /// Text suggests nothing.
        GET text() -> String { path "a"; response Text; }
        /// Binary suggests nothing.
        GET bytes() -> Vec<u8> { path "a"; response Binary; }
        /// A format of the application's own without a suggestion.
        GET quiet() -> Item { path "a"; response NoSuggestion; }
        /// An authored literal (any case) wins and is not merged.
        GET literal() -> Item { path "a"; headers { override "Accept" = "application/vnd.acme.v2+json"; } }
        /// Removed: no suggestion comes back.
        GET removed() -> Item { path "a"; headers { remove ACCEPT; } }
        /// Removed, then reintroduced from an optional value.
        GET reintroduced(again?: String) -> Item {
            path "a";
            headers { remove ACCEPT; "accept" = again; }
        }
        /// An empty authored value is present.
        GET empty(blank: HeaderValue = HeaderValue::from_static("")) -> Item {
            path "a";
            headers { override ACCEPT = blank; }
        }
        /// Repeated authored values and q-values are sent as written.
        GET repeated() -> Item {
            path "a";
            headers { append ACCEPT = "text/html;q=0"; append "accept" = "application/json;q=0.5"; }
        }
    }
}

concord::api! {
    /// `each` over a list: an empty list places no value, like an absent optional one.
    pub client Listed {
        base "http://accept.invalid";
        response Json;
        GET listed(accepts: Vec<String> = Vec::new()) -> Item {
            path "a";
            headers { "accept" = each accepts; }
        }
    }
}

concord::api! {
    /// An authored `Accept` on the client, and a sensitive one.
    pub client Authored(#[sensitive] header("accept") secret?: String) {
        base "http://accept.invalid";
        headers { append ACCEPT = "application/vnd.api+json"; }
        response Json;
        GET inherited() -> Item { path "a"; }
        /// Removed below an authored value: the chain ends with `remove`.
        GET removed() -> Item { path "a"; headers { remove "accept"; } }
    }
}

concord::api! {
    /// `accept` marked sensitive (literal key), its value optional and absent.
    pub client SecretLiteral(#[sensitive] header("accept") secret?: String) {
        base "http://accept.invalid";
        response Json;
        GET item() -> Item { path "a"; }
    }
}

concord::api! {
    /// The same with the native key: decided when the request is built.
    pub client SecretNative(#[sensitive] header(ACCEPT) secret?: String) {
        base "http://accept.invalid";
        response Json;
        GET item() -> Item { path "a"; }
    }
}

fn accept_of<T>(prepared: &Prepared<T>) -> Vec<&[u8]> {
    prepared
        .request()
        .headers()
        .get_all(ACCEPT)
        .iter()
        .map(HeaderValue::as_bytes)
        .collect()
}

#[test]
fn prepared_requests_follow_the_truth_table() {
    let api = Plain::new(reqwest::Client::new());
    let json: &[u8] = b"application/json";
    // M01: typed output, nothing authored → the suggestion; no typed output → nothing.
    assert_eq!(accept_of(&api.suggested().prepare().unwrap()), [json]);
    assert!(accept_of(&api.untyped().prepare().unwrap()).is_empty());
    assert!(accept_of(&api.text().prepare().unwrap()).is_empty());
    assert!(accept_of(&api.bytes().prepare().unwrap()).is_empty());
    assert!(accept_of(&api.quiet().prepare().unwrap()).is_empty());
    // M02: an authored literal (any case) or native value wins, unmerged.
    assert_eq!(
        accept_of(&api.literal().prepare().unwrap()),
        [&b"application/vnd.acme.v2+json"[..]]
    );
    let native = api.clone().set_accept(HeaderValue::from_static("text/csv"));
    assert_eq!(
        accept_of(&native.suggested().prepare().unwrap()),
        [&b"text/csv"[..]]
    );
    // M03: an optional authored value that is absent → the suggestion; a removal is not absence.
    assert!(accept_of(&api.removed().prepare().unwrap()).is_empty());
    // M04: reintroduced after the removal: present → authored; absent → the suggestion (the
    // chain's last word is a set, not a removal).
    assert_eq!(
        accept_of(
            &api.reintroduced()
                .set_again("text/x-again")
                .prepare()
                .unwrap()
        ),
        [&b"text/x-again"[..]]
    );
    assert_eq!(accept_of(&api.reintroduced().prepare().unwrap()), [json]);
    // M05: an empty authored value is present; repeated values and q-values are kept in order.
    assert_eq!(accept_of(&api.empty().prepare().unwrap()), [&b""[..]]);
    assert_eq!(
        accept_of(&api.repeated().prepare().unwrap()),
        [&b"text/html;q=0"[..], b"application/json;q=0.5"]
    );

    let authored = Authored::new(reqwest::Client::new());
    assert_eq!(
        accept_of(&authored.inherited().prepare().unwrap()),
        [&b"application/vnd.api+json"[..]]
    );
    assert!(accept_of(&authored.removed().prepare().unwrap()).is_empty());

    // `each` over an empty list places nothing: the suggestion; over values: exactly those.
    let listed = Listed::new(reqwest::Client::new());
    assert_eq!(accept_of(&listed.listed().prepare().unwrap()), [json]);
    assert_eq!(
        accept_of(
            &listed
                .listed()
                .set_accepts(vec!["text/csv".to_owned(), "text/plain".to_owned()])
                .prepare()
                .unwrap()
        ),
        [&b"text/csv"[..], b"text/plain"]
    );
}

/// M11: the suggestion inserted under a name the endpoint classifies sensitive is flagged too
/// (Concord may add sensitivity, never clear it).
#[test]
fn a_sensitive_accept_classifies_the_suggestion() {
    let prepared = Authored::new(reqwest::Client::new())
        .removed()
        .prepare()
        .unwrap();
    // `remove "accept"` closed the chain: nothing is sent, nothing to classify.
    assert!(accept_of(&prepared).is_empty());
    let prepared = Plain::new(reqwest::Client::new())
        .suggested()
        .prepare()
        .unwrap();
    assert!(!prepared.request().headers()[ACCEPT].is_sensitive());
    // Absent sensitive value: the suggestion is sent, flagged and redacted in Concord's Debug.
    for prepared in [
        SecretLiteral::new(reqwest::Client::new())
            .item()
            .prepare()
            .unwrap(),
        SecretNative::new(reqwest::Client::new())
            .item()
            .prepare()
            .unwrap(),
    ] {
        let value = &prepared.request().headers()[ACCEPT];
        assert_eq!(value, "application/json");
        assert!(value.is_sensitive());
        assert!(format!("{prepared:?}").contains("accept: <redacted>"));
    }
}

/// M06: on the wire. A request's `Accept` (authored or suggested) replaces a client default; when
/// Concord sends none (no typed output, removal), reqwest's default (`*/*`) or the caller's
/// configured default goes out. C18: editing the native request does not change the decoder.
#[tokio::test]
async fn the_wire_and_the_client_defaults() {
    let server = Server::start(|_| Reply::json(200, r#"{"id":1}"#)).await;
    let mut defaults = reqwest::header::HeaderMap::new();
    defaults.insert(
        ACCEPT,
        HeaderValue::from_static("application/x-client-default"),
    );
    let configured = reqwest::Client::builder()
        .default_headers(defaults)
        .build()
        .unwrap();
    for (client, fallback) in [
        (reqwest::Client::new(), "*/*"),
        (configured, "application/x-client-default"),
    ] {
        let api = Plain::new(client).with_base_url(server.url()).unwrap();
        api.suggested().send().await.unwrap();
        api.untyped().send().await.unwrap();
        api.removed().send().await.unwrap();
        api.empty().send().await.unwrap();
        api.repeated().send().await.unwrap();
        // C18: the caller removes the header natively; the decoder (Json) is unchanged.
        let (client, mut request, decoder) = api.suggested().prepare().unwrap().into_parts();
        request.headers_mut().remove(ACCEPT);
        let item = decoder.finish(client.execute(request).await).await.unwrap();
        assert_eq!(item.id, 1);
        let seen: Vec<Vec<String>> = server
            .requests()
            .iter()
            .rev()
            .take(6)
            .rev()
            .map(|r| {
                r.header_all("accept")
                    .iter()
                    .map(|v| (*v).to_owned())
                    .collect()
            })
            .collect();
        assert_eq!(
            seen,
            [
                vec!["application/json".to_owned()],
                vec![fallback.to_owned()],
                vec![fallback.to_owned()],
                vec![String::new()],
                vec![
                    "text/html;q=0".to_owned(),
                    "application/json;q=0.5".to_owned()
                ],
                vec![fallback.to_owned()],
            ],
            "client default {fallback}"
        );
    }
}
