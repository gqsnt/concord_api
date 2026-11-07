# Friction

What building this SDK cost on top of Concord, with the evidence, what was written by hand,
which part is Datadog's semantics and which is generic mechanics, and the smallest Concord
change that would remove the cost. No shared crate was edited (frozen baseline
`c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`); the changes below are proposals only.

The result is **example-assisted**: the examples consulted are listed at the end.

## What Concord carried without extra code

- **The declaration** (`src/api.rs`, 45 lines inside `api!`) covers both operations:
  the `api/v2/events` prefix kept across a runtime base (`path` on the client, as the author
  guide advises), five optional query values with bracketed keys, a JSON body, the JSON
  response with its `Content-Type` check before the body is read, the `Accept` suggestion, an
  8 MiB `limit` on every success body, and both keys in vendor headers.
- **`#[sensitive]` on `header(DD_API_KEY)`/`header(DD_APPLICATION_KEY)`.** `dd-api-key` is not on
  Concord's well-known list, so the marker is what redacts it; the test
  `keys_are_sensitive_and_never_printed` checks `is_sensitive()` and `Prepared`'s `Debug`, and a
  mutation that removed the marker from the API key was caught by it.
- **`query(..) range?: TimeRange`.** One `QueryPairs` value writes `filter[from]` and
  `filter[to]` together, so "both bounds or neither" needs no wrapper endpoint (a six-line
  `QueryPairs` impl).
- **`send SendPolicy -> Result<_, Error>;`.** Every generated `send()` returns the SDK's
  `Result<EventsPage, Error>`, named in rustdoc; the policy (`api.rs`, 31 lines) uses
  `receive()`, `document::<Json, ErrorDocument>(limit)` and `decode()`. Status, head, operation
  and the redacted Concord error survive every failure class without hand capture.
- **`concord-pagination::Pages`** fits a cursor carried in a POST body: the factory clones the
  body and sets `page.cursor`; caps, `Bounded` vs exhaustion, resume points, `collect_complete`
  with the prefix all come from the crate (`search_pages_from`, 16 lines).

## Costs

### 1. A site-specific host whose parent domain varies is a runtime base with an `expect`

- **Evidence.** The API hosts are `api.datadoghq.com`, `api.us3.datadoghq.com`,
  `api.datadoghq.eu`, `api.ddog-gov.com`, `api.us2.ddog-gov.com`, ... (`fixtures/sites.doc.json`).
  `subdomain` only prepends labels to the one declared base host, and the parent domain
  differs between sites, so the declaration's `base` is an arbitrary site (US1) and
  `DatadogEvents::new` installs the real one with `with_base_url(site.api_base_url())`, which
  returns `Result`: `src/client.rs` carries `.expect("every listed site's API base is an https URL
  with a domain host")` for nine constant inputs (made safe by `sites_match_the_documented_table`,
  which prepares a request for every site).
- **By hand.** `Site` (`src/site.rs`, 150 lines: the table, `DD_SITE` parsing, host lookup) —
  provider data that belongs in the SDK — plus the `expect` and the arbitrary declared base,
  which are generic mechanics.
