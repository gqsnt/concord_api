# Friction

Costs observed while writing this SDK against the frozen baseline
(`14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`). No shared crate was edited; each item names the
smallest Concord change that would remove it, as a proposal only. "Provider" marks PayPal's
semantics, which stay in the SDK; "generic" marks mechanics another SDK would repeat.

## 1. The token endpoint needs a second declaration of the same host

- **Evidence.** `src/auth.rs` declares `pub client OAuth` with `base
  "https://api-m.sandbox.paypal.com"`, the same literal as the `PayPal` declaration in
  `src/lib.rs`, plus the `SANDBOX` constant: three spellings of one fact. `Builder::build` passes
  one base string to both clients' `with_base_url` (`OAuth::for_base`, `Session::new`); only the
  main declaration's base is checked by a unit test (`declared_base_is_the_documented_sandbox`).
- **Why.** `send session;` applies to every endpoint of the client, and the token endpoint must
  not go through it (it is what the policy calls, with Basic credentials, and answers identity
  errors `error`/`error_description` instead of `name`/`debug_id`). There is no per-scope `send`
  and no way to share a `base` between declarations.
- **By hand.** The OAuth declaration (13 lines) and `OAuth::for_base` (14 lines).
- **Provider / generic.** Different auth scheme and error format: provider. One client-wide policy
  with no opt-out: generic (every OAuth client-credentials SDK has a token endpoint on the API
  host).
- **Smallest change.** A scope or endpoint statement that opts out of the client's policy
  (`send none;`, or naming another policy value), so the token endpoint stays in the same
  declaration and `with_base_url` moves both together.

## 2. `Retry::receive` cannot await a credential per attempt

- **Evidence.** `concord_retry::Retry::receive`'s hook is `FnMut(&mut reqwest::Request, usize)`:
  it can read `TokenCache::get` but not await `get_or_refresh` (a token that expired during
  backoff, or the first acquisition). `src/policy.rs` therefore uses `Retry::run` and re-creates
  the adapter's per-attempt template handling: `try_clone`, the one-shot `template.take()`
  fallback, the replay preflight (`template.try_clone().is_some()`), and `decoder.execute`.
  Each outcome is wrapped in an SDK enum `Attempt<T> { Token(TokenError), Sent(Result<..>,
  Generation) }` so the classifier knows which token generation a `401` must invalidate.
- **By hand.** About 30 lines (the factory closure and `Attempt`). The concord-auth README
  describes exactly this route ("use `Retry::run`'s async attempt factory").
- **Provider / generic.** Generic: any SDK whose credential is acquired asynchronously.
- **Smallest change.** An async variant of the adapter, e.g. `Retry::receive_with(prepared, mode,
  hook, classify, ..)` whose hook returns a future yielding `Result<A, X>` (here: the token
  generation, or the acquisition error), with `A` handed to the classifier beside the `Received`.

## 3. The retry report cannot say what happened before the last attempt

- **Evidence.** `Run` keeps the last completed outcome and `Report::attempts` counts started
  factories. A write must stay uncertain if *any* earlier attempt was ambiguous, and an attempt
  cancelled by the deadline after it may have been sent must be noticed. `src/policy.rs` keeps
  classifier state `Observed { completed, ambiguous, renewed }` and infers a cancelled attempt
  from `report.attempts > seen.completed` (test `a_deadline_that_cancels_an_attempt_is_reported`).
- **By hand.** About 15 lines.
- **Provider / generic.** "Which outcomes are ambiguous" is the SDK's (provider). Knowing that an
  attempt was cancelled in flight is generic.
- **Smallest change.** `Report::completed` (outcomes classified), or a flag on
  `StopReason::Deadline` saying an attempt was in flight.

## 4. Which endpoints may be repeated is not part of the declaration

- **Evidence.** Only Create order and Capture payment for order document `PayPal-Request-Id`.
  `accepts_request_id` in `src/policy.rs` compares `E::operation()` with
  `<PayPalOrdersCreate as Endpoint<Order>>::operation()` and the capture's — type-checked, but a
  list kept apart from the declaration (a new endpoint with the header is never repeated until
  added: it fails safe). The policy also recovers the typed `RequestId` by reading the native
  header back from the prepared request and validating it again.
