# Fresh consumer: SDK50 REST Countries

Date: 2026-11-07. Live provider calls: **NOT_RUN**. All SDK requests will use
`TestServer::start(|request: &Recorded| -> Reply)` on loopback with an invented fixture
credential. Fixtures are authored samples, not downloaded country datasets.

## Reading boundary and observable preparation

Before writing application code, read only the SDK README and generated public rustdoc,
the root/SDK/testserver Cargo manifests, and the two permitted official provider pages.
No SDK source, pre-existing tests, FRICTION.md or PROVENANCE.md has been read.
File-name discovery listed the latter files without opening them. No AGENTS.md was found
inside the worktree. Nothing in campaign parent directories was read.

Generated SDK rustdoc with:

```sh
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo doc --offline -p concord-sdk-rest-countries --no-deps
```

This first build deliberately omitted `--locked`; it succeeded without changing Cargo.lock.
The package is already a workspace member. Subsequent cargo commands use `--locked`,
`CARGO_BUILD_JOBS=4`, the default worktree target directory, and run one at a time.
Also generated public rustdoc for `concord-example-testserver`, `concord-pagination` and
`concord` with `--offline --locked --no-deps` to discover fixture construction, partial-result
handling and response-head access. No dependency or manifest changes are planned.

Public API pages read before code: SDK crate index, RestCountries, RestCountriesSearchNames,
CountryPage, PageMeta, pages, ListOptions, Error, ProviderError, Premium and Country;
testserver TestServer, Recorded and Reply; Concord ResponseMeta and Error; pagination
Pages, Items, ItemsReport, ItemsProgress and PagesError. Rustdoc Source links were not opened.

## Provider research and current access conditions

Read on **2026-11-07**:

