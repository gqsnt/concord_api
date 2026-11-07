# Friction

Costs observed while writing this SDK against the frozen baseline `8c27cd1a`. No shared crate
was edited; each item says what the smallest Concord change would be. Line references are to
this package after `cargo fmt`.

## What worked without hand-written code

- client → scope → endpoint: `api.tickets().list()`, `api.tickets().create(..)`,
  `api.ticket(id).show()`, each with generated `send()` through `send policy;`.
- `subdomain "{subdomain}"` with `path "api/v2"`: Concord's label rules refuse `acme.evil`,
  `-acme`, `ac_me` and `""` as `Build` errors before any I/O (`protocol.rs`,
  `invalid_inputs_are_refused_before_anything_is_sent`); `with_base_url` plus `resolve` reaches
  the local server with the subdomain intact.
- `query("page[size]") page_size: PageSize = PageSize::MAX` keeps cursor mode impossible to turn
  off; `limit` on the client, a scope and an endpoint bounds 8 MiB pages and 1 MiB tickets.
- `Received::document::<F, D>(limit)` kept the status for every unreadable error body (other
  media type, oversized, malformed, truncated) with no extra code.
- `concord_auth::bearer` validated and flagged the token; `concord_retry::http::retry_after`
  parsed `Retry-After` (seconds and dates); `concord_pagination::Pages` gave bounded traversal
  with a resume cursor.

## Items

### F1. A native credential constant makes a sibling literal header sensitive

- **Evidence.** With `#[sensitive] header(AUTHORIZATION) authorization: HeaderValue` (the
  guides' idiom), the generated rustdoc of `ZendeskTicketsCreate` said "Headers, in chain order:
  AUTHORIZATION (sensitive), idempotency-key (sensitive)", and `set_idempotency_key` said
  "Sensitive: ... withheld from Concord's Debug output". The macro cannot tell whether the
  native path and the literal `"idempotency-key"` name the same header, and promotes the
  literal's text value to sensitive (`crates/concord-macros/src/model.rs`, the `Rel::Unknown`
  branch and the promotion after it).
- **By hand.** Nothing; `src/lib.rs:47` uses the literal `header("authorization")` instead, so
  the idempotency key stays readable in `Prepared`'s `Debug` (asserted in
  `the_token_and_provider_text_are_never_formatted`).
- **Generic.** Entirely Concord's classification; it errs toward more redaction, which is safe
  but hid a debugging aid.
- **Smallest change.** Say it in AUTHOR_GUIDE §4.8/§7: a literal header sharing a chain with a
  sensitive native constant is withheld too; spell the credential as a literal when other
  headers must stay readable.

### F2. Honouring `Retry-After` through `Retry::receive` is mostly plumbing

- **Evidence.** `src/policy.rs:72-96`: seven arguments, of which the hook (no-op), the clock
  (an `Instant` epoch), the sleep (`tokio::time::sleep`) and the jitter (identity, the backoff
  ceiling being zero) are generic. The provider part is `classify_read` (`policy.rs:132-153`:
  429, or 503 with a parsed `Retry-After` within the cap). The hint is parsed twice, in the
  classifier and again for the returned `HttpError`, because the `Decision` does not travel with
  the final outcome. `run.outcome == None` needs its own error (`Error::DeadlineExceeded`).
- **Smallest change.** `Retry::receive_tokio(prepared, mode, hook, classify)` under
  concord-retry's `tokio` feature, mirroring `run_tokio`.

### F3. Replay permission is inferred from the HTTP method

- **Evidence.** `SendWith::send` (`policy.rs:155-167`) branches on
  `E::operation().method() == Some("GET")`. The declaration has no way to say "safe to repeat"
  or "write with an idempotency key".
- **Provider vs generic.** Which operations may repeat is provider semantics; for this slice
  the method rule is exact (two GETs, one POST).
- **Smallest change.** None required; an endpoint attribute surfaced on `Operation` would only
  pay for SDKs whose safe operations are not GETs.

### F4. The policy reads the idempotency key back from the native request by name

- **Evidence.** `policy.rs:103-108` reads `"idempotency-key"` from
  `prepared.request().headers()` and re-validates it with `IdempotencyKey::new`, repeating the
  header name of `lib.rs:83`. A mismatch compiles and silently leaves `UncertainWrite` without
  its key; only the test `a_write_without_a_response_is_uncertain` would notice.
- **Generic.** `SendWith` receives an `Endpoint` (`prepare`, `operation`), never the declared
  values.
- **Smallest change.** None in core for one header; record only.

### F5. A followed `links.next` cannot go back through the client's send policy

- **Evidence.** The pagination page (D2) says to request `links.next`. `Prepared::follow(url)`
  exists, but the policy is reached only through generated `send()`, that is
  `SendWith::send(E: Endpoint<T>)`, and `Endpoint::operation()` takes no `self`, so a wrapper
  `Endpoint` around a followed `Prepared` cannot report its operation faithfully. Following the
  URL would have bypassed the read retry and the SDK's error type.
- **By hand.** `src/pagination.rs:39-61`: the next request is the template with `page[after]`
  set to `meta.after_cursor`, and `links.next`, when present, must name the same cursor
  (`InconsistentNextLink` otherwise).
- **Provider vs generic.** Choosing and checking the cursor is provider work; "send a followed
  request through the policy" is generic.
