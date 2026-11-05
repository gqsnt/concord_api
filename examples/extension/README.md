# concord-example-extension

Downstream example (not published): an independent crate adds a Catalog operation that the SDK
([`examples/sdk`](../sdk)) does not declare — a tenant's CSV export,
`GET tenants/{tenant}/exports/{export}` — without editing the SDK and without any Concord change.
It is tested against the fake Catalog service of [`examples/fixture`](../fixture), whose export
route exists only there.

## What it does

| Item | Role |
|---|---|
| `export(&tenant, id)` | one request; returns `Vec<CsvRow>` |
| `export_with(&tenant, id, \|request\| ..)` | the same, with a native edit (a header, a timeout) before its one execution |
| `export_parts(&tenant, id)` | `(reqwest::Client, reqwest::Request, Decoder<Vec<CsvRow>>)`, for a caller that executes the request itself (to read `x-export-id` before decoding, say) |
| `Csv`, `CsvRow` | the operation's own representation: a `concord::Decode<Vec<CsvRow>>` that refuses a non-`text/csv` `Content-Type` before the body is read and reports static labels with line/column numbers |
| `ExtensionError` | the operation's error: its name (`catalog-extension: tenant export`) plus either the SDK's `ContextError` (nothing sent) or Concord's `Error` |

The request is built by the SDK's tenant context, gets `Accept: text/csv`, is executed once with
the context's `reqwest::Client`, and is finished by `Decoder::<Vec<CsvRow>>::new::<Csv>()`:
`decoder.finish(client.execute(request).await)`. A transport failure therefore goes through
Concord's conversion (URL removed), a non-2xx is a `Status` error, a JSON-labelled 2xx is a
`Metadata` error with the response unread, and a malformed body is a `Decode` error.

## The boundary it relies on

Only public items: the SDK's [`context`](../sdk/src/context.rs) module (frozen before this crate
was written) and Concord's `Decoder::new`, `Decode`, `CodecError` and `Error`.

- `CatalogContext::new(http, CatalogConfig { .. })`, `with_base_url`, `set_region`/`maybe_region`
  configure one context; `catalog()` and `tenant(t).catalog_tenant()` are the generated handles
  of the same values, `tenant(t).request(method, &[segments])` the native request for an
  operation the SDK does not declare, `http()` the client to execute it with.
- A built request keeps the base path prefix, encodes the tenant and each segment as one path
  segment exactly as a generated endpoint does (an SDK test compares them over every ASCII
  character, controls, percent escapes and Unicode), refuses empty, `.` and `..` segments with a
  typed `ContextError` before any I/O, and carries `x-client`, the sensitive `x-api-key` and
  `x-region`.
- It is not a generated endpoint: no endpoint name, no Concord redaction. The native request's
  `Debug` and URL show the tenant and export id. Do not log them when they may be secret.

The decoder is unlabeled, so `concord::Error::endpoint()` is `None`; the operation is named by
`ExtensionError::operation()`, never by a declared endpoint.

## Compared with the archived suffix helper

The earlier campaign's consumer (an `archive(items, id)` helper) prepared the SDK's declared
detail endpoint as a seed, took it apart with `into_parts()`, pushed `archive` onto its URL,
removed the seed's JSON `Accept`, and decoded with a fresh public decoder. That works only for a
route that extends a declared URL and inherits the seed's identity:

| | Suffix of a seed endpoint | SDK context (this crate) |
|---|---|---|
| Route | only `{declared URL}/suffix`; `tenants/{t}/exports/{e}` is not a suffix of any declared Catalog URL | any path under the base or a tenant |
| Request | the seed's values, headers and `Accept` must be undone by hand | built for this operation; nothing to undo |
| Representation | the seed's decoder is stale and discarded (or wrongly kept) | the operation's own `Csv` decoder from the start |
| Identity | a kept seed decoder would label errors with the seed endpoint's name | `endpoint()` is `None`; the operation names itself |
| Redaction | the seed's metadata described another route; after `into_parts` it no longer applies anyway | none claimed: documented native boundary |
| Requests sent | one: the seed endpoint's request, rewritten | one, built for this route |

## Tests

`cargo test -p concord-example-extension` — `tests/extension.rs`, every test against the fixture
with request counts:

| Acceptance | Tests |
|---|---|
| X01 absent from the SDK, public APIs only, no seed | `x01_the_sdk_declares_no_export_route`, `x01_the_extension_uses_no_hidden_api_and_no_declared_endpoint`, `x01_one_request_to_the_export_route_and_nothing_else`; the crate doc's `compile_fail` example |
| X02 base prefix, path data | `x02_a_base_path_prefix_is_kept`, `x02_hard_tenant_and_export_data_stay_one_segment`, `x02_the_encoding_agrees_with_the_generated_endpoint`, `x02_empty_and_dot_data_fail_before_http` |
| X03 independent contexts, headers on the wire | `x03_contexts_from_one_base_stay_independent` |
| X04 own representation, retained decoder, identity | `x04_the_csv_decoder_is_kept_when_the_caller_changes_accept`, `x04_a_json_labelled_response_is_refused_unread_with_a_truthful_identity`, `x04_a_malformed_body_is_a_decode_error_with_positions_only`, `x04_the_csv_format_checks_its_head_and_body` |
| X05 native edit, transport failure | `x05_a_native_edit_adds_a_header_before_one_execution`, `x05_a_transport_failure_does_not_restore_the_url` |

The tenant `acme/east?x=1%20+ü #` reaches the service as the single segment
`acme%2Feast%3Fx%3D1%2520%2B%C3%BC%20%23` under `/edge/catalog/v1/tenants/`, the same segment the
SDK's generated `item` endpoint sends.