- [Countries API](https://restcountries.com/docs/countries): v5 GET endpoints, field allowlists,
  bearer authentication, free-plan page ceiling 100 (paid 500), default 25, premium-field
  notices, frozen-account 403s and rate limiting above 20 requests per 10 seconds.
- [API versions](https://restcountries.com/docs/countries/api-versions): v5 is maintained,
  uses `api.restcountries.com` and requires a key; v1–v4 are deprecated.

These are the access conditions verifiable within the two-page research boundary.
Separate linked plans, acceptable-use and legal Terms of Service pages are outside that
boundary and were not read; full legal access terms therefore remain unverified. No real
service is used, including with the provider's demo key. This capsule makes no claim of
v3 compatibility or permission for future live use.

## Predictions

These predictions were written before creating `tests/first_use.rs` or sending an SDK request.
They are based on the public SDK docs, including the rustdoc generated above.

### Workflow 1: search names and fetch a chosen country

- Construct `RestCountries::new(http, RestCountriesConfig { authorization: bearer(key)? })`
  and call `.with_base_url(server.url())?`. Use reqwest with proxies disabled, redirects
  disabled and a finite timeout. The server URL supplies the origin; `/countries/v5` remains.
- Call `api.search_names("germ").set_response_fields(Fields::new([Property::NAMES_COMMON,
  Property::CODES_ALPHA_3])?).send().await?`.
  Wire: `GET /countries/v5/name`, query `q=germ` and
  `response_fields=names.common,codes.alpha_3`. No explicit limit or offset is sent.
  Headers: `Authorization: Bearer <fixture-key>`, `Accept: application/json`, and the
  transport's loopback Host. No request body, API key in query or refresh request.
- Expect `CountryPage` with Germany's common name and alpha-3 code, optional unselected
  fields absent, provider `meta` (including request_id), and optional demo notice retained.
  `next_offset()?` is `None` when `more=false` on a consistent terminal page.
- Parse the chosen code via `CountryCode::parse`, then
  `api.country(&code).send().await?.single()?`.
  Wire: `GET /countries/v5/codes.alpha_3/DEU`, no query, same headers/body rules.
  Expect `Some(Country)` with name, codes and borders from the fixture. Lowercase input
  to `CountryCode::parse` is uppercased. No extra scope or setter trait import is needed.
- An unmatched search (`q=Atlantis`) is `Ok` with no countries, total/count zero and
  `next_offset() == Ok(None)`; `single() == Ok(None)`. Explicit `pages(...).items()
  .collect_complete()` also completes empty after exactly one request. It does not retry,
  refresh credentials or invent another page.
- Empty `q` reaches the provider as a request and may produce 400; 401 remains a typed
  provider status error, with no automatic authentication attempt. Inspect class, messages,
  response headers and operation using the public error API. Provider text must not appear
  in error Debug/Display, even if it echoes the invented credential.
- A JSON-labelled but malformed 200 is `Error::Concord` with `ErrorKind::Decode` and the
  received head retained. It is neither a no-match nor a status failure. A wrong media type
  would instead be `Metadata`; truncation/oversize are documented Transport/Limit failures.
- Invalid bearer syntax or an invalid country code is rejected locally before a request.
  Debug of the client and generated request withholds the credential.

### Workflow 2: filter a documented property and select fields

- Build `Filters::new().with(Property::REGION, "Europe")?
  .with(Property::MEMBERSHIPS_EU, "1")?` and `Fields` containing `NAMES_COMMON`,
  `CODES_ALPHA_2`, `LEADERS` and `BORDERS`. Request a small sample using
  `api.countries().set_filters(filters).set_response_fields(fields).set_limit(1)`
  (the argument is `NonZeroU32`, not a bare integer).
- Wire: `GET /countries/v5`, query `region=Europe`, `memberships.eu=1`,
  `response_fields=names.common,codes.alpha_2,leaders,borders`, `limit=1`.
  No offset for the first request; `.set_offset(1)` or explicit pagination adds `offset=1`.
  Decoded query pairs, rather than raw parameter order/percent-encoding, define the assertion.
  Headers and body match workflow 1. No undocumented v3 routes or parameters are used.
- One `.send()` returns one page only, even with `more=true`. Inspect total/count/limit/
  offset/request_id and demo metadata. An omitted `borders` is `None`; a free-plan notice
  for `leaders` is `Some(Premium::Withheld { message })`, with other fields still useful.
  Neither absence nor the notice triggers a follow-up request or auth handling.
- `pages(listing).max_pages(NonZeroUsize::new(2).unwrap()).items().collect_complete()`
  reads the two-page authored sample (France, Germany) in order, preserving query/headers
  and advancing by offset + count. It returns `Ok` only after the provider's terminal page;
  ending exactly at the cap is complete. An offset beyond total is a terminal empty page.
- A one-page cap while `more=true` is `PagesError::Bounded`, not success: the incomplete
  result retains France and resume offset 1. No second request is made.
- A second-page 429 is `PagesError::Provider` containing `Error::Provider`, preserving the
  first page's France, failed offset 1, counters, Retry-After and request-id headers. It
  stops without retry or auth work. A premium-property filter rejected with 403 remains
  an ordinary provider error; an unsearchable property rejected with 404 is likewise typed.
- Pagination conversion uses `Pages<Country, u64, (), Error>`: provider page metadata is
  inspected with `.send()` before flattening; the high-level walker does not expose it.
- Inconsistent 2xx metadata is a contract error when calling `next_offset()` or walking
  pages; `single()` refuses ambiguous lookups. No secret appears in ordinary SDK debug or
  formatted errors; explicitly accessed provider messages and header values are unredacted.

## First working call

The first caller command was:

```sh
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-rest-countries --test first_use search_names_then_fetch_chosen_country -- --nocapture
```

It compiled and passed on the first attempt (1 test, 0 failures). The first `.send()` was
`search_names("germ")` with the two-field allowlist, answered by a local v5 fixture. Before
any later request, assertions confirmed exactly one request, Germany decoded, terminal page
metadata and the demo marker. The remaining test then fetched DEU and checked no-match,
400, 401, malformed JSON, response-head retention, redaction and local input validation.
It made exactly seven expected requests. No SDK source or existing tests were needed.

The compiler reported one unused `NOTICE` constant intended for the second test; this is
caller scaffolding, not SDK friction. The second workflow will use it.

## Friction incidents (in observed order)

1. **Access-term evidence is incomplete.** The README expressly says plans, quotas and
   Terms of Service were not reviewed. The two permitted official pages supplied operational
   access conditions, but full legal terms remain outside the permitted research boundary.
   Decision: use only loopback fixtures and state that limitation, with no live-use claim.
2. **Following pagination documentation takes another crate's rustdoc.** The SDK's `pages`
   page documents the walker but delegates cap/error/partial-result details to
   `concord_pagination`. Generated that crate's docs and read `Pages` and `PagesError` before
   code. While navigating, also read `Items`, `ItemsReport` and `ItemsProgress`; the concrete
   flattening type is actually `PageItems`, whose docs are being read before workflow 2.
   No implementation lookup was necessary.

No extra setter trait import, generated endpoint type annotation, failed compilation,
surprising endpoint name or misleading diagnostic was needed for the first working call.
The docs explicitly distinguish SDK `Error` from `concord::Error`; `?` worked as documented.

The second workflow also compiled and passed on its first attempt. Public rustdoc pages
`Property`, `PageItems`, `Incomplete` and `Progress` supplied the constants, concrete
collector type, retained prefix and counter names. Adding this test consumed `NOTICE`,
removing the temporary unused-constant warning. No further friction incidents occurred.

## Predictions compared with observations

There were **no observed prediction mismatches**. All observations below are fixture
behaviour; they do not establish how a live account or current provider deployment behaves.

| Prediction | Observation |
|---|---|
| Client base override keeps v5; GET, bearer/JSON Accept, loopback Host, no body or key in URL | Both tests assert every recorded request: 7 in workflow 1, 9 in workflow 2. All matched. The client disables proxies and redirects and sets a 3-second timeout. |
| Name search sends q and the two-field allowlist without default limit/offset parameters | The first request matched the exact decoded query set. Germany decoded with alpha-3, no alpha-2 or borders, request_id, demo notice and terminal metadata. |
| Chosen alpha-3 code drives the property-read endpoint; lowercase code normalizes | `deu` parsed equal to `DEU`; lookup used `/countries/v5/codes.alpha_3/DEU` with no query. `single()` yielded Germany, DE and nine borders including FRA. No endpoint type or setter trait import was necessary. |
| Empty results are successful and terminal | Atlantis returned zero total/count, `next_offset() == None` and `single() == None`. A separate bounded collector completed empty after exactly one request. |
| Empty q is a provider 400; revoked credentials remain 401 without auth work | Both were typed provider errors after one request each. The 400 retained its message, request-id header and operation. Formatting withheld an echoed fixture credential. |
| Malformed JSON-labelled success is Decode with its response head | The malformed 200 produced `Error::Concord` / Decode. The SDK error exposed status 200 and the Concord error's head retained status and x-request-id. |
| Invalid input fails locally; ordinary debug withholds credentials | Invalid country-code and bearer syntax sent no requests. Client/request Debug and SDK error Debug/Display did not contain the fixture key. Explicit message access retained it, as documented. |
| Europe and EU filters plus the four-field allowlist and typed limit form the list query | All list requests retained those query pairs across pagination, with offsets added only when needed. No v3 routes or query authentication appeared. |
| One send returns one page despite more=true; absent/withheld selected fields stay ordinary values | The first page reported total 2/count 1/limit 1/offset 0, more=true, request_id and demo metadata, with next offset 1. France retained name/FR, omitted borders as None, and a Premium::Withheld leaders notice. Exactly one request had occurred. |
| Complete pagination ends at the provider's terminal page, including exactly at the cap | The collector returned France then Germany after two requests at offsets 0 (implicit) and 1. max_pages=2 was complete; no third page was requested. |
| A cap retains a useful prefix and continuation instead of reporting completion | max_pages=1 returned Incomplete with France, PagesError::Bounded, resume At(1), and counters admitted 1/succeeded 1/failed 0. No second request. |
| A page failure retains partial results, status, metadata and continuation | A `q=republic` sample returned France, then 429 at offset 1. Incomplete retained France; PagesError::Provider retained failed At(1), counters 2/1/1, RateLimited, provider message, Retry-After=10, request-id and operation. Exactly two requests, no retry/refresh. |
| Offset beyond total completes empty | Explicit offset 100 returned total 2/count 0/offset 100, more=false and no next offset. |
| Unsupported filter access is an ordinary provider error | Premium leaders filtering returned Forbidden/403 with its message and request-id; population filtering returned NotFound/404 with its message. Each made one GET using the original credential. |
| The high-level walker has unit page metadata; inspect provider meta with send | The public return type confirmed this; the test inspected CountryPage meta before using the item collector. The preview plus collector intentionally fetched page zero twice, as the README example does. |
| Other documented failure classes | Wrong media types, truncated/oversized bodies, contradictory page metadata and ambiguous code lookup were not re-created in these two fresh caller tests. They remain documented predictions for this capsule. The existing suite was subsequently executed (without opening its files); all its corresponding named tests passed. |

The omitted borders field is deliberate fixture projection/missing-data input, not a claim
that France has no land borders. The filter fixture represents a two-record sample, marked
with `_demo`; it does not assert that Europe has only two EU members. Both tests use the
ordinary `.send()` path. Explicit bounded pagination is used only where completion or
partial-result handling requires it; no `prepare()`, raw request editing or auth machinery
was necessary.

## Final validation and scope

Validation commands (one cargo command at a time); rustfmt was applied only to the new
caller file before the full package checks:

```sh
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-rest-countries --test first_use -- --nocapture
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-rest-countries
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo clippy --offline --locked -p concord-sdk-rest-countries --all-targets -- -D warnings
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-rest-countries --no-deps
rustfmt --check --edition 2024 examples/rest-countries-sdk/tests/first_use.rs
```

- Fresh caller tests: **2 passed, 0 failed**. Full package: **24 integration tests and
  3 doctests passed, 0 failed** (no unit tests declared).
- Clippy, rustdoc with warnings denied, and rustfmt check: **passed**.
- No new crates, dependency versions or manifest edits. Cargo.lock did not change during
  this session, so no new-crate provenance justification is needed.
- Compared root manifest/lock structurally against frozen baseline
  `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: the only member addition is
  `examples/rest-countries-sdk`; the only lock package addition is
  `concord-sdk-rest-countries` 0.0.0. No removed or changed existing lock packages, no other
  root manifest changes, and no tracked changes outside the permitted paths.
- This session added only `FIRST_USE.md` and `tests/first_use.rs`. SDK source, existing
  tests, README, FRICTION.md, PROVENANCE.md and every shared crate remain untouched.
  Existing test sources, SDK sources, FRICTION.md and PROVENANCE.md were never opened,
  even after the first working call. No commits were attempted.
- Documentation suggestion, recorded without changing shared crates: put an SDK-oriented
  `Incomplete { collected, error }` recovery example near the pagination entry point so
  callers can find cap and provider-failure recovery with fewer cross-crate lookups. No
  shared implementation change was needed or is proposed.

## Assistance label

**docs-only** throughout. Evidence came from the original public README, generated public
rustdoc, the two dated official provider pages, build/test diagnostics and recorded local
requests. No source-assisted exception was taken. Live provider calls: **NOT_RUN**.
