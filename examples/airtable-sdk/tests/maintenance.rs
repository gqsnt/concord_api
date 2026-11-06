//! Maintenance ticket: a table field is renamed and an unknown field appears while the offset
//! contains reserved characters. Fixture provenance: ../PROVENANCE.md; record: ../MAINTENANCE.md.
#![allow(missing_docs)]
use concord_example_testserver::{Recorded, Reply, Script, TestServer};
use concord_sdk_airtable::{
    Client, Error, ListOptions, ProviderError,
    pagination::{Outcome, PagesError, Resume},
};
use serde_json::json;
use std::num::NonZeroUsize;

/// The offset in `before_rename.json` after JSON decoding (the fixture spells `/`, `é`, `"` and
/// `\` as JSON escapes). RFC 3986 gen-delims and sub-delims, `%`, a space, a percent-escape
/// look-alike and non-ASCII.
const OFFSET: &str = "itr/rec:@!$&'()*+,;=?#[]% %2F~é\"\\";
/// The same offset as observed in the request target: query-encoded exactly once.
const OFFSET_ON_WIRE: &str =
    "offset=itr%2Frec%3A%40%21%24%26%27%28%29*%2B%2C%3B%3D%3F%23%5B%5D%25+%252F%7E%C3%A9%22%5C";
const OLD_NAME: &str = "Status";
const NEW_NAME: &str = "Stage [v2] / #1 & 50%+";
const NEW_NAME_ON_WIRE: &str = "fields%5B%5D=Stage+%5Bv2%5D+%2F+%231+%26+50%25%2B";
const UNKNOWN: &str = "Reviewer (added later)";

const BEFORE: &[u8] = include_bytes!("../fixtures/maintenance/before_rename.json");
const AFTER: &[u8] = include_bytes!("../fixtures/maintenance/after_rename.json");

fn client(server: &TestServer) -> Client {
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    Client::new(http, "SYNTHETIC_TOKEN_NOT_A_CREDENTIAL")
        .with_base_url(server.url())
        .unwrap()
}

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn assert_get(request: &Recorded) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), "/v0/appMaint/Tasks");
    assert_eq!(request.body_len, 0);
}

/// The SDK holds no schema: a rename between pages is data. The old key is simply absent from
/// later records (Airtable also omits empty values, so absence is not evidence of either), the
/// new name and an unmodelled field arrive as JSON, an unmodelled record property is ignored,
/// and the reserved-character offset is sent back exactly once-encoded.
#[tokio::test]
async fn rename_between_pages_passes_new_and_unknown_fields_and_returns_offset_exactly() {
    let script = Script::new([Reply::json(200, BEFORE), Reply::json(200, AFTER)]);
    let server = TestServer::start(move |_| script.next()).await;
    let mut pages = client(&server).list_pages(
        "appMaint",
        "Tasks",
        ListOptions::default(),
        Resume::Start,
        cap(3),
    );

    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].fields[OLD_NAME], json!("Open"));
    assert_eq!(first.next.as_deref(), Some(OFFSET));

    let second = pages.next_page().await.unwrap().unwrap();
    let record = &second.items[0];
    assert_eq!(record.id, "recMaintTwo");
    assert!(!record.fields.contains_key(OLD_NAME));
    assert_eq!(record.fields[NEW_NAME], json!("Open"));
    assert_eq!(
        record.fields[UNKNOWN],
        json!({"id": "usrSyntheticMaint", "email": "reviewer@example.invalid", "name": "Synthetic Reviewer"})
    );
    assert_eq!(record.fields.len(), 3);
    assert!(second.next.is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_get(request);
    }
    assert_eq!(requests[0].query(), None);
    assert_eq!(requests[1].query(), Some(OFFSET_ON_WIRE));
    assert_eq!(requests[1].query_all("offset"), [OFFSET]);
    server.shutdown().await;
}

