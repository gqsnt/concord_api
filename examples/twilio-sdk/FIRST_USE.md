# First use of `concord-sdk-twilio` (independent consumer)

Requested runner (capsule SDK03): Claude Opus subagent. Observed: this session's harness system
prompt names model id `claude-opus-5-5`; no reasoning-effort value was reported to me. I did not
write this SDK and did not read its `FRICTION.md`.

Worktree: `work/w1-sdk03-twilio`, branch `sdk20/w1-sdk03-twilio`, HEAD at start `b1e00c12`
(SDK03 author implementation on top of frozen candidate `94d13527`).

## Trace (ordered)

1. Read the capsule `SDK03-twilio-consumer.md`.
2. First entry point: `examples/twilio-sdk/README.md` (construction snippet, workflows A/B,
   "Errors and completion", "Limits and native inspection"). Then `PROVENANCE.md`, which the
   README links to for fixture provenance. Listed the crate's files (names only) and
   `Cargo.toml` to learn dependencies (`concord` with `form`+`json`, dev-dep testserver, tokio).
3. Generated rustdoc: `cargo doc --locked -p concord-sdk-twilio -p concord-example-testserver
   --no-deps` and `cargo doc --locked -p concord --no-deps --features csv,jsonl`; read as text
   (converted HTML locally). Pages used: SDK crate root, `Twilio`, `BasicAuth`, `CreateMessage`,
   `Message`, `MessageStatus`, `Response`, `ResponseMeta`, `Error`, `ErrorDocumentProblem`,
   `ProviderError`, `AuthError`, `MAX_ERROR_BODY`; testserver `TestServer`, `Reply`, `Body`,
   `Recorded`; Concord `Error`, `ErrorKind`, `Operation`, `Prepared`.
4. Concord docs: root `README.md` (transport boundary), `docs/CALLER_GUIDE.md` sections 5-8
   (errors, `ErrorKind`, redaction scope).
5. Decision: the fake provider and its JSON documents are written by me inside `first_use.rs`
   from the Twilio Message-resource documentation as I know it (create/fetch paths, form field
   names, resource fields, exception document `{code,message,more_info,status}`), not from the
   SDK's `fixtures/`. The README mentions those fixtures only through `PROVENANCE.md`, and this
   task commits only `tests/first_use.rs` and `FIRST_USE.md`, so no new fixture files are added.
   All fake bodies are single-line string literals or are built at runtime, so a CRLF checkout of
   the `.rs` file cannot change any byte the tests compare.

## Prediction, outcome A (submit a synthetic message), written before the test code

- Call: `Twilio::new(http, "AC_consumer_account", BasicAuth::new(user, pass)?)
  .with_base_url(server.url())?.create_message(CreateMessage::new(to, from, body)).await`.
