# Airtable records over Concord

Read table records using Airtable's v0 GET endpoint: traverse opaque offset pages or pass a filter, selected field names, a view and page/record limits. Dynamic fields are JSON values keyed by the provider's field names. Airtable omits empty-valued fields: a missing key is not evidence of a schema or a default value. Writes, schema administration, and formula semantics are outside this SDK. This GET-only implementation does not automatically switch to POST for long queries; Airtable documents a 16,000-character URL limit.

Construct with a caller-configured HTTP transport and bearer token. A redirect policy of `none` avoids forwarding credentials or query data to another destination. Set timeouts suitable for your application. Only trust a replacement base URL that may receive the token.

```no_run
use concord_sdk_airtable::{Client, ListOptions, pagination::Resume};
use std::{num::NonZeroUsize, time::Duration};
# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let http = concord::reqwest::Client::builder()
    .timeout(Duration::from_secs(30))
    .redirect(concord::reqwest::redirect::Policy::none()).build()?;
let client = Client::new(http, "SYNTHETIC_TOKEN_NOT_A_CREDENTIAL");
// Workflow A: lazy page traversal.
let mut pages = client.list_pages("appExample", "Tasks", ListOptions::default(), Resume::Start, NonZeroUsize::new(100).unwrap());
while let Some(page) = pages.next_page().await? {
    for record in page.items { println!("{}", record.id); }
}
// Workflow B: filter and selection; Airtable evaluates the expression.
let options = ListOptions { filter_by_formula: Some("{Status} = 'Open'".into()), fields: vec!["Name".into(), "Status".into()], ..Default::default() };
let page = client.list_page("appExample", "Tasks", &options, None).await?;
assert!(page.value.records.iter().all(|r| !r.id.is_empty()));
# Ok(()) }
```

`list_page` returns a complete bounded JSON page and response metadata. Non-2xx responses retain the actual status, headers, optional typed provider error and bounded raw body or body-read error. Decode and transport failures are distinct, retaining metadata when available. No retry, rate limiting or snapshot consistency is supplied. The 16 MiB success and 64 KiB error body limits are SDK limits.

`list_pages` makes no request until polled. `Ok(None)` means exhaustion; a page cap or failure is an explicit `PagesError` with a resume position. `collect_complete` preserves a partial prefix on failure; `items().into_stream()` yields records incrementally by page and loses page metadata. Resume only with the same origin, token, base, table and options; offsets may expire. Stopping or dropping a traversal does not establish completion. Fixtures are synthetic offline examples, not vendor conformance tests.

Field renames and unknown fields: record keys are the field names in each response (this SDK does not send `returnFieldsByFieldId`), and the SDK keeps no schema. After a rename, later records, including later pages of the same traversal, carry the new name and the old key is absent, which looks the same as an empty value. Fields you have not seen before arrive as JSON values; record properties the SDK does not model (such as `commentCount`) are ignored. `fields` and `filter_by_formula` are sent verbatim, and Airtable accepts field IDs in both, so options written with IDs do not change when a field is renamed. A selection or formula that still uses an old name is for Airtable to reject: you get `Error::Http` with the provider's type string kept, and a traversal ends with `PagesError::Provider` whose `failed` position is the exact offset. Because an offset is resumed only with the options it was issued for, restart from `Resume::Start` after changing options. Offsets are sent back exactly as decoded from the JSON response, query-encoded once (reserved characters, `%`, `+` and non-ASCII included), and never trimmed or rewritten.
