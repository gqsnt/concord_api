# Friction

What building this SDK on Concord cost, with evidence from this package. Line numbers refer to
the files as delivered. "Provider" marks a Spotify rule that belongs in the SDK; "generic" marks
mechanics every SDK of the same kind would rewrite.

Example-assisted: `examples/xero-accounting-sdk/{Cargo.toml,src/auth.rs,src/policy.rs}` and
`examples/testserver/src/lib.rs` were consulted (details in `PROVENANCE.md`).

## What worked without friction

- **Declaration.** Five operations under `client → scope → endpoint` (`WebApi`, `me`,
  `playlist(id)`), `format Json`, `limit BODY_LIMIT`, `path "v1"` so `with_base_url` can point at
  `127.0.0.1` (src/lib.rs 69–117). The search's fixed `type=artist` is a `query { }` literal.
- **One page group, one adapter.** `params PageParams(query limit?: u32, query offset?: u32)` is
  included by three listings; its generated trait plus `concord::Sends<Result<R, Error>>` let one
  `pages(first)` serve all three (src/pages.rs, 88 lines including a `Listing` projection for the
  search envelope).
- **Send policy as a client value.** `send session -> Result<_, Error>;` gives every generated
  `send()` the SDK's result type, named in rustdoc; callers import nothing.
- **Error documents.** `received.document::<Json, ErrorEnvelope>(64 KiB)` kept the status
  authoritative for an HTML 502, an empty 503 and a JSON 403 alike (tests/protocol.rs
  `every_documented_status_class_is_mapped_with_its_document`).
- **Formats.** `concord` feature `form` encodes the token request (`skip_serializing_if` honored);
  strict `Json` turned an unlabelled or `text/html` 2xx into `ErrorKind::Metadata`.
- **concord-auth's single flight** held under a held token response (tests/workflows.rs
  `concurrent_requests_share_one_refresh`; mutation M2 below makes it fail), and generation-based
  invalidation kept two concurrent 401s on the same token to one refresh
  (`concurrent_401s_on_the_same_token_share_one_refresh`; mutation M10).

## Costs

### 1. concord-auth: the refresh token is not part of the cached credential (generic + provider)

- **Evidence.** `TokenCache` caches `Token { header, lifetime }` only
  (`crates/concord-auth/src/cache.rs` 20–33). The refresh token, the provider's rule that a
  missing (or empty) replacement keeps it, the `invalid_grant` memory and the granted scope live
  in SDK state behind a `Mutex`, owned by the acquisition closure: `RefreshState`, `GrantState`,
  `Acquirer` (src/session.rs 116–192, about 75 lines).
- **What it could not express.** "Credential state that the acquisition reads and updates, whose
  rotation the application must see to persist it." The cache is right not to know the rule;
  but every OAuth-refresh SDK writes this same holder (Xero's `FileStore` ticket is the same
  shape with a journal).
- **Provider vs generic.** Keep-on-absent is Spotify's documented rule (RFC 6749 §6 lets a server
  omit the replacement). The holder, its lock discipline and "only rotate if the token sent is
  still the stored one" are generic.
- **Smallest change.** A recipe in the concord-auth README ("refresh state beside the cache"),
  not a new type: the rule differs per provider.

### 2. concord-auth: no way to seed the cache with a token already held (generic)

- **Evidence.** The code exchange returns a usable access token, but the cache can only be filled
  by its own acquisition. The SDK stores it as a `seed` with an absolute expiry, takes it on the
  first acquisition and computes the remaining lifetime itself (src/session.rs 126–127, 147–152,
  231–241).
- **Smallest change.** `TokenCache::insert(key, Token) -> Snapshot`, measuring the lifetime from
  insertion with the same checked arithmetic (about 15 lines in concord-auth). It would also
  remove item 3.

### 3. concord-auth: the injected clock cannot be read back (generic)

- **Evidence.** The seed's remaining lifetime must be computed on the cache's clock, but
  `TokenCache`'s clock is private; the SDK keeps its own `Arc<dyn Fn() -> Instant>` and passes a
  clone to `with_clock` (src/session.rs 231, 243–250). Without that, a test clock would move the
  cache but not the seed.
- **Smallest change.** Item 2, or a `TokenCache::now()` accessor.

### 4. concord-auth: an acquisition error is only ever shared (`Arc<E>`) (generic, minor)

- **Evidence.** `RefreshError::Acquire(Arc<E>)` forces the SDK's variant to be
  `Error::Auth(Arc<AuthError>)`; accessors (`Error::auth`, `Error::concord`) hide the `Arc`.
  A caller that was the only waiter still cannot own the error.
- **Smallest change.** None needed; an `Arc::try_unwrap` helper would be cosmetic.

### 5. concord-auth: failures are forgotten by design; the provider forbids retrying `invalid_grant` (provider)

