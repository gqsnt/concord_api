//! Secrets: the API key (`ApiKey::new`, `src/model.rs:100-120`; `#[sensitive]
//! header(AUTHORIZATION)`, `src/lib.rs:62`, sent as `Authorization: ApiKey`), and the values
//! `Error`'s docs (`src/error.rs:12-14`) and the types' `Debug` withhold: a document source
//! (`DocumentSource`, `Document`), a search query (`PitSearch`), a PIT id (`PitId`,
//! `PointInTime`) and sort values (`SortValues`, `Hit`). Each has its own canary. The source
//! travels in the conditional-index body and is echoed back as the read's `_source`; the query
//! travels in search bodies; the PIT id and sort values come from the server and travel back in
//! the next search and close bodies. The server echoes the key into headers, error `reason`s
//! (withheld by `ErrorCause`), a bare-text `error` (withheld by `ErrorField`) and every
//! malformed, wrong-media and truncated body. Read (`get_document`) and write
//! (`conditional_index`) cover the seven scenarios through the SDK, the generated `send()` and
//! the native `Prepared`; the success case also runs `scan` (open PIT, two search pages, close).
//! The generated `ElasticsearchApiConfig` is also formatted with an unflagged header value. A
//! key refused by `ApiKey::new` gets its own canary in a separate test (it never travels).
//! Not covered: native reqwest values, explicit clear accessors (`source_json`, `reason`,
//! `PitId::as_str`, `SortValues::as_str`), the bulk workflow (not driven). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroU32, num::NonZeroUsize, time::Duration};

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(
            401,
            format!(
                r#"{{"error":{{"type":"security_exception","reason":"{echo}","root_cause":[{{"type":"security_exception","reason":"{echo}"}}],"caused_by":{{"type":"x","reason":"{echo}"}}}},"status":401}}"#
            ),
        ),
        2 => Reply::json(500, format!(r#"{{"error":"{echo}","status":500}}"#)),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

async fn receive<T, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) -> Result<T, concord::Error<D>> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error.widen()),
    }
}

