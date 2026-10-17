//! Executable follow-up to the V3 public-documentation exercises.
//!
//! These downstream callers distinguish payload absence from wire omission,
//! representation direction from Rust type, and successful acquisition from
//! complete traversal/consumption. They use only public facade contracts and
//! the ordinary loopback server; no generated/private executor hooks are used.

#![cfg(all(feature = "json", feature = "ndjson"))]

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::http::{HeaderName, SafeProxy};
use concord::inspection::RepresentationSource;
use concord::observation::SanitizedHeaderValue;
use concord::pagination::{
    ContinuationKnowledge, OffsetInit, OffsetPagination, PaginationErrorKind, StopReason,
};
use concord::records::NdJson;
use concord::response::StreamState;
use concord::retry::{RetryDecision, RetryPolicy};
use concord::{
    AtomEncoder, AtomError, Binary, BoundedAtomWriter, ByteStream, CancellationToken,
    DispatchKnowledge, Empty, ItemStream, Json, RequestAtom, Text, api,
};
use concord_test_support::server::{ResponseScript, ScriptedServer};

pub const INITIAL_NESTED: Option<i32> = Some(3);
static INITIALIZATIONS: AtomicUsize = AtomicUsize::new(0);

pub struct NullableNumber;

impl AtomEncoder<Option<i32>> for NullableNumber {
    fn write(value: &Option<i32>, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        match value {
            Some(value) => out.push_str(&value.to_string()),
            None => out.push_str("null"),
        }
    }
}

#[derive(Clone)]
pub struct Initialized(usize);

