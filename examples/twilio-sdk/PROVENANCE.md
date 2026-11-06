# Source and fixture provenance

Access date for every official source below: **2026-11-06**. Resource version:
**Twilio REST API `2010-04-01`**. Twilio's documentation pages are mutable and expose
no document revision used here; the pin is the API version plus this access date,
not a vendor SDK release or an upstream documentation commit.

Sources re-opened during this task:

- **M:** [Messages resource](https://www.twilio.com/docs/messaging/api/message-resource):
  Create a Message resource / Request body parameters; Fetch a Message resource /
  Response; Message Properties; Message Status values.
- **A:** [Twilio API requests](https://www.twilio.com/docs/usage/requests-to-twilio):
  authentication and Use your Account SID and Auth Token.
- **E:** [Twilio API responses](https://www.twilio.com/docs/usage/twilios-response):
  Status codes; Exceptions; 400 Bad Request; 404 Not Found.
- **20003:** [Permission Denied](https://www.twilio.com/docs/api/errors/20003), Description.
- **20429:** [Too many requests](https://www.twilio.com/docs/api/errors/20429), Description.
- **30007:** [Message filtered](https://www.twilio.com/docs/api/errors/30007), Description.

All files below are **hand-authored**, not captured provider traffic and not verbatim
downloaded examples. M's rendered fetch response example was read, but these files
were written independently using its field schema. Every row inherits the version
and access date above. No credentials, vendor SDK or live provider calls were used.

| Fixture under `fixtures/` | Official page/section | Authorship and transformations |
|---|---|---|
| `create.form` | M, Create / Request body parameters | Literal UTF-8 form expectation: synthetic numbers and text, spaces as `+`, literal plus/ampersand and UTF-8 bytes percent-escaped. No trailing newline. Not produced by the SDK encoder. |
| `create-intent.form` | M, Create / `messageIntent` | Baseline plus one optional field. `通知 & + café` is an intentionally synthetic encoding stress value; acceptance as an intent is unverified. No trailing newline. |
| `create-empty-intent.form` | M, Create / `messageIntent` | Baseline with literal `&MessageIntent=` appended; probes empty versus omitted, not provider acceptance. No trailing newline. |
| `created.json` | M, Create response / Message Properties | Hand-authored resource shape; synthetic SID/account, addresses, text and dates, queued status and null error fields. Some documented extra fields exercise unknown-field tolerance. |
| `undelivered.json` | M, Fetch / Response and status values; 30007 | Reduced resource with synthetic processing-failure explanation and `30007`. Uses `undelivered`, rather than copying the rendered fetch example's `sent` status. |
| `delivered.json` | M, Fetch / Message Properties and status values | Reduced synthetic observation, null body and errors. No real delivery occurred. |
| `unknown-status.json` | M, Fetch / Message Properties | Synthetic future status and extra field; omits nullable properties to test evolution. |
| `bad-request.json` | E, Exceptions / 400 Bad Request | Hand-authored flat JSON equivalent of the described exception schema; `21201` from the displayed example, rewritten explanation containing a diagnostic sentinel. |
| `unauthorized.json` | E, Status codes / Exceptions; 20003 | Synthetic HTTP 401 exception using documented code and rewritten explanation. |
| `not-found.json` | E, Exceptions / 404 Not Found | Synthetic flat JSON error with optional code/link absent. |
| `rate-limited.json` | E, Status codes / Exceptions; 20429 | Synthetic HTTP 429 exception with documented code and rewritten explanation. |
| `malformed.json` | M, resource JSON; E, exception JSON | Deliberately cut-off JSON used for both success and error decoding negatives. No trailing newline. |
| `missing-status.json` | M, Fetch / Response | Synthetic invalid SDK resource: identity present, required status removed. |
| `wrong-error-code.json` | M, Message Properties / `errorCode` | Synthetic invalid response: string instead of numeric code. |
| `not-an-error.json` | E, Exceptions | Synthetic invalid exception: required message absent. |
| `html-error.html` | No vendor example; negative against M/E JSON expectations | Fully synthetic gateway HTML and diagnostic sentinel. |

Except where noted, fixture files end with LF. `fixtures/** -text` in
`.gitattributes` prevents checkout newline conversion. Expected forms are checked
as byte slices, not decoded pairs or expectations generated with the encoder.

## Other synthetic evidence

`tests/support/mod.rs` uses `fixture-user` / `fixture-password`, with a literal
expected Basic header, informed by A. Authentication values deliberately do not
resemble valid Twilio credentials. `AC_fixture_account` and `SM_fixture_message`
are visibly fake, nonconforming identifiers; this SDK leaves SID validation to the
provider. Request assertions with slashes, spaces, `?` and `#` are path-injection
negatives, not valid Twilio SIDs. The fixture phone numbers are never contacted.

Test response headers (`Twilio-Request-Id`, `Retry-After`, duplicate `X-Fixture`),
HTTP/body status disagreement, missing/duplicate/wrong media types, hangups and
unterminated chunked bodies are scripted test choices. Header presence/value is
not claimed as a vendor guarantee. The body-limit test appends JSON whitespace to
`bad-request.json` at runtime to 65,536 and 65,537 bytes, so it needs no large fixture.

No fixture establishes vendor conformance, delivery, callbacks or billing behavior.

## Concord material and dependency choices

Baseline inspected: `94d13527`; pinned Rust toolchain `1.99.1`, edition 2024.
Read the root README, caller/author guides, recipes, core/extras README files and
public response/codec rustdoc; generated the requested Concord rustdoc with
`--features csv,jsonl`. No frozen library or other example was edited.

Read `examples/stripe-sdk/src/send.rs` and declaration/manifest, and GitHub's
`src/error.rs`. **Stripe supplied the response-adapter pattern**: `receive`, capture
metadata, branch on status, decode success or bound an error body. This SDK adapts
it to Twilio's flat exception schema and checks error Content-Type too. GitHub's
typed error accessors/redaction were additional reading. No provider HTTP execution
was delegated to a vendor SDK.

- `concord` (path): public `api!`, `Prepared`/`Received`, request execution, `Form`
  and `Json`. No direct core/extras dependency, custom form encoder or pagination
  dependency is necessary for these two finite single-resource operations.
- `serde` (workspace, derive): typed form field names/omission and resource decoding.
- `base64` **0.23.1**: mature generic Base64 implementation for Basic auth, already
  present in the lockfile. Avoids a handwritten Base64 codec; it does not execute HTTP.
- Dev dependencies: `concord-example-testserver` by path for loopback fixtures and
  `tokio` for async tests/timeouts. No new registry package/version was added to the
  final lockfile; only the new SDK package entry was added.
