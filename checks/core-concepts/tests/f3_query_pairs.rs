//! Between waves 1 and 2, F3 query pairs: a `query(..)` placement whose value writes its own
//! key(s). Witnesses: SDK24 Discord's mutually exclusive `around`/`before`/`after` message
//! anchors (a private `raw` module plus wrappers, +245 lines, to keep three optional query
//! values from being set together) and ClickHouse's dynamic `param_<name>` keys (SDK20).

use concord::{ErrorKind, Json, QueryPairs, QueryPairsWriter};
use concord_example_testserver::{Reply, TestServer};
use serde::Deserialize;

/// One message.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Message {
    /// Its id.
    pub id: u64,
}

/// Where a page of messages is anchored. Exactly one anchor, so the three cursors can never be
/// sent together.
#[derive(Debug, Clone, Copy)]
pub enum Anchor {
    /// Messages around this id.
    Around(u64),
    /// Messages before this id.
    Before(u64),
    /// Messages after this id.
    After(u64),
}

impl QueryPairs for Anchor {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        match *self {
            Self::Around(id) => out.pair("around", id),
            Self::Before(id) => out.pair("before", id),
            Self::After(id) => out.pair("after", id),
        }
    }
}

/// Named parameters bound into a query text, sent as `param_<name>=<value>`.
#[derive(Debug, Default, Clone)]
pub struct Bindings(Vec<(String, String)>);

impl Bindings {
    /// Adds one binding.
    pub fn bind(mut self, name: &str, value: impl ToString) -> Self {
        self.0.push((format!("param_{name}"), value.to_string()));
        self
    }
}

impl QueryPairs for Bindings {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        for (key, value) in &self.0 {
            out.pair(key, value);
        }
    }
}

concord::api! {
    /// A chat service and a query service.
    pub client Chat() {
        base "http://chat.invalid";
        format Json;

        scope channels(id: u64) {
            path "channels/{id}";
            query { "v" = "10"; }
            /// A page of messages, anchored at most once.
            GET messages(query limit?: u8, query(..) anchor?: Anchor) -> Vec<Message> {
                path "messages";
            }
        }
        /// Runs a query text with named bindings.
        GET run(query sql: String, query(..) bindings: Bindings) -> Vec<Message> {
            path "query";
        }
    }
}

fn chat(server: &TestServer) -> Chat {
    Chat::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap()
}