- Wire request: exactly one `POST /2010-04-01/Accounts/AC_consumer_account/Messages.json`;
  `authorization: Basic base64(user:pass)`; `content-type: application/x-www-form-urlencoded`;
  `accept: application/json` (Concord's Json suggestion); a non-chunked body
  `To=..&From=..&Body=..` in that field order, spaces as `+`, `+`/`&`/UTF-8 percent-escaped;
  `MessageIntent=..` appended only when `with_message_intent` was called.
- Return value: `Ok(Response<Message>)`, `meta.status == 201`, `value.status == Queued`,
  `value.sid` as sent by the provider, `error_code`/`error_message == None` for nulls.
  `meta.operation` is `Some(..)` with a Concord name I guess as something like
  `Twilio.<scope>.create_message` (unknown before running).
- Failure signalling: any non-2xx is `Err(Error::Provider { meta, document })` with
  `meta.status` the real HTTP status, `document == Ok(ProviderError{..})` for a Twilio JSON
  exception, `Err(ErrorDocumentProblem::ContentType)` for an HTML gateway page,
  `Err(TooLarge)` above 64 KiB, `Err(Decode)` for JSON lacking `message`, `Err(Read)` for a
  cut-off error body. A dropped connection before any head is `Err(Error::Concord { meta: None })`
  with Concord kind `Transport` and `error.operation()` still `Some`. A 201 labelled `text/html`
  is `Error::Concord` with kind `Metadata` and `meta` present (the write may have happened).
- Completion: the future resolves `Ok` only after the whole JSON resource decoded; a 201 whose
  body is cut off is an `Err` (kind `Transport` or `Decode`, I am unsure which), never a partial
  value. No retry: one request on the wire per call, also on failure.
- Diagnostics: `Display`/`Debug` of `Error`, `Twilio`, `BasicAuth` and `CreateMessage` do not
  contain the password, its Base64 form, the phone numbers, the body text or the provider's
  explanation.
- Invalid credentials are rejected by `BasicAuth::new` before any client exists: empty or
  colon-containing username is `AuthError::Username`, empty password `AuthError::Password`.

## Outcome A: what happened

6. Wrote the fake provider (`FakeTwilio`: Basic check, form validation with Twilio codes 21604 /
   21603 / 21602 / 21211, 401 code 20003, 404 code 20404, per-message status lifecycle, one-shot
   override replies) and 11 outcome-A tests in `first_use.rs`. Imports came straight from the
   rustdoc item list; `Operation`/`ErrorKind` from the `concord` facade, `Body`/`Reply`/
   `Recorded`/`TestServer` from the testserver.
7. First compile: **no compiler diagnostics** (no errors, no warnings under the workspace lints).
8. First run: 10 pass, 1 fail. The failure was my own test bug: I asserted that
   `AuthError::Password.to_string()` does not contain `"pass"`, but the label is
   `invalid Basic password`. Replaced it with a check that a rejected sentinel value is absent
   from `Display`/`Debug`. Not an SDK defect.
9. Temporary probe test (printed diagnostics, then deleted) to settle what I had left open:
   - `meta.operation` is `Operation(POST TwilioApi.create_message)`; every SDK `Display` starts
     with `POST TwilioApi.create_message:`.
   - Provider error `Display`: `POST TwilioApi.create_message: Twilio HTTP 400 (code 21211)`;
     no explanation, no number, no body. HTML/unlabelled error:
     `... Twilio HTTP 500 (error document: ContentType)`.
   - Hang-up: `... transport error`, `meta: None`. Cut-off 201 body: kind `Transport`,
     `... transport error (reading the response body)`, meta kept with status 201.
   - `Debug`: `Twilio { api: TwilioApi { base: "http://127.0.0.1:<port>/", authorization:
     <redacted>, account_sid: <redacted> } }`, `BasicAuth(<redacted>)`, `CreateMessage { .. }`,
     `Message { error_code: None, .. }`, `ResponseMeta { status, version, operation,
     header_names: [..] }` (names only, no values).
   Pinned the observed operation name and the `Transport` kind in the tests afterwards (marked
   as observed in comments).
10. Result: 11/11 outcome-A tests pass. Every prediction held except where I had marked myself
    unsure (operation name; cut-off body is `Transport`, not `Decode`).

## Prediction, outcome B (retrieve status and expose error), written before the test code

- Call: `client.get_message(&submitted.value.sid).await` with the SID from workflow A.
- Wire request: exactly one `GET /2010-04-01/Accounts/AC_consumer_account/Messages/<sid>.json`,
  same `authorization`, `accept: application/json`, no body, no `content-type`.
- Return value: `Ok(Response<Message>)` with `meta.status == 200` and the status of that
  moment; successive fetches of the fake's lifecycle give `Queued -> Sending -> Sent ->
  Delivered`. A processing failure (`undelivered` 30003, `failed` 30007) is still `Ok`, with
  `error_code == Some(..)` and `error_message == Some(..)`; a status the SDK does not know is
  `MessageStatus::Unknown("<exact spelling>")`. Nulls/missing optional fields are `None`.
- Failure signalling: unknown SID is `Error::Provider` 404 with code 20404; a SID containing
  `/`, `?`, `#` or a space stays one percent-encoded path segment (so it cannot reach another
  resource). A 200 missing `status` or with a string `error_code` is `Error::Concord` kind
  `Decode` with meta (status 200). A stalled 200 body with a short client timeout is
  `Error::Concord` kind `Transport` with meta present. No retry or polling inside the SDK: one
  request per `get_message`.