fn concord_error<D: 'static>(c: &mut Canaries, error: &concord::Error<D>) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord::reqwest::header::HeaderValue;
use concord_sdk_elasticsearch::{
    ApiKey, DocumentId, DocumentSource, DocumentVersion, Elasticsearch, ElasticsearchApi,
    ElasticsearchApiConfig, Error, IndexName, KeepAlive, PitSearch, Sort,
};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Provider(p) | Error::VersionConflict(p) => {
            c.assert_debug_absent("provider failure", p);
            c.assert_display_absent("provider failure", p);
            c.assert_debug_absent("provider metadata", p.meta());
            c.assert_debug_absent("error document", &p.document());
            c.assert_debug_absent("error type", &p.error_type());
            if let Err(e) = p.document() {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("elasticsearch");
    let key = c.issue();
    let source = c.issue();
    let query = c.issue();
    let pit = c.issue();
    let sort = c.issue();
    let document = serde_json::json!({ "note": source });
    let search = PitSearch::new(vec![Sort::asc("@timestamp")], 10)
        .unwrap()
        .with_query(serde_json::json!({ "match": { "note": query } }))
        .unwrap();
    c.assert_debug_absent("search", &search);
    // The generated config, with a value the caller did not flag sensitive.
    let config = ElasticsearchApiConfig {
        origin: concord::BaseUrl::from_static("http://localhost:9200"),
        authorization: HeaderValue::from_str(&format!("ApiKey {key}")).unwrap(),
    };
    c.assert_debug_absent("generated config (value not flagged)", &config);
    c.assert_debug_absent(
        "generated client from config",
        &ElasticsearchApi::new(http(), config),
    );
    let keep_alive = KeepAlive::minutes(NonZeroU32::new(1).unwrap());
    for case in 0..7 {
        let (echo, source, pit, sort) = (key.clone(), source.clone(), pit.clone(), sort.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let success = match (r.method.as_str(), r.path()) {
                ("GET", "/docs/_doc/1") => format!(
                    r#"{{"_index":"docs","_id":"1","_version":1,"_seq_no":5,"_primary_term":1,"found":true,"_source":{{"note":"{source}"}}}}"#
                ),
                ("PUT", _) => r#"{"_index":"docs","_id":"1","_version":2,"_seq_no":6,"_primary_term":1,"result":"updated","_shards":{"total":2,"successful":1,"failed":0}}"#.into(),
                ("POST", "/docs/_pit") => format!(r#"{{"id":"{pit}"}}"#),
                ("GET", "/_search") if r.body_text().contains("search_after") => format!(
                    r#"{{"pit_id":"{pit}","took":1,"timed_out":false,"_shards":{{"total":1,"successful":1,"failed":0}},"hits":{{"hits":[]}}}}"#
                ),
                ("GET", "/_search") => format!(
                    r#"{{"pit_id":"{pit}","took":1,"timed_out":false,"_shards":{{"total":1,"successful":1,"failed":0}},"hits":{{"hits":[{{"_index":"docs","_id":"1","_score":null,"_source":{{"note":"{source}"}},"sort":["{sort}"]}}]}}}}"#
                ),
                _ => r#"{"succeeded":true,"num_freed":1}"#.into(),
            };
            reply(case, &success, &echo)
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let api_key = ApiKey::new(&key).unwrap();
        c.assert_debug_absent("API key", &api_key);
        let es = Elasticsearch::new(http(), &base, api_key).unwrap();
        let api = es.api();
        let index = api.index(IndexName::new("docs").unwrap());
        let read = index.get_document(DocumentId::new("1").unwrap());
        let version = DocumentVersion::new(5, 1).unwrap();
        let body = DocumentSource::new(&document).unwrap();
        c.assert_debug_absent("document source", &body);
        let write = index.conditional_index(DocumentId::new("1").unwrap(), body, version);
        c.assert_debug_absent("SDK client", &es);
        c.assert_debug_absent("generated client", api);
        c.assert_debug_absent("index scope", &index);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("read Prepared", &read.clone().prepare().unwrap());
        let prepared = write.clone().prepare().unwrap();
        c.assert_debug_absent("write Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let generated = read.send().await;
        c.assert_debug_absent("generated read Result", &generated);
        assert_eq!(generated.is_ok(), case == 0);
        if let Err(e) = &generated {
            error(&mut c, e);
        }
        let result = es.get_document("docs", "1").await;
        c.assert_debug_absent("read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
        }
        let result = es.conditional_index("docs", "1", version, &document).await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
        }
        let opened = es.open_point_in_time("docs", keep_alive).await;
        c.assert_debug_absent("open PIT Result", &opened);
        if let Err(e) = &opened {
            error(&mut c, e);
        }
        if case == 0 {
            let opened = opened.unwrap();
            c.assert_debug_absent("point in time", &opened);
            c.assert_debug_absent("PIT id", &opened.id());
            let mut pages = es.search_after(&opened, &search, NonZeroUsize::new(3).unwrap());
            let page = pages.next_page().await.unwrap().unwrap();
            c.assert_debug_absent("search page", &page);
            c.assert_debug_absent("hits", &page.items);
            c.assert_debug_absent("sort values", &page.items[0].sort());
            c.assert_debug_absent("end of traversal", &pages.next_page().await);
            c.assert_debug_absent("close Result", &es.close_point_in_time(opened).await);
            let scan = es
                .scan("docs", keep_alive, &search, NonZeroUsize::new(3).unwrap())
                .await;
            c.assert_debug_absent("scan Result", &scan);
            assert_eq!(scan.unwrap().hits.len(), 1);
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 250);
    println!(
        "elasticsearch: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn refused_key_never_prints() {
    let mut c = Canaries::new("elasticsearch");
    let key = format!("{} with spaces", c.issue());
    let e = ApiKey::new(&key).unwrap_err();
    c.assert_debug_absent("refused key", &e);
    c.assert_error_absent("refused key and sources", &e);
}