- **By hand.** About 15 lines.
- **Provider / generic.** The set of endpoints is provider. Attaching an SDK fact to an endpoint,
  and reading a declared value back in the policy, is generic (Zendesk's policy compares
  an operation name string for its own per-endpoint rule).
- **Smallest change.** A static, SDK-defined marker on an endpoint that `Operation` exposes (for
  example `Operation::marks() -> &'static [&'static str]` from a declaration statement), so the
  rule lives where the endpoint is declared. Attributes are a closed set today (AUTHOR_GUIDE §8).

## 5. The bearer header is invisible to the declaration

- **Evidence.** The token is inserted natively after `prepare()?.into_parts()`
  (`request.headers_mut().insert(AUTHORIZATION, ..)`). `prepare()` therefore returns an
  unauthenticated request (documented in AUTHOR_GUIDE §5.13; test
  `prepare_is_undecorated_and_sends_nothing`), and `PayPal::validate_header_names()` cannot see
  the header: a later declaration of `authorization` would be replaced silently by the insert.
- **Provider / generic.** Generic.
- **Smallest change.** A declared header slot whose value the send policy supplies, so header
  composition and `validate_header_names()` know it exists.

## 6. The generated `with_base_url` can move a client away from its credentials

- **Evidence.** The session's token endpoint and credentials are bound to one origin, but the
  generated `PayPal::with_base_url` moves only the API requests, and a policy value cannot observe
  it. The policy compares each request's origin with the session's and refuses
  (`Error::Destination`, test `a_client_moved_to_another_origin_sends_nothing`): otherwise a
  bearer token for the sandbox would be sent to whatever host the client was moved to.
- **By hand.** 3 lines, an error variant and a test.
- **Provider / generic.** Generic (any policy holding host-bound credentials).
- **Smallest change.** Pass the client's base to `SendWith::send`, or let an SDK hide the
  generated `with_base_url` of a client whose policy owns the destination.

## 7. Generated construction surface the SDK's callers cannot use

- **Evidence.** The session is a required client value (`send session;` requires one), so rustdoc
  of `PayPal` shows `new(http, PayPalConfig { session })`, `PayPalConfig` and `set_session`, while
  `Session` has no public constructor (callers use `PayPal::sandbox` / `PayPal::builder`).