- **Smallest change.** A generated `follow(url)` on endpoint request types whose `send()` still
  goes through the client's policy.

### F6. `concord-pagination` does not refuse a repeated key

- **Evidence.** `pagination.rs:47-49`: a page that names the cursor that produced it would be
  requested again until `max_pages`; the SDK refuses it (`RepeatedCursor`, tested in
  `pages_that_cannot_be_followed_end_the_traversal`).
- **Generic.** Comparing the next key with the requested one needs only `K: PartialEq`.
- **Smallest change.** An opt-in `Pages::refuse_repeated_key()` with its own `PagesError`.

### F7. An error format accepting two media types re-parses `Content-Type`

- **Evidence.** `src/codecs.rs:58-95`: D5 documents `application/json` and `text/plain` error
  bodies. `concord_codecs::check_content_type(head, &[..], Charset::Utf8)` validates the head
  but does not say which entry matched, so `context` parses the essence again
  (`codecs.rs:65-79`). Those helpers are not re-exported by `concord`, so the package depends on
  `concord-codecs` directly for them.
- **Smallest change.** A variant of `check_content_type` returning the matched entry's index,
  and re-exporting `check_content_type`/`Charset` from `concord`.

### F8. Envelopes are unwrapped by the caller

- **Evidence.** Show Ticket decodes `TicketResponse { ticket }`; callers write
  `.send().await?.ticket`. Create uses a format of its own (`codecs.rs:16-47`, delegating to
  `Json`'s `check_content_type` and `decode_body`) because it also reads `x-idempotency-lookup`
  from the head, Concord's documented route for header-dependent decoding.
- **Smallest change.** None; a format per envelope would cost more than the field access.

### F9. A send-policy value cannot have a default

- **Evidence.** The macro requires `send value;` to name a required value ("a send policy value
  must be required (no `?`, no default)"), so `ZendeskConfig` has a `policy` field every caller
  of `Zendesk::new` fills with `SendPolicy::default()`. `Zendesk::connect` (`lib.rs:100-122`)
  exists to validate the token and to hide that field.
- **Smallest change.** Allow `policy: SendPolicy = SendPolicy::default()` for a `send` value,
  evaluated once in `new`, still replaceable with `set_policy`.

### F10. "Not sent" versus "maybe sent" rests on `reqwest::Error::is_connect`

- **Evidence.** `policy.rs:113`: a connect failure is reported as `Error::Concord` (nothing
  left the process), every other transport failure of a write is uncertain. Concord's
  `Transport` error has no notion of "request written"; reqwest and hyper expose none either.
- **Smallest change.** None in Concord; record only.

## Shared crates considered

| Crate | Used | Why / why not |
|---|---|---|
| `concord-auth` | `bearer()` | Validation and the sensitive flag. `TokenCache` is not used: the application supplies the OAuth access token, and acquisition or refresh is outside the slice. |
| `concord-retry` | `Retry::receive`, `http::retry_after` | Bounded read attempts with the server delay as a minimum and no local backoff; writes never enter it (`Replay` is not even requested for them). |
| `concord-pagination` | `Pages` | Dependent cursor chain, page cap, resume cursor, typed provider errors. |
| `concord-ratelimit` | no | Its windows and cooldowns are process-local. Zendesk's quotas are account-wide and shared with every other integration and process (D3, D7), so a local window would not predict them. The slice asks for `Retry-After` on the read that received it, which `concord-retry` covers. Spreading one 429 to concurrent callers would need an SDK-owned limiter and key mapping that no workflow here needs. |
| `concord-codecs` | `check_content_type`, `Charset` | For the two-media error format (F7). |

## Examples consulted (example-assisted)

| File | What was taken |
|---|---|
| `examples/fred-sdk/Cargo.toml` | The dependency layout: path dependencies on the shared crates, `concord-example-testserver` as a dev-dependency. |
| `examples/fred-sdk/src/lib.rs` | The shape of a `SendWith` policy (prepare → receive → `document::<F, D>` for non-2xx → decode), `http_client()` without reqwest retries or redirects, and the `#[cfg(doctest)] include_str!("../README.md")` check. |
| `examples/fred-sdk/src/error.rs` | An `HttpError` holding `ResponseMeta`, `Result<Problem, concord::Error>` and the parsed `Retry-After`, boxed in the error enum. |
| `examples/fred-sdk/src/pagination.rs` | A `Pages::new` adapter that clones a generated request template and sets the key. |
| `examples/fred-sdk/src/codecs.rs` (lines 1-80, 180-221) | An error-document format built on `concord_codecs::check_content_type`. |
| `examples/fred-sdk/tests/workflows.rs` (first 120 lines) | Fixture responders keyed by path and query; `server.shutdown().await.open_after == 0`. |
| `examples/linear-sdk/src/lib.rs` (first 60 lines) | A `#[sensitive]` `HeaderValue` credential as a client value. |
| `examples/twilio-sdk/src/{lib.rs,auth.rs}` (grep of constructor lines only) | Wrapping a `concord-auth` credential constructor in an SDK constructor. |
| `examples/testserver/src/lib.rs` | The test server API (`Reply`, `Body`, `Script`, `Recorded`). |

A grep over `examples/*/Cargo.toml` and `examples/*/src` showed that no provider SDK in the
baseline used `Retry::receive`; this package is the first adopter of that adapter.