/// A selection and formula that still use the old name fail at the provider. The failure stays
/// typed, keeps the records already read, names the exact failed offset, and prints neither the
/// offset nor the provider's text. The caller's change is its options; because an offset belongs
/// to the options it was issued for, the caller restarts rather than resuming with new ones.
#[tokio::test]
async fn stale_name_in_selection_fails_typed_with_exact_offset_and_restart_uses_new_name() {
    let script = Script::new([
        Reply::json(200, BEFORE),
        Reply::json(
            422,
            include_bytes!("../fixtures/maintenance/renamed_field_rejected.json"),
        )
        .header("x-request-id", "synthetic-rename"),
        Reply::json(
            200,
            include_bytes!("../fixtures/maintenance/restart_after_rename.json"),
        ),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let stale = ListOptions {
        fields: vec!["Name".into(), OLD_NAME.into()],
        filter_by_formula: Some(format!("{{{OLD_NAME}}} = 'Open'")),
        ..Default::default()
    };
    let incomplete = api
        .list_pages("appMaint", "Tasks", stale, Resume::Start, cap(5))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].fields[OLD_NAME], json!("Open"));
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    let PagesError::Provider { error, failed, .. } = &incomplete.error else {
        panic!("expected a provider failure, got {:?}", incomplete.error);
    };
    assert!(matches!(failed, Resume::At(offset) if offset == OFFSET));
    assert!(matches!(error, Error::Http { body: Some(_), .. }));
    assert_eq!(error.meta().unwrap().status().as_u16(), 422);
    assert_eq!(
        error.meta().unwrap().headers()["x-request-id"],
        "synthetic-rename"
    );
    assert!(matches!(
        error.provider(),
        Some(ProviderError::Object { kind, message: Some(_) }) if kind == "INVALID_REQUEST_UNKNOWN"
    ));
    let shown = format!("{incomplete:?} {incomplete} {error:?} {error}");
    for withheld in [
        OFFSET,
        &OFFSET_ON_WIRE["offset=".len()..],
        "Synthetic maintenance",
    ] {
        assert!(!shown.contains(withheld), "{withheld:?} printed in {shown}");
    }

    let renamed = ListOptions {
        fields: vec!["Name".into(), NEW_NAME.into()],
        filter_by_formula: Some(format!("{{{NEW_NAME}}} = 'Open'")),
        ..Default::default()
    };
    let records = api
        .list_pages("appMaint", "Tasks", renamed, Resume::Start, cap(5))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(records.len(), 2);
    for record in &records {
        assert_eq!(record.fields[NEW_NAME], json!("Open"));
        assert!(!record.fields.contains_key(OLD_NAME));
    }

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for (request, name) in requests.iter().zip([OLD_NAME, OLD_NAME, NEW_NAME]) {
        assert_get(request);
        assert_eq!(request.query_all("fields[]"), ["Name", name]);
        assert_eq!(
            request.query_all("filterByFormula"),
            [format!("{{{name}}} = 'Open'")]
        );
    }
    assert_eq!(requests[0].query_all("offset"), Vec::<String>::new());
    assert_eq!(requests[1].query_all("offset"), [OFFSET]);
    assert!(requests[1].target.ends_with(&format!("&{OFFSET_ON_WIRE}")));
    assert_eq!(requests[2].query_all("offset"), Vec::<String>::new());
    assert!(requests[2].target.contains(NEW_NAME_ON_WIRE));
    server.shutdown().await;
}

/// Airtable accepts field IDs in `fields[]` and in formulas, so options written with IDs do not
/// change when a field is renamed and a saved offset can be resumed with the same options. The
/// response stays keyed by the current field *name* (this SDK does not send
/// `returnFieldsByFieldId`), so the caller's reads still see the new name.
#[tokio::test]
async fn id_selection_survives_rename_and_resumes_saved_offset_but_keys_follow_names() {
    let script = Script::new([
        Reply::json(200, BEFORE),
        Reply::json(
            200,
            include_bytes!("../fixtures/maintenance/selected_by_id_after_rename.json"),
        ),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let by_id = ListOptions {
        fields: vec!["fldSynthName0001".into(), "fldSynthStage001".into()],
        filter_by_formula: Some("{fldSynthStage001} = 'Open'".into()),
        ..Default::default()
    };

    let bounded = api
        .list_pages("appMaint", "Tasks", by_id.clone(), Resume::Start, cap(1))
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        bounded.collected[0].items[0].fields[OLD_NAME],
        json!("Open")
    );
    let PagesError::Bounded {
        resume: saved @ Resume::At(_),
        ..
    } = bounded.error
    else {
        panic!("expected the page cap, got {:?}", bounded.error);
    };
    assert_eq!(saved.key().map(String::as_str), Some(OFFSET));

    // The field is renamed on the provider here; the caller's options stay as they were.
    let mut resumed = api.list_pages("appMaint", "Tasks", by_id, saved, cap(1));
    let page = resumed.next_page().await.unwrap().unwrap();
    assert_eq!(page.items[0].fields[NEW_NAME], json!("Open"));
    assert!(!page.items[0].fields.contains_key(OLD_NAME));
    assert!(resumed.next_page().await.unwrap().is_none());
    assert_eq!(resumed.finish().outcome, Outcome::Exhausted);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_get(request);
        assert_eq!(
            request.query_all("fields[]"),
            ["fldSynthName0001", "fldSynthStage001"]
        );
        assert!(request.query_all("returnFieldsByFieldId").is_empty());
    }
    assert_eq!(requests[1].query_all("offset"), [OFFSET]);
    assert!(requests[1].target.ends_with(&format!("&{OFFSET_ON_WIRE}")));
    server.shutdown().await;
}