- **Provider / generic.** Generic (AUTHOR_GUIDE §5.13 notes the same for Dropbox's config).
- **Smallest change.** A client value constructed only by SDK code (a visibility on client values,
  or a `send` value left out of the generated config when the SDK supplies a constructor).

## 8. A constant `{}` body is a caller argument

- **Evidence.** The capture specification's request examples are all `{}`. A body cannot have an
  initializer (AUTHOR_GUIDE §5.4), so callers write `capture(CaptureRequest::new())`. The
  alternative — no body and a declared `content-type` — sends `Content-Length: 0` labelled JSON,
  which is not what the examples show.
- **Provider / generic.** Generic, minor.
- **Smallest change.** Allow `body b: T = expr` like other values.

## 9. `concord-auth`'s token cache against "client credentials, no refresh token"

Used: `TokenCache<(), TokenError>` with capacity 1 (the session instance fixes origin and
credentials, the README's instance-context pattern), `get_or_refresh` with an owned
client-credentials acquisition, `invalidate` with the generation each request used,
`with_clock` in tests, `concord_auth::basic` and `bearer`.

It expressed: single flight (8 concurrent callers, one token request:
`concurrent_calls_fetch_one_token`; one shared failure: `concurrent_callers_share_one_failed_token_request`),
reuse until `expires_in` minus the margin and a new grant afterwards
(`an_expired_token_is_obtained_again_with_client_credentials`), a failure that is not cached
(`a_refused_token_request_sends_no_api_request_and_is_not_cached`), and a `401` that discards only
the token it used. "No refresh token" needs nothing from the cache: the acquisition closure is the
grant.

It could not express:

- **An owned provider error for each waiter.** Waiters receive `RefreshError::Acquire(Arc<E>)`;
  returning the SDK's error by value needs `E: Clone`, so `TokenError::Failed` keeps only the
  `ErrorKind` of a failed token request, not the `concord::Error` (its transport source, its
  unread response). Inherent to sharing one failure; accepted.
- **A minimum remaining validity.** A token whose lifetime is not longer than the margin is
  handed to current waiters and never reused, so every later call would request a token again.
  The SDK refuses such documents during acquisition (`TokenError::Unusable`), the route the
  README suggests. Smallest change: a minimum-lifetime option on the cache.
- **Token metadata.** `Token` holds the header and lifetime only; PayPal's `scope` and `app_id`
  cannot travel with the cached credential (not needed by this slice).

## 10. Shared crates not used

- **`concord-ratelimit`**: it admits requests against a local fixed window; PayPal publishes no
  quota and limits traffic it considers abusive until satisfied (rate-limiting page), so any
  window would be invented. A `429` is returned to the caller with any `Retry-After`. Not a
  defect of the crate.
- **`concord-pagination`**: the slice has no listing.
- **`concord-retry`'s `concord` feature** (`Retry::receive`): not usable, see item 2; the engine
  (`Retry::run`, `Policy`, `Decision`, `Replay`) and `http::retry_after` are used.

What needed no workaround: `Error::delivery()` (a timed-out capture is `Unknown`), bounded error
documents in two formats with the status kept (`received.document::<Json, D>(limit)`), `limit`
on every read path, `concord::Form` for the token body, `Endpoint::operation()` equality, and
strict `Json` media checks.

## Examples consulted

The result is example-assisted. Files read, and what was taken:

| File | Taken |
|---|---|
| `examples/zendesk-support-sdk/Cargo.toml` | dependency layout (path dependencies, testserver dev-dependency, workspace lints) |
| `examples/zendesk-support-sdk/src/lib.rs` | `send policy -> Result<_, Error>` with a client-value policy; a constructor on the generated type; `http_client()` without retries or redirects; README doctest inclusion |
| `examples/zendesk-support-sdk/src/policy.rs` | the read/write split in a send policy, the write classification (no response, 5xx, unreadable 2xx are uncertain), `Decision`-based classifier; it uses `Retry::receive`, which this SDK could not (item 2) |
| `examples/zendesk-support-sdk/src/error.rs` (first 120 lines) | `HttpError` read with `received.document`, redacted `Debug` with `*_unredacted` accessors, `Retry-After` through `concord_retry::http` |
| `examples/zendesk-support-sdk/tests/workflows.rs` (first 80 lines), `tests/support/mod.rs` (first 80 lines) | fixture helpers (`fixture`, `json_fixture`), a test transport, a stateful fake service |
| `examples/zendesk-support-sdk/PROVENANCE.md` (first 60 lines) | layout of the documentation, rules and fixture tables |
| `examples/hubspot-crm-sdk/Cargo.toml`, `src/lib.rs` | a separate OAuth declaration (`request Form;`, `response Json;`); `TokenCache` with capacity 1 and a plain-data acquisition error; `concord_auth::bearer` |
| `examples/testserver/src/lib.rs` | the fixture server API (`Reply`, `Body::FailAfter`, delays, `Recorded`) |

Shared crates read (not examples): the READMEs of `concord-auth`, `concord-retry`,
`concord-ratelimit`, `concord-codecs`, `concord-pagination` (opening section); the sources of
`concord-auth` (`cache.rs`, `credentials.rs`) and `concord-retry` (`lib.rs`, `concord.rs`); and in
`concord-core`, `prepared.rs` (`Endpoint`, `SendWith`, `Decoder`), `error.rs` and `response.rs`
(`Operation`, `Received::document`). Guides read: `README.md`, `docs/QUICKSTART.md`,
`docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`, `docs/RECIPES.md`. The worktree has no
`AGENTS.md` or `CLAUDE.md`.