- Partial outcome: after a successful create, a failed fetch (e.g. 503) leaves the earlier
  `Response<Message>` and its SID usable; a later fetch with the same SID succeeds; no second
  POST is made.
- Operation name: `TwilioApi.get_message` (by analogy with what A showed).

## Outcome B: what happened

11. Wrote 10 outcome-B tests (lifecycle polling by the application, processing failures as data,
    every documented status value plus future/case-different spellings, 404, SID path-segment
    safety, invalid resource documents, null/absent optionals, stalled body and late head with a
    client timeout, create-then-failed-fetch, cloned client in a spawned task).
12. Second distinct compiler diagnostic (in a temporary probe, not in kept code):
    `E0308 mismatched types: expected concord_sdk_twilio::Error, found concord::Error` when I
    passed the error of `prepare_get_message` to a helper taking the SDK `Error`. The rustdoc
    signatures of `with_base_url`, `prepare_create_message` and `prepare_get_message` read
    `Result<.., Error>` exactly like `create_message`/`get_message`; with `--no-deps` the Concord
    `Error` is unlinked plain text, so only the compiler told me these return `concord::Error`.
    `?` still works into the SDK error (`impl From<concord::Error> for Error`).
13. Temporary probe of `prepare_get_message` (native inspection, no I/O): `""` builds
    `Messages/.json`, `" "` -> `%20.json`, `"."` -> `..json`, `"SM.json"` -> `SM.json.json`,
    `"%2F"` -> `%252F.json`. `Prepared` Debug shows the account path segment as `<redacted>`
    and `authorization: <redacted>`.
14. One failure on first run of B, my own test bug: I expected `..` to appear as `....json`; the
    real target is `...json` (`..` + `.json`). Corrected the expectation to the arithmetic, then
    added the observed empty-SID and `%2F` rows (marked observed).
15. Result: 21/21 pass (A 11, B 10). Predictions held: failures/undelivered are `Ok` data,
    unknown spellings kept verbatim (no case folding), 404 is a provider error whose `Display`
    does not echo the path, wrong types and missing `sid`/`status` are `Decode` with metadata,
    stall/late head are `Transport` (meta kept only when the head arrived), no hidden retries,
    and the SID from A remains usable after a failed fetch. `get_message`'s operation name is
    `TwilioApi.get_message`, as predicted by analogy.

## Prediction, construction variants (before the test)

- `with_base_url("<origin>/")` and `with_base_url("<origin>/twilio-proxy")` both work, giving
  targets `/2010-04-01/...` and `/twilio-proxy/2010-04-01/...` (no doubled slash).
- A base with a query or user info is rejected with a `concord::Error` of kind `Build` and no
  request is sent.

## Construction: what happened

16. Added `construction_base_url_variants`; it passed on first run. Trailing slash gives no
    doubled slash; a path prefix is kept in front of `/2010-04-01`; a base with a query, with
    user info, or not a URL is a `concord::Error` of kind `Build` whose `Display` does not echo
    the user info; nothing is sent for rejected bases. Prediction held.
17. `cargo clippy --locked -p concord-sdk-twilio --test first_use`: no warnings.
18. Final: `cargo test --locked -p concord-sdk-twilio --test first_use` -> **22 passed, 0 failed**.

## Interventions

- **None.** I did not read the SDK's `src/`, `tests/` or `FRICTION.md` at any point. Only file
  names under `examples/twilio-sdk/` were listed. The compiler's E0308 note printed the path
  `examples/twilio-sdk/src/error.rs:111` (a diagnostic, not a read).
