# Compact downstream compatibility fixture

Edition 2021 downstream consumer of Concord's public API, tested only against a local fake
HTTP/1.1 server. The fake server checks requests Concord sends and the caller's observations;
it proves nothing about any real service. No live services or credentials are used. Transport
has proxies and redirects disabled and a five-second timeout. `compat.invalid` is a declaration
placeholder; every executed request replaces it with a loopback base.

## 1. Response metadata with typed decoding (`tests/metadata.rs`)

Shows a private `client Metadata`, declared and used inside the library. Its wrapper returns
`Prepared<Document>`. The caller uses `into_parts()`, executes the native request, reads `ETag:
v7`, and calls the endpoint's `Decoder::decode(response)` to retain its declared JSON decoding.
A 409 is retained as an error: `Error::response()` borrows status and `ETag: v8`, then
`into_response()` supplies the unread body for `<Json as Decode<Document>>::decode`.
Outside facts: **synthetic**, modelled on a document service and not verified here.

## 2. An application error inside a 2xx (`tests/application.rs`)

Shows both explicit routes for `{"ok":false,"error":"synthetic-detail"}` with HTTP 200.
The JSON endpoint returns a typed envelope for the caller to check; `response Checked;` names
our `Decode<Envelope>` marker, which returns a `BodyError` with the content-free description
`application rejected request`. The detail is accessible through `Error::body_source()` and
absent from error Display/Debug. The custom decoder also accepts an `ok: true` envelope.
Concord does not inspect bodies for error markers; this policy is entirely our decoder's.
Outside facts: **synthetic**, modelled on an envelope-based service and not verified here.

## 3. Flat versus nested forms (`tests/forms.rs`)

Relies on opt-in `request Form;` and a custom `Encode<Nested>` marker with
`CONTENT_TYPE = Some("application/x-www-form-urlencoded")`.
The flat struct sends exactly `title=blue+pen%26ink&order=7`. The nested struct fails before
another request reaches the server. The exact observed Display is:

```text
POST Forms.nested: request not built: body: form encode error
```

The underlying source says `unsupported value`. Initial triage expected the same error without
`body:`; the test now records the actual complete output, without relaxing the assertion.
`request Brackets;` explicitly writes the bracket key and sends exactly
`metadata%5Border%5D=7` (decoded key `metadata[order]`).
Outside facts: **synthetic**, modelled on a metadata-accepting form service and not verified here.
The bracket convention is synthetic, not a built-in nested-form behavior or a verified service rule.

## 4. A multi-segment resource key (`tests/resource_key.rs`)

Relies on the native exit and `request.url_mut().path_segments_mut()` to replace a placeholder
with separate segments. Ordinary Concord path values always occupy one segment. Exact targets:

| Input | Ordinary path value | Native split into segments |
|---|---|---|
| `docs/2026/report.md` | `/objects/docs%2F2026%2Freport.md` | `/objects/docs/2026/report.md` |
| `docs/encoded%2Fname.md` | `/objects/docs%2Fencoded%252Fname.md` | `/objects/docs/encoded%252Fname.md` |

A percent escape supplied as data stays data: `%` becomes `%25` even in the native segment API.
Outside facts: **synthetic**, modelled on an object/document service and not verified here.

## 5. JSON with apply-patch YAML media type (`tests/apply.rs`)

Shows a Kubernetes-style PATCH with `request Json "application/apply-patch+yaml";` and a
`fieldManager` query value. The request uses the exact declared media type and body bytes
`{"apiVersion":"v1","kind":"ConfigMap"}`. This minimal serialization fixture is not a valid
complete apply object and does not exercise a Kubernetes server.
Outside facts: **sourced**, from the supplied [Kubernetes serialization documentation](https://kubernetes.io/docs/reference/using-api/server-side-apply/#serialization)
(JSON is permitted with this YAML media type); the URL was not fetched here, as instructed.
The payload and server response are synthetic.
The observed raw target is
`/api/v1/namespaces/default/configmaps/demo?fieldManager=compat+manager`.
The `+` for the space is Concord's observed query serialization, not an assumption about a service.
This is the fixture's only query value containing a space.

## 6. A fresh idempotency value per handle (`tests/idempotency.rs`)

Relies on endpoint initializers running at request-handle construction. A function outside the
macro generates distinct atomic counter values. Two endpoint calls and an explicit clone of the
first produce wire keys `fixture-1`, `fixture-1`, `fixture-2`: the clone is the deliberate retry
path. The client-initializer pitfall then produces `fixture-3`, `fixture-3` for separate requests.
No automatic retry occurs. Counter values are deterministic fixture values, not production keys.
Outside facts: **synthetic**, modelled on a payment service and not verified here; no service
idempotency guarantee is asserted.

## 7. Refreshing a base or root value (`tests/refresh.rs`)

Relies on `with_base_url` and `set_token` returning new snapshot handles. An earlier scope sends
to the old loopback server with `old-token`; a scope from the refreshed handle sends to the new
server with `new-token`. The original client also retains its base and token. Both servers check
paths, host/port, and the authorization value.
Outside facts: **synthetic**, modelled on an authenticated account service and not verified here.

## 8. Native and literal headers, media types, sensitivity (`tests/headers.rs`)

Relies on native header identity: `CT = CONTENT_TYPE` removes the inherited literal
`"content-type"`, restoring the JSON body's `application/json`. A sibling overrides through
`CT`, sending `application/custom`. `GATE` re-exports `AUTHORIZATION`; sensitive values are
sent under that actual header, including an override through the literal spelling.
`local::AUTHORIZATION` holds `x-other` and sends `ordinary`, demonstrating resolution by value.
A `#[cfg(any())]` endpoint with unavailable types is beside the compiled endpoints.

For each compiled endpoint, the tests assert wire values and native sensitivity flags:
authorization is sensitive; content-type and x-other are not. Both fake secrets are absent from
client, request-handle and `Prepared` Debug, and from status-error Display/Debug. The server's
403 creates an actual status error, and JSON body bytes are checked too. Sensitivity flags are
caller-side metadata; they are not encoded on the wire.
Outside facts: **synthetic**, modelled on an authenticated JSON service and not verified here.

A second test (`inherited_literal_content_type_with_an_optional_body`, added by the coordinator)
declares a client-wide literal `"content-type"` and an optional JSON body: the header is sent
whether or not the body is, and labels the JSON body when it is (author guide §5.3). A literal
`content-type` is deliberately not rejected; a declared media type plus a reaching `content-type`
header is the compile error.

## 9. JSON or nothing (`tests/json_or_empty.rs`, added by the coordinator)

An endpoint that answers `200` with a JSON document or `204` with no body. Concord's `json`
format keeps an empty body an error (JSON has no empty document), so the application declares a
small format of its own: `JsonOrEmpty` implements `Decode<Option<T>>`, maps an empty or
whitespace-only body to `None` and decodes anything else as JSON; the endpoint names it with
`response JsonOrEmpty;` and returns `-> Option<Playback>`. A malformed body is still an error.
Outside facts: **synthetic**, modelled on a media player service and not verified here.

## Validation

Run offline from the workspace root after updating the lock entry:

```sh
cargo test -p concord-example-compat --locked --offline
cargo clippy -p concord-example-compat --locked --offline --all-targets -- -D warnings
cargo fmt -p concord-example-compat -- --check
cargo +1.85 test -p concord-example-compat --locked --offline
```

The lock change adds only this workspace package, using already locked dependencies.