- **Smallest Concord change.** A host template fed by a client value (`host "api.{site}";`, the
  value's `Display` giving `datadoghq.eu`), validated per request like `subdomain` labels; or a
  `BaseUrl` type with a `const` checked constructor that `with_base_url` accepts infallibly.

### 2. Cursor guards are hand-written in every cursor SDK

- **Evidence.** Without a cap, a provider page that repeats its `meta.page.after` makes
  `Pages` request forever. `search_pages_from` keeps a `HashSet` of requested cursors and
  `project()` refuses an empty cursor (`src/client.rs`); the Notion SDK's `client.rs` has the same
  `seen` set and empty-cursor check (where this pattern was taken from). Tests:
  `a_repeated_cursor_ends_the_traversal_before_a_loop` (two requests, then `Error::Protocol`, no
  third request) and `an_empty_cursor_ends_the_traversal_as_a_protocol_error`.
- **Provider vs generic.** What an empty cursor means is provider semantics (here: restart).
  Refusing a key the traversal already requested is generic.
- **Smallest change** (in `concord-pagination`, not core): an opt-in
  `Pages::refuse_repeated_keys()` for `K: Eq + Hash` that ends the traversal with a typed
  `PagesError` variant before the factory is called.

### 3. The error-document plumbing is copied between SDKs

- **Evidence.** `src/error.rs` lines 21–126 (`ErrorBodyError` with `Metadata`/`Read`/`Decode`
  from `Received::document`'s error kind, and `ProviderError` holding the head and the
  document with a value-free `Debug`) are adapted almost line for line from
  `examples/frankfurter-sdk/src/error.rs`. None of it is Datadog-specific.
- **Provider part kept in the SDK.** The `{"errors": [..]}` model, the status → variant table
  (401/403 authentication, 400, 429, other), and the choice not to print provider text.
- **Smallest Concord change.** Return a small owned type from `document` — say
  `concord::Answer<D> { meta, document: Result<D, DocumentError> }` with
  `DocumentError::{Metadata, Read, Decode}` and a `Debug` that prints the status, header names
  and the failure stage only — so SDKs keep just the status classification.

### 4. A bad key is a build error at the first request, not at construction

- **Evidence.** Generated `new` is infallible; a key with a line break would surface as
  `ErrorKind::Build` from the first `prepare()` (naming the header, never the value). To make a
  missing or unsendable key an *authentication* error before any request exists (the required
  failure), `Credentials::new` (`src/credentials.rs`) re-runs `http`'s
  `HeaderValue::from_str` rule on both keys and checks emptiness;
  `keys_are_checked_before_any_request_exists` covers it.
- **Provider vs generic.** Emptiness and "this is an authentication failure" are the SDK's rules;
  the header-value check duplicates what Concord does at build time.
- **Smallest Concord change.** A generated `Client::try_new(http, config)` (or
  `Config::check()`) that applies the placement checks of every client value once and returns
  the `Build` error naming the value.

### 5. Two different `Error`s render alike in generated docs

- **Evidence.** In `cargo doc --no-deps` output, `DatadogApiEventsList::prepare` shows
  `Result<Prepared<EventsPage>, Error>` (Concord's, unlinked) and `send` shows
  `Result<EventsPage, Error>` (this crate's, linked). The Frankfurter SDK documents the same
  ambiguity in its client docs; this SDK says it on `DatadogApi` and on `Error`.
- **Smallest change.** The generated `prepare()` documentation could name `concord::Error`
  explicitly when the client has a send policy (the `send()` docs already explain the split).

### 6. The site the policy reports must not be a second source of truth

- **Evidence.** The first version made the site a client value (`policy: SendPolicy { site }`,
  `send policy;`), as the author guide suggests for a stateful policy. That added a required
  `DatadogApiConfig` field that could disagree with the base URL (after `with_base_url`). The
  final policy is stateless (`send SendPolicy`) and reads the answering host from
  `received.url()`, mapping it back to a `Site` when listed (`Rejected::host`, `Rejected::site`).
- **Not a Concord defect**; recorded because the guide's stateful-policy pattern invites the
  duplicate. No change requested.

### 7. Vendor credential headers make the transport the SDK's business

- **Evidence.** reqwest forwards custom headers on cross-origin redirects (author guide §3), so
  the SDK ships `DatadogEvents::http_client()` (no redirects, `retry::never()`) and documents it;
  `a_redirect_is_not_followed_and_the_keys_stay_home` checks that the redirect target receives
  nothing. A caller-supplied client can still follow redirects; Concord cannot see it.
- **Smallest change.** None in core (the transport is the application's). A shared, documented
  `no_redirect` client constructor would remove the copies this pattern has across SDKs (the
  author guide §3 quotes FRED's `http_client()`; this SDK has its own).

### 8. Observations without a requested change

- `filter[from]` and the other bracketed keys go on the wire form-encoded
  (`filter%5Bfrom%5D=...`, asserted in `list_request_on_the_wire`), while the documentation's
  `links.next` spells them raw. Equivalent under URL decoding; whether Datadog decodes them is
  NOT_RUN.
- The documented 200 example is not valid against its own schema (placeholder `"integer"`
  strings). Keeping the excluded event schemas as raw JSON is what lets it decode verbatim.
- Testing the documented hosts on the wire needs `ClientBuilder::resolve` plus
  `with_base_url("http://<host>:<port>")`, as the quick start (§6) shows: the HTTPS base of a site
  cannot reach a plain-HTTP stand-in, so the per-site `https://` URLs are checked through
  `prepare()` (no I/O) instead.

## Shared capabilities not adopted

| Crate | Decision | Why |
|---|---|---|
| `concord-pagination` | **used** (`Pages`, `Resume`, `PagesError`, `Incomplete`) | cursor traversal with caps and resume points |
| `concord-retry` | not used | the cited pages document 429 but no retry hint or rate-limit header, and the slice asks for no retry; inventing a schedule would be a policy without evidence. A 429 returns `Error::RateLimited` with the head, so a caller can wrap a factory in `Retry::run` (search is a read, replay-safe by meaning). |
| `concord-ratelimit` | not used | no documented quota or bucket to admit against |
| `concord-auth` | not used | its constructors build `Authorization` schemes (`Basic`, `Bearer`, `scheme token`) and its cache serves acquired tokens with lifetimes; Datadog's keys are static bare values in two vendor headers, which `#[sensitive] header(..)` already sends and redacts |
| `concord-codecs` features | only `json` (through `concord`) | both bodies and every response are JSON |

No native escape (`into_parts`, `send_raw`) was needed.

## Examples consulted

| File | Taken from it |
|---|---|
| `examples/frankfurter-sdk/Cargo.toml` | dependency layout (`concord` path with `json`, testserver dev-dependency, workspace lints) |
| `examples/frankfurter-sdk/src/lib.rs` | send-policy shape: `receive()`, `document::<Json, D>(LIMIT)` on non-2xx, decode on 2xx |
| `examples/frankfurter-sdk/src/error.rs` | `ErrorBodyError` and its `From<concord::Error>` by kind; `Error::Concord { source, meta: head_meta() }`; value-free `ProviderError` `Debug` (adapted almost verbatim, see cost 3) |
| `examples/notion-sdk/Cargo.toml` | the `concord-pagination` dependency |
| `examples/notion-sdk/src/api.rs` | a POST endpoint whose cursor lives in the JSON body, `limit` on the client |
| `examples/notion-sdk/src/client.rs` | `Pages::new(from, factory)` with a requested-cursor set, the empty-cursor refusal, a `*_from(Resume)` entry point (cost 2) |
| `examples/notion-sdk/tests/protocol.rs` (lines 1–80) | test transport with `no_proxy()` and no redirects, fixtures by `include_*!`, `Script` for page sequences |
| `examples/testserver/src/lib.rs` | the stand-in's API (`Recorded`, `Reply`, `Body::FailAfter`, `Script`, `resolve`-friendly `host`) — test support, not an SDK |

Concord documentation read: `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md`, `docs/CALLER_GUIDE.md` §6–8, the READMEs of
`concord-pagination`, `concord-auth`, `concord-retry`, `concord-ratelimit` and
`concord-codecs`, and the signatures of `SendWith`, `QueryPairs`, `Received::document`,
`ResponseMeta`, `Operation` and `Pages` in the crate sources.