- **Evidence.** A failed flight is removed and the next call runs the closure again. The
  refreshing-tokens page says to discard the refresh token on `invalid_grant` "instead of
  retrying", so the SDK marks it `Rejected` and answers `ReauthorizationRequired` without I/O
  (src/session.rs 180–188; test `invalid_grant_stops_refreshing_until_the_user_authorizes_again`,
  mutation M6).
- **Smallest change.** None: this is provider policy and belongs in the SDK.

### 6. concord-retry: a 429 cannot be classified on its document through `receive`/`receive_with` (generic)

- **Evidence.** A quota 429 (`reason: QUOTA_EXCEEDED`, which waiting does not cure) and a rate
  429 differ only in the body. `Retry::receive` and `receive_with` hand the classifier an unread
  `&Received<T>`, which cannot be read through a reference. The SDK therefore used `Retry::run`
  and wrote what the adapter would have done: the template clone (`request.try_clone()`, with an
  unreachable `None` branch for bodiless GETs), applying the token, `decoder.execute`, plus its own
  `Attempt<T>` enum that reads the 429 document inside the attempt (src/session.rs 319–399, about
  80 lines). `ReplayMode` preflight is not available on `run`.
- **Provider vs generic.** Which reason means "do not retry" is Spotify's; "the decision needs the
  bounded error document" is generic (any provider with a typed error body).
- **Smallest change.** A README recipe naming `run` as the route when classification needs the
  document; or a `receive_with` classifier that may consume the outcome into an SDK value.

### 7. concord-retry: one attempt budget for several causes (provider, minor)

- **Evidence.** `Policy::max_attempts` is one count; the SDK sets it to `2 + n` and keeps
  per-cause counters in the classifier (one 401 replay, at most `n` 429 retries). Mutation M7
  (replaying every 401) survived until a test gave the budget room (`rate_limit_retries(3)` in
  `a_second_401_is_final_after_one_refresh`): the classifier, not the policy, bounds the replay.
- **Smallest change.** None; per-cause rules are the SDK's.

### 8. Pagination: the first page's offset cannot be checked (generic, minor)

- **Evidence.** Generated request types have setters but no getters, so `pages(first)` cannot know
  whether the caller already set `offset`; the "page starts at the requested offset" check runs
  for pages the adapter requested only (src/pages.rs 79–83).
- **Smallest change.** None in Concord; callers who start mid-listing use `pages_from(first,
  Resume::At(offset))`, which is checked.

### 9. The authorization URL is a declared, never-sent endpoint (generic, minor)

- **Evidence.** `Accounts::authorize` (src/auth.rs) is a `GET` declared only so that
  `prepare()?.request().url()` assembles and encodes the browser URL with the same base override as
  the token endpoint. It works, but the endpoint also has a `send()` that would fetch an HTML page,
  so the declaration stays private behind `OAuth::authorize_url`.
- **Smallest change.** A short note in the author guide that a redirect URL is built with
  `prepare()`; no language change.

### 10. rustdoc: two different `Error`s side by side (generic, minor)

- **Evidence.** With `cargo doc --no-deps`, `WebApiArtist::prepare` renders
  `Result<Prepared<Artist>, Error>` with Concord's `Error` unlinked, directly above
  `send() -> Result<Artist, Error>` with the SDK's `Error` linked
  (`target/doc/concord_sdk_spotify_web_api/struct.WebApiArtist.html`).
- **Smallest change.** The generated `prepare` doc text could name `concord::Error` explicitly.

### 11. concord-ratelimit was not adopted (provider)

- **Evidence.** The pages read publish no request rate; quota buckets' "groupings and limits are
  subject to change". A local fixed window would invent a number. The SDK only reacts to 429 and
  `Retry-After`.
- **Smallest change.** None.

## Mutation checks run

Each mutation was applied to a copy-backed source file, the whole test suite was run with
`--no-fail-fast`, and the file was restored. All ten were detected.

| Id | Mutation | Failing tests |
|---|---|---|
| M1 | a refresh without replacement erases the refresh token | 3 (both refresh workflows, the empty-replacement case) |
| M2 | every acquisition removes the in-flight entry first (no single flight) | 2 (`concurrent_requests_share_one_refresh`, `concurrent_401s_on_the_same_token_share_one_refresh`) |
| M3 | a 403 is treated like a 401 | 2 (the 403 workflow, the status table) |
| M4 | `QUOTA_EXCEEDED` is not recognized | 2 |
| M5 | the page offset check is skipped | `pages_that_break_the_paging_contract_are_refused` |
| M6 | `invalid_grant` is not remembered | `invalid_grant_stops_refreshing_until_the_user_authorizes_again` |
| M7 | every 401 is replayed | `a_second_401_is_final_after_one_refresh` (after the test was strengthened) |
| M8 | the grant's access token is never used | 14 |
| M9 | an empty `refresh_token` replaces the stored one | `the_token_document_contract_is_checked` |
| M10 | a 401 removes the cache entry whatever its generation (instead of `invalidate`) | `concurrent_401s_on_the_same_token_share_one_refresh` |