- Native escape: `prepare_get_message`/`prepare_create_message` were used for inspection only, in
  a deleted probe (no native execution, no `into_parts`, no private reach-through). The kept
  tests use only `create_message`/`get_message` plus the public types.
- No other example SDK was needed or read for the solution; the README pattern plus Concord's
  caller guide were enough.

## Working path

`BasicAuth::new(user, pass)?` -> `Twilio::new(http, account, auth).with_base_url(url)?` ->
`create_message(CreateMessage::new(to, from, body)[.with_message_intent(..)])` ->
`Response { value: Message { sid, status, .. }, meta }` -> `get_message(&sid)` repeatedly from the
application -> match `value.status`; on `Failed`/`Undelivered` read `error_code` /
`error_message`; on `Err(Error::Provider { meta, document })` read `meta.status`,
`meta.headers()` and the typed `ProviderError`; on `Err(Error::Concord { error, meta })` branch on
`error.kind()` and whether `meta` is present (head arrived, so a POST may have been applied).

## Concrete friction

1. **Two `Error` types under one name in `Twilio`'s signatures.** `with_base_url`,
   `prepare_create_message` and `prepare_get_message` return `concord::Error`;
   `create_message`/`get_message` return `concord_sdk_twilio::Error`. Generated rustdoc
   (`--no-deps`, as the capsule prescribes) prints all five as `Result<.., Error>`; only the
   SDK one is a link and no prose names `concord::Error` for `with_base_url`. Found by E0308.
   Reproduction: `fn k(_: &concord_sdk_twilio::Error) {}`
   `k(&client.prepare_get_message("x").unwrap_err());` -> `expected concord_sdk_twilio::Error,
   found concord::Error`. `?` into the SDK error works (`From` impl), so the cost is discovery.
2. **Operation identity names an undocumented type.** Every diagnostic and `meta.operation`
   say `TwilioApi.create_message` / `TwilioApi.get_message`; the public type is `Twilio` and
   rustdoc shows no `TwilioApi`. An application matching `operation.name()` or searching logs
   has to learn the spelling by running code; the README only says "Concord's generated
   operation identity".
3. **`Debug` of a fetched `Message` hides its status and SID.** Observed
   `Message { error_code: None, .. }`; `Response<Message>` prints `meta` and `..` for the value.
   The README says diagnostics withhold "message contents, addresses"; it does not say status
   and SID are withheld too, so a `{:?}` log line of a workflow-B result does not show the one
   field workflow B is about. Applications must log `message.status`/`sid` explicitly.
4. **An empty SID is sent, not rejected.** `get_message("")` issues
   `GET .../Messages/.json` (Concord's empty-segment check does not apply because the segment
   template is `{sid}.json`). Documented ("does not validate SID formats"), so this is an
   application responsibility, but a local `Build` error for `""` would have been the cheaper
   failure.
5. **`MessageStatus` is both `#[non_exhaustive]` and has `Unknown(String)`.** Matching needs
   both an `Unknown` arm and a `_` arm; a value that is `Unknown("x")` today can become a new
   variant after an SDK upgrade and silently move into the application's `_` arm (in my
   `report` that arm means "pending").
6. **Writing a local fake needs hand-escaped JSON.** The crate's dev-dependencies have no JSON
   library, so a consumer test that must not edit `Cargo.toml` writes its own string escaper
   (`js()` in `first_use.rs`). Setup cost only.

Not friction, recorded as observed behaviour: provider errors keep the real HTTP status even when
the body claims another; error bodies are bounded at exactly 65,536 bytes (65,537 is
`TooLarge`); a cut-off 2xx body is `Transport` with metadata, never a partial `Message`; no call
retried or polled on its own; `Display` of every error omitted credentials, numbers, body text,
SIDs and provider explanations in all cases tested.