#[tokio::test]
async fn an_anchor_writes_exactly_its_own_key_after_the_declared_query() {
    let server = TestServer::start(|_| Reply::json(200, r#"[{"id":1}]"#)).await;
    let chat = chat(&server);
    let channel = chat.channels(7);

    assert_eq!(
        channel.messages().send().await.unwrap(),
        vec![Message { id: 1 }]
    );
    channel
        .messages()
        .set_limit(50)
        .set_anchor(Anchor::Before(99))
        .send()
        .await
        .unwrap();
    channel
        .messages()
        .set_anchor(Anchor::Around(5))
        .send()
        .await
        .unwrap();

    let seen: Vec<_> = server.requests().iter().map(|r| r.query_pairs()).collect();
    let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
        p.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    // Absent: nothing written. Present: declared query first (scope, then endpoint), then the
    // value's pairs.
    assert_eq!(seen[0], pairs(&[("v", "10")]));
    assert_eq!(
        seen[1],
        pairs(&[("v", "10"), ("limit", "50"), ("before", "99")])
    );
    assert_eq!(seen[2], pairs(&[("v", "10"), ("around", "5")]));
}

#[tokio::test]
async fn dynamic_keys_are_encoded_with_the_rest_of_the_query() {
    let server = TestServer::start(|_| Reply::json(200, "[]")).await;
    let chat = chat(&server);
    chat.run(
        "SELECT {x:UInt8}",
        Bindings::default().bind("x", 3).bind("name", "a&b=c"),
    )
    .send()
    .await
    .unwrap();
    let request = server.single();
    assert_eq!(request.path(), "/query");
    assert_eq!(
        request.query_pairs(),
        vec![
            ("sql".to_string(), "SELECT {x:UInt8}".to_string()),
            ("param_x".to_string(), "3".to_string()),
            ("param_name".to_string(), "a&b=c".to_string()),
        ]
    );
    // Reserved characters in keys and values are percent-encoded, never split the query.
    assert!(request.query().unwrap().contains("param_name=a%26b%3Dc"));
}

#[tokio::test]
async fn an_empty_key_is_a_build_error_before_any_request() {
    let server = TestServer::start(|_| Reply::json(200, "[]")).await;
    let chat = chat(&server);
    let error = chat
        .run("SELECT 1", Bindings(vec![(String::new(), "1".into())]))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    let text = error.to_string();
    assert!(text.contains("bindings"), "{text}");
    assert!(text.contains("empty key"), "{text}");
    assert!(server.requests().is_empty());
}

#[test]
fn debug_withholds_a_pairs_value() {
    let chat = Chat::new(concord::reqwest::Client::new());
    let shown = format!(
        "{:?}",
        chat.run("SELECT 1", Bindings::default().bind("secretish", "v"))
    );
    assert!(!shown.contains("secretish"), "{shown}");
    assert!(!shown.contains("param_"), "{shown}");
    let shown = format!(
        "{:?}",
        chat.channels(1).messages().set_anchor(Anchor::After(42))
    );
    assert!(!shown.contains("42"), "{shown}");
}

#[test]
fn a_well_known_secret_key_is_redacted_in_prepared_debug() {
    let chat = Chat::new(concord::reqwest::Client::new());
    let mut pairs = Bindings::default().bind("x", 1);
    pairs.0.push(("API_Key".into(), "s3cr3t".into()));
    pairs.0.push(("Token".into(), "t0k3n".into()));
    let prepared = chat.run("SELECT 1", pairs).prepare().unwrap();
    let shown = format!("{prepared:?}");
    assert!(!shown.contains("s3cr3t"), "{shown}");
    assert!(!shown.contains("t0k3n"), "{shown}");
    assert!(shown.contains("API_Key=<redacted>"), "{shown}");
    assert!(shown.contains("param_x=1"), "{shown}");
    // The wire is unchanged.
    let query = prepared.request().url().query().unwrap().to_owned();
    assert!(query.ends_with("API_Key=s3cr3t&Token=t0k3n"), "{query}");
}

concord::api! {
    /// Order witnesses: a `query(..)` value sits where it is declared among the endpoint's keys.
    pub client Ordered() {
        base "http://ordered.invalid";
        query { "v" = "1"; }
        /// Discord's wire order: the cursor, then `limit`.
        GET first(query(..) anchor?: Anchor, query limit?: u8) {
            path "a";
            query { "tail" = "t"; }
        }
        /// Between two keyed values; block entries (even referencing an earlier value) last.
        GET middle(query a: u8, query(..) anchor: Anchor, query b: u8) {
            path "b";
            query { "again" = a; }
        }
    }
}

#[test]
fn pairs_keep_their_declared_position_among_the_endpoints_keys() {
    let ordered = Ordered::new(concord::reqwest::Client::new());
    let query = |p: concord::Prepared<()>| p.request().url().query().unwrap().to_owned();
    assert_eq!(
        query(
            ordered
                .first()
                .set_anchor(Anchor::Before(9))
                .set_limit(5)
                .prepare()
                .unwrap()
        ),
        "v=1&before=9&limit=5&tail=t"
    );
    // Absent: nothing is written at its position.
    assert_eq!(
        query(ordered.first().set_limit(5).prepare().unwrap()),
        "v=1&limit=5&tail=t"
    );
    assert_eq!(
        query(ordered.middle(1, Anchor::After(2), 3).prepare().unwrap()),
        "v=1&a=1&after=2&b=3&again=1"
    );
}