impl Default for Initialized {
    fn default() -> Self {
        Self(INITIALIZATIONS.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

impl RequestAtom for Initialized {
    fn write_atom(&self, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(&self.0.to_string())
    }
}

api! {
    client ValueExerciseApi {
        base "http://value-exercises.example.test/";
        response Empty;
        query { "locale" = "fr"; "tag" = "root"; }
        headers { "X-Locale" = "fr"; }

        GET slots(
            query plain?: i32,
            query nested?: Option<i32> via NullableNumber,
            query retained?: Option<i32> via NullableNumber = INITIAL_NESTED,
            #[default] query once?: Initialized,
        ) -> ();

        GET conditional(language?: String) -> () {
            query { when_present "locale" = language; }
            headers { when_present "x-locale" = language; }
        }
        GET exact(language?: String) -> () {
            query { replace "locale" = language; }
            headers { replace "x-locale" = language; }
        }
        GET appended(tags: Vec<String>) -> () {
            query { append "tag" = each tags; }
        }
        GET conditional_tags(tags?: Vec<String>) -> () {
            query { when_present "tag" = each tags; }
        }
        GET removed() -> () {
            query { remove "locale"; }
            headers { remove "x-locale"; }
        }
        GET replaced_family(query page?: u32) -> () {
            query replace { "only" = "this"; }
        }
        scope cleared() {
            query off;
            headers off;
            GET child(query again?: u32) -> ();
        }
    }
}

fn query_values<'a>(url: &'a str, key: &str) -> Vec<&'a str> {
    url.split_once('?')
        .map(|(_, query)| {
            query
                .split('&')
                .filter_map(|part| part.split_once('='))
                .filter_map(|(name, value)| (name == key).then_some(value))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn nested_option_controls_preserve_inner_none_and_the_retained_initializer() {
    let api = ValueExerciseApi::new().expect("client");
    let before = INITIALIZATIONS.load(Ordering::SeqCst);
    let request = api.slots();
    let initial = request.plan().expect("initial plan");
    assert!(query_values(initial.observed_url(), "nested").is_empty());
    assert_eq!(query_values(initial.observed_url(), "retained"), ["3"]);
    let retained_once = query_values(initial.observed_url(), "once")[0].to_owned();

    let inner_none = request.clone().set_nested(None).plan().expect("inner None");
    assert_eq!(query_values(inner_none.observed_url(), "nested"), ["null"]);

    let conditional = request
        .clone()
        .set_plain(7)
        .set_nested(Some(7))
        .set_nested_if_some(None)
        .plan()
        .expect("no new opinion");
    assert_eq!(query_values(conditional.observed_url(), "plain"), ["7"]);
    assert_eq!(query_values(conditional.observed_url(), "nested"), ["7"]);

    let exact_none = request
        .clone()
        .replace_nested(Some(None))
        .plan()
        .expect("exact inner None");
    assert_eq!(query_values(exact_none.observed_url(), "nested"), ["null"]);
    let absent = request
        .clone()
        .set_nested(Some(7))
        .replace_nested(None)
        .omit_retained()
        .plan()
        .expect("outer absence");
    assert!(query_values(absent.observed_url(), "nested").is_empty());
    assert!(query_values(absent.observed_url(), "retained").is_empty());

    let restored = request
        .set_retained(None)
        .omit_retained()
        .reset_retained()
        .set_once(Initialized(999))
        .omit_once()
        .reset_once()
        .plan()
        .expect("retained reset");
    assert_eq!(query_values(restored.observed_url(), "retained"), ["3"]);
    assert_eq!(
        query_values(restored.observed_url(), "once"),
        [retained_once]
    );
    assert_eq!(INITIALIZATIONS.load(Ordering::SeqCst), before + 1);
}

#[test]
fn patches_distinguish_absence_empty_sequences_and_family_resets() {
    let api = ValueExerciseApi::new().expect("client");
    let header = HeaderName::from_static("x-locale");
    let inherited = api.conditional().plan().expect("inherited");
    assert_eq!(query_values(inherited.observed_url(), "locale"), ["fr"]);
    assert_eq!(
        inherited.public_headers().get(&header),
        Some(SanitizedHeaderValue::Text("fr"))
    );
    let replaced = api
        .conditional()
        .set_language("de")
        .plan()
        .expect("replace");
    assert_eq!(query_values(replaced.observed_url(), "locale"), ["de"]);
    assert_eq!(
        replaced.public_headers().get(&header),
        Some(SanitizedHeaderValue::Text("de"))
    );
    for removed in [api.exact().plan().unwrap(), api.removed().plan().unwrap()] {
        assert!(query_values(removed.observed_url(), "locale").is_empty());
        assert!(removed.public_headers().get(&header).is_none());
    }
    let absent_tags = api.conditional_tags().plan().expect("absent tags");
    assert_eq!(query_values(absent_tags.observed_url(), "tag"), ["root"]);
    let empty_tags = api
        .conditional_tags()
        .set_tags(Vec::new())
        .plan()
        .expect("present empty tags");
    assert!(query_values(empty_tags.observed_url(), "tag").is_empty());

    let fresh = api
        .replaced_family()
        .set_page(2)
        .plan()
        .expect("family reset");
    assert_eq!(
        fresh.observed_url().split_once('?').unwrap().1,
        "page=2&only=this"
    );
    assert_eq!(
        fresh.public_headers().get(&header),
        Some(SanitizedHeaderValue::Text("fr"))
    );
    let descendant = api
        .cleared()
        .child()
        .set_again(4)
        .plan()
        .expect("new child entry");
    assert_eq!(
        descendant.observed_url().split_once('?').unwrap().1,
        "again=4"
    );
    assert!(descendant.public_headers().get(&header).is_none());
}

fn complete(status: u16, media: &str, body: &[u8]) -> ResponseScript {
    ResponseScript::Complete {
        status,
        headers: vec![("Content-Type".to_owned(), media.to_owned())],
        body: body.to_vec(),
    }
}

#[tokio::test]
async fn append_each_preserves_the_declared_vector_and_adds_exact_wire_occurrences() {
    let server =
        ScriptedServer::start((0..3).map(|_| complete(200, "text/plain", b""))).expect("server");
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    let api = ValueExerciseApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("client");
    for tags in [
        vec![],
        vec!["one".to_owned()],
        vec!["one".to_owned(), "two".to_owned()],
    ] {
        api.appended(tags).await.expect("appended request");
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(query_values(requests[0].target(), "tag"), ["root"]);
    assert_eq!(query_values(requests[1].target(), "tag"), ["root", "one"]);
    assert_eq!(
        query_values(requests[2].target(), "tag"),
        ["root", "one", "two"]
    );
}

// ANCHOR: exercise_representation_additions
api! {
    client RepresentationExerciseApi {
        base "http://representation-exercises.example.test/";
        request Json;
        response Json;

        POST inherited(body input: String) -> String;
        POST inline_response(body input: String) -> String via Text;

        scope request_only() {
            request Text;
            POST preserves_response(body input: String) -> String;
        }
        scope overrides() {
            request Text;
            response Text;
            POST added(body input: String) -> String;
        }
        scope fenced() {
            request explicit;
            response explicit;
            POST added(body input: String via Text) -> String via Text;
        }
    }
}
// ANCHOR_END: exercise_representation_additions

#[tokio::test]
async fn representations_are_independent_even_when_every_rust_payload_is_string() {
    let server = ScriptedServer::start([
        complete(200, "application/json", br#""root reply""#),
        complete(200, "text/plain", b"inline reply"),
        complete(200, "application/json", br#""mixed reply""#),
        complete(200, "text/plain", b"scope reply"),
        complete(200, "text/plain", b"fenced reply"),
    ])
    .expect("server");
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    let api = RepresentationExerciseApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("client");
    let mixed = api.request_only().preserves_response("mixed".to_owned());
    let description = mixed.description().contract().expect("contract");
    assert_eq!(
        description.body().unwrap().representation().source(),
        RepresentationSource::Inherited("RepresentationExerciseApi::request_only")
    );
    assert_eq!(
        description.result().representation().source(),
        RepresentationSource::Inherited("RepresentationExerciseApi")
    );
    assert_eq!(
        api.inherited("root".to_owned()).await.unwrap(),
        "root reply"
    );
    assert_eq!(
        api.inline_response("inline".to_owned()).await.unwrap(),
        "inline reply"
    );
    assert_eq!(mixed.await.unwrap(), "mixed reply");
    assert_eq!(
        api.overrides().added("scope".to_owned()).await.unwrap(),
        "scope reply"
    );
    assert_eq!(
        api.fenced().added("fenced".to_owned()).await.unwrap(),
        "fenced reply"
    );

    let requests = server.requests();
    assert_eq!(requests[0].body(), br#""root""#);
    assert_eq!(requests[1].body(), br#""inline""#);
    for (request, expected) in requests[2..].iter().zip(["mixed", "scope", "fenced"]) {
        assert_eq!(request.body(), expected.as_bytes());
        assert_eq!(
            request.header("content-type"),
            Some("text/plain; charset=utf-8")
        );
    }
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    assert_eq!(requests[1].header("content-type"), Some("application/json"));
}

api! {
    client RuntimeExerciseApi {
        base "http://runtime-exercises.example.test/";
        response Text;

        GET safe() -> String { effect safe; }
        PUT idempotent(body input: String via Text) -> String { effect idempotent; }
        POST write(body input: String via Text) -> String { effect non_idempotent; }

        GET list(region: String, query offset: u64 = 0, query count: NonZeroU32) -> Vec<u32> via Json {
            path "{region}";
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: count };
                cursor -> offset;
            }
        }
        GET bytes() -> ByteStream via Binary;
        GET records() -> ItemStream<u32> via NdJson;
    }
}

fn runtime_client(server: &ScriptedServer) -> RuntimeExerciseApi {
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    RuntimeExerciseApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .retry(
            RetryPolicy::builder()
                .max_retries_per_operation(1)
                .without_budget()
                .classify_fn(|_| RetryDecision::Retry)
                .build()
                .expect("retry policy"),
        )
        .build()
        .expect("client")
}

#[tokio::test]
async fn effect_gates_observed_retries_independently_of_buffered_body_replayability() {
    for effect in ["safe", "idempotent", "non_idempotent"] {
        let server = ScriptedServer::start([
            complete(503, "text/plain", b"unavailable"),
            complete(200, "text/plain", b"ok"),
        ])
        .expect("server");
        let api = runtime_client(&server);
        let result = match effect {
            "safe" => api.safe().await,
            "idempotent" => api.idempotent("command".to_owned()).await,
            _ => api.write("command".to_owned()).await,
        };
        if effect == "non_idempotent" {
            let error = result.expect_err("an observed write must not replay");
            assert_eq!(
                error.dispatch_knowledge(),
                DispatchKnowledge::ResponseObserved
            );
            assert_eq!(server.request_count(), 1);
        } else {
            assert_eq!(result.expect("eligible retry"), "ok");
            assert_eq!(server.request_count(), 2);
            if effect == "idempotent" {
                assert_eq!(server.requests()[0].body(), b"command");
                assert_eq!(server.requests()[1].body(), b"command");
            }
        }
    }
}

#[tokio::test]
async fn page_bound_outcomes_distinguish_preview_complete_and_lazy_use() {
    let server = ScriptedServer::start([
        complete(200, "application/json", b"[1,2]"),
        complete(200, "application/json", b"[3,4]"),
        complete(200, "application/json", b"[5,6]"),
        complete(200, "application/json", b"[7]"),
    ])
    .expect("server");
    let api = runtime_client(&server);
    let count = NonZeroU32::new(2).unwrap();
    let prefix = api.list("items", count).collect_prefix(1).await.unwrap();
    assert_eq!(prefix.items(), [1, 2]);
    assert_eq!(prefix.summary().stop_reason(), StopReason::PageBound);
    assert_eq!(
        prefix.summary().continuation(),
        ContinuationKnowledge::NextRequestKnown
    );

    let error = api
        .list("items", count)
        .collect_all(NonZeroU32::MIN)
        .await
        .unwrap_err();
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CompletionLimitExceeded)
    );
    assert_eq!(error.traversal_summary().unwrap().items_delivered(), 0);
    let mut pages = api.list("items", count).pages(1);
    assert_eq!(pages.next_page().await.unwrap().unwrap().items(), [5, 6]);
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().stop_reason(), StopReason::PageBound);

    let complete = api
        .list("items", count)
        .collect_all(NonZeroU32::MIN)
        .await
        .unwrap();
    assert_eq!(complete, [7]);
    assert_eq!(
        server.request_count(),
        4,
        "bounds never trigger an extra proof fetch"
    );
}

#[tokio::test]
async fn zero_page_bounds_skip_even_invalid_public_shaping_and_allocate_no_operation() {
    let server = ScriptedServer::start([]).expect("empty server");
    let api = runtime_client(&server);
    assert!(api.list("invalid/path", NonZeroU32::MIN).plan().is_err());
    let prefix = api
        .list("invalid/path", NonZeroU32::MIN)
        .collect_prefix(0)
        .await
        .unwrap();
    assert!(prefix.items().is_empty());
    assert_eq!(prefix.summary().stop_reason(), StopReason::PageBound);
    assert_eq!(
        prefix.summary().continuation(),
        ContinuationKnowledge::Unknown
    );
    assert!(prefix.summary().operation_id().is_none());
    let mut pages = api.list("invalid/path", NonZeroU32::MIN).pages(0);
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.summary().stop_reason(), StopReason::PageBound);
    assert_eq!(
        pages.summary().continuation(),
        ContinuationKnowledge::Unknown
    );
    assert!(pages.summary().operation_id().is_none());
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn byte_success_record_failure_and_cancellation_keep_distinct_stream_outcomes() {
    let payload = b"1\nnot-json\n";
    let server = ScriptedServer::start([
        complete(200, "application/octet-stream", payload),
        complete(200, "application/x-ndjson", payload),
        complete(200, "application/octet-stream", b"cancel me"),
        complete(200, "application/octet-stream", b"stop early"),
    ])
    .expect("server");
    let api = runtime_client(&server);
    let mut bytes = api.bytes().await.expect("byte acquisition");
    assert_eq!(bytes.summary().state(), StreamState::Active);
    let mut received = Vec::new();
    while let Some(chunk) = bytes.next().await {
        received.extend_from_slice(&chunk.expect("opaque byte chunk"));
    }
    assert_eq!(received, payload);
    assert_eq!(bytes.summary().state(), StreamState::Eof);
    assert_eq!(bytes.summary().delivered_records(), 0);

    let mut records = api
        .records()
        .await
        .expect("record acquisition precedes decode");
    assert_eq!(records.summary().state(), StreamState::Active);
    assert_eq!(records.next().await.unwrap().unwrap(), 1);
    let error = records
        .next()
        .await
        .unwrap()
        .expect_err("malformed second record");
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(
        records.next().await.is_none(),
        "the terminal error is yielded once"
    );
    assert_eq!(records.summary().state(), StreamState::Failed);
    assert_eq!(records.summary().delivered_records(), 1);

    let token = CancellationToken::new();
    let mut cancelled = api
        .bytes()
        .cancel_with(token.clone())
        .unwrap()
        .await
        .unwrap();
    token.cancel();
    assert!(cancelled.next().await.unwrap().is_err());
    assert!(cancelled.next().await.is_none());
    assert_eq!(cancelled.summary().state(), StreamState::Cancelled);

    let stopped = api.bytes().await.unwrap().finish();
    assert_eq!(stopped.state(), StreamState::Application);
    assert_eq!(
        server.request_count(),
        4,
        "the eager retry classifier cannot replay a handed-off stream"
    );
}
