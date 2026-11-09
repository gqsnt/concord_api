# FIRST_USE — fresh consumer of `concord-sdk-digitalocean` (SDK41)

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Role: fresh consumer (did not write the SDK). Date: 2026-11-07.
Live provider calls: **NOT_RUN**. Every call below runs against `concord-example-testserver`
on loopback (`127.0.0.1`).

## Inputs read before writing any code

Observable reading log, in order:

1. `examples/digitalocean-sdk/README.md` (full).
2. `examples/digitalocean-sdk/Cargo.toml` (package manifest only: dependency/dev-dependency names).
3. Directory listings of `examples/digitalocean-sdk/` (`src`, `tests`, `fixtures` names only — no
   file contents opened).
4. Rustdoc generated with
   `cargo doc --locked --offline -p concord-sdk-digitalocean --no-deps`, read as text (crate
   index; `DigitalOcean`; the four endpoint structs and two scope structs; `Response`,
   `ResponseMeta`, `Error`, `ProviderError`, `Problem`, `HttpClass`; every model; `PageSize`,
   `PageNumber`, `Paging`, `Policy`, `DigitalOceanConfig`, `Traversal`, constants).
5. Because the SDK docs (built `--no-deps`) leave `Pages`, `Incomplete`, `PagesError`,
   `concord::Error`, `ErrorKind` and `FieldError` unlinked, I also generated
   `cargo doc --locked --offline -p concord-pagination -p concord -p concord-example-testserver --no-deps`
   and read: `concord_pagination` crate index, `Pages`, `PageItems`, `Page`, `PagesError`,
   `Incomplete`, `Resume`, `PageItemEvents`, `PageItemEvent`; `concord::Error`, `ErrorKind`,
   `FieldError`, `Operation`, `Prepared`; the testserver crate index, `TestServer`, `Recorded`,
   `Reply`.
6. Official documentation, read 2026-11-07 through the WebFetch tool (which converts the page to
   markdown and answers through a small summarising model — so the statements below are relayed,
   not raw page text):
   - https://docs.digitalocean.com/products/droplets/reference/api/droplets/ — read in four
     offset windows (the page text is ~317k characters). Observed: `GET /v2/droplets`
     (scope `droplet:read`), `page` (>= 1, default 1), `per_page` (1–200, default 20), `links`
     optional with `pages.{first,prev,next,last}`, single-page examples show `"pages": {}`,
     `GET /v2/droplets/{droplet_id}` returning `{"droplet": {...}}`, 404 example
     `{"id":"not_found","message":"The resource you requested could not be found."}`, the
     droplet's required-field list, rate-limit headers. The droplet *actions* endpoints are
     **not** on this page (they are only in the site navigation as "Droplet Actions").
   - https://docs.digitalocean.com/reference/api/reference/public-apis/ — observed: curl
     examples send `Authorization: Bearer $DIGITALOCEAN_TOKEN`; 20 or fewer objects means no
     `links` object; keys that do not apply in `pages` are omitted; `meta.total`; rate-limit
     headers (`ratelimit-reset` is "the time when the oldest request will expire"),
     `retry-after` on burst 429; error body `id`, `message`, optional `request_id`. No
     action-object schema; nothing on redirects.

   So the action model (`status`, `type`, `started_at`, `completed_at`, ...) cannot be
   checked against the two permitted pages; my action fixtures are synthetic, shaped after the
   SDK's `Action` model and the general DigitalOcean envelope conventions.

## Predictions

Written before any test code. "Token" below is the synthetic fixture token
`dop_v1_first_use_synthetic_token`.

### Common to every call

- Construction: `DigitalOcean::for_fixture(token, &server.url())` performs no I/O; it returns
  `Err(Error::Credential)` for a token with a control character or a newline, and
  `Err(Error::FixtureOrigin)` for `http://localhost:<port>`, an `https://` origin, an origin
  with a path, or a non-loopback IP — all before any request.
- Wire headers on every request: `authorization: Bearer <token>` (I predict the constructor
  adds the `Bearer ` prefix — the docs say "Bearer token" but `set_authorization` takes a
  whole header value, so this is the first uncertain point); `accept: application/json`
  (predicted from "its `Decode::ACCEPT`, if any, is sent" for the `Json` format); `host:
  127.0.0.1:<port>`; no `referer`, no proxy headers, no request body, no `content-type`.
- Success: `Ok(Response { value, meta })`; `meta.status() == 200`;
  `meta.operation().unwrap().name()` is the declared name (`DigitalOcean.droplet.get`, ...);
  `meta.limit()/remaining()/reset_epoch_seconds()` are `Ok(Some(n))` when the fixture sends
  `ratelimit-*` headers, `Ok(None)` when absent, `Err(FieldError::Invalid)` for a non-number,
  `Err(FieldError::Repeated)` for a repeated header.
- Status errors: any non-2xx → `Err(Error::Provider(box))` with `box.meta.status()` the
  status, `box.class()` the `HttpClass` (401 → `Unauthorized`, 404 → `NotFound`, 429 →
  `RateLimited`, 5xx → `Server`, 3xx → `Other`), and `box.document == Ok(Problem { id,
  message, request_id })` for a JSON error document. For an HTML/empty/malformed error body:
  `document == Err(_)` but the class/status still hold. Exactly one request is sent (no
  retry on 401/429/5xx). `format!("{err}")` / `{err:?}` do not contain the provider message
  nor the token.
- Malformed 2xx body (truncated JSON, wrong shape such as `{"droplets": 3}`) →
  `Err(Error::Concord(e))` with `e.kind() == ErrorKind::Decode`. A 2xx with
  `content-type: text/html` → `ErrorKind::Metadata`.

### Workflow 1 — list droplets with `links.pages`, then retrieve one

Calls:
1. `api.droplet_pages(PageSize::new(2)?, NonZeroUsize::new(5).unwrap()).items().collect_complete().await`
2. `api.droplet(selected.id).get().send().await`

Wire:
- Request 1: `GET /v2/droplets?per_page=2` (I predict `page` is omitted on the first page,
  since the setters start absent and `droplet_pages` only takes a size).
- Response 1 (fixture): 200, `{"droplets":[A,B],"links":{"pages":{"next":"http://127.0.0.1:<port>/v2/droplets?page=2&per_page=2","last":"...page=2..."}},"meta":{"total":3}}`.
- Request 2: the next link verbatim — `GET /v2/droplets?page=2&per_page=2`, same
  `authorization`.
- Response 2: `{"droplets":[C],"links":{"pages":{"first":"...","prev":"..."}},"meta":{"total":3}}`
  — no `next` → completion; `collect_complete` returns `Ok(vec![A, B, C])` after exactly 2
  requests.
- Request 3: `GET /v2/droplets/<C.id>` → `Ok(Response<DropletDocument>)`,
  `value.droplet.id == C.id`, nested `region.slug`, `networks.v4[0].ip_address` decoded.

Failures:
- Single-page listing with no `links` at all (and also `"links":{}` / `"pages":{}` /
  `"next":null`) → completion after one request.
- Empty intermediate page with `next` → traversal continues (not treated as the end).
- `meta.total` larger than what was returned, with no `next` → still completion (total
  never invents a page).
- Cap: `max_pages = 1` while page 1 has `next` → `Err(Incomplete { collected: [A, B],
  error: PagesError::Bounded { .. } })`; exactly one request.
- Next link naming another host (a second loopback server on another port): →
  `Err(Incomplete { collected: [A, B], error: PagesError::Provider { error:
  Error::UnsafeNext, .. } })`; the second server receives **zero** requests, so the bearer
  token never leaves the trusted origin. Same for a same-origin link to another collection
  (`/v2/images?page=2`), a link with user info, an `https://` link to the same host:port, and
  an empty-string next.
- 302 redirect from page 2 to the untrusted server → `PagesError::Provider` wrapping
  `Error::Provider` with class `Other` (status 302); untrusted server receives nothing.
- 429 on page 2 with `retry-after: 30` → `Incomplete { collected: [A, B], error: Provider {
  error: Error::Provider(pe) } }`, `pe.class() == RateLimited`,
  `pe.meta.retry_after_seconds() == Ok(Some(30))`; 2 requests total (no replay).
- Get with 404 `{"id":"not_found",...}` → `Error::Provider`, `NotFound`, `Problem.id ==
  "not_found"`.
- Secrets: the fixture never sees a token other than the one supplied; `{api:?}` and error
  formatting do not contain the token.

### Workflow 2 — list the selected droplet's actions, then retrieve one

Calls:
1. `api.action_pages(droplet_id, PageSize::new(2)?, cap).items().collect_complete().await`
2. `api.droplet(droplet_id).action(action_id).send().await`

Wire:
- Request 1: `GET /v2/droplets/<id>/actions?per_page=2`.
- Response 1: `{"actions":[a1,a2],"links":{"pages":{"next":"http://127.0.0.1:<port>/v2/droplets/<id>/actions?page=2&per_page=2"}},"meta":{"total":3}}`.
- Request 2: `GET /v2/droplets/<id>/actions?page=2&per_page=2`.
- Response 2: `{"actions":[a3],"links":{"pages":{"prev":"..."}},"meta":{"total":3}}` →
  completion, `Ok(vec![a1, a2, a3])`.
- Request 3: `GET /v2/droplets/<id>/actions/<a1.id>` → `Ok(Response<ActionDocument>)`,
  `value.action.kind` holds the JSON `type` (e.g. `"reboot"`), `completed_at == None` for an
  `in-progress` action.

Failures:
- Next link pointing at another droplet's actions (`/v2/droplets/999/actions?page=2`) on the
  same origin → `Error::UnsafeNext` inside `PagesError::Provider`, prefix kept, no request
  for the other droplet.
- A 500 with an HTML body on page 2 → `PagesError::Provider` wrapping `Error::Provider`,
  `class() == Server`, `document == Err(_)`; `collected` keeps page 1.
- Truncated JSON on page 1 → `PagesError::Provider` wrapping `Error::Concord` with
  `ErrorKind::Decode`, `collected` empty.
- Action retrieval 404 → `NotFound`.
- Calling the traversal again after an error yields `PagesError::Ended`, never `Ok(None)`.

### Expected friction (predicted)

- Pagination helpers live on the client (`api.action_pages(id, ..)`) and not on the scope
  (`api.droplet(id)...`), so the droplet id is passed twice in workflow 2.
- Traversal items lose the per-page `ResponseMeta` and `meta.total` unless I switch to
  `next_page()` / `item_events()`.
- `droplet_pages` requires a `PageSize`; there is no "provider default" traversal.

---

Everything below was written after running code. Toolchain: rustc 1.99.1, cargo 1.99.1.
Builds: `CARGO_TARGET_DIR=<worktree>/target` (absolute path), `CARGO_BUILD_JOBS=4`, one cargo
command at a time, `--locked --offline`. `Cargo.lock` and the root `Cargo.toml` were not
changed: the package and its workspace entry were already there in the author's commit, so
`--locked` worked on the first build.

## First working call

`tests/first_use.rs`, first version: one `#[tokio::test]` starting
`TestServer::start(|_| Reply::json(200, r#"{"droplets":[],"meta":{"total":0}}"#))`, then
`DigitalOcean::for_fixture(token, &server.url())` and
`api.droplet_pages(PageSize::new(2)?, NonZeroUsize::new(5)?).items().collect_complete().await`.
It compiled and passed on the first attempt. What it took: the README snippet plus the
testserver rustdoc (for `TestServer::start`, `Reply::json`, `server.single()`). Imports:
`DigitalOcean`, `PageSize`, `NonZeroUsize`, plus the testserver types. The wire record it
printed:

```text
GET /v2/droplets?per_page=2
authorization: Bearer dop_v1_first_use_synthetic_token
accept: application/json
host: 127.0.0.1:<port>
```

This settled my two open points at once: the constructor adds the `Bearer ` prefix, and the
`Json` format sends `accept: application/json`.

## The application (`tests/first_use.rs`, 19 tests, all passing)

- Caller task 1, `inspect_droplet(api, name)`: walks every droplet page, selects a droplet by
  name, retrieves it and reports status, region, public IPv4 and remaining quota.
- Caller task 2, `inspect_running_action(api, droplet_id)`: walks the droplet's action pages,
  selects the first `in-progress` action and retrieves it.
- An application error type, `AppError`, keeps the `Incomplete` prefix of either listing.
- The tests assert on the wire (method, raw target, `authorization`, `accept`, no `referer`, no
  body, the host), on values, on `ResponseMeta` (status, operation name, strict
  `ratelimit-*`/`retry-after` parsing), on completion (`Ok(None)` stays `Ok(None)`, request
  counts), and on partial results (`Incomplete.collected`, `PagesError::{Provider, Bounded,
  Ended}`, the resume position).
- Credential containment: a second `TestServer` stands in for the untrusted host. A
  next link to it, and a 302 redirect to it, both leave its request log empty. A control call
  made directly to that server shows that it does record requests, and that it would have seen
  the bearer token. That makes the empty log real evidence.

Results: `cargo test --locked --offline -p concord-sdk-digitalocean` gives first_use 19/19,
plus the author's protocol 11/11, workflows 6/6 and 1 doctest, all passing.
`cargo clippy --locked --offline -p concord-sdk-digitalocean --all-targets -- -D warnings`
is clean. `rustfmt --check` passes on `tests/first_use.rs`.

## Friction incidents, in order

1. **Unlinked cross-crate types in the SDK rustdoc.** I built it as instructed
   (`--no-deps`). `Error::Concord(Error)` and `ProviderError.document: Result<Problem,
   Error>` show a bare `Error` without a link, which reads like the SDK's own `Error`.
   `Traversal<T> = Pages<T, NextLink, ResponseMeta, Error>`, `FieldError`,
   `Incomplete` and `PagesError` are not linked either. I had to generate and read the
   `concord`, `concord-pagination` and testserver rustdoc to learn that the inner error is
   `concord::Error` and how `Incomplete` and `PagesError` behave. That took four pages of extra
   reading before writing any error handling. No source was opened.
2. **Is the `Bearer ` prefix added?** The README says "bearer token". The rustdoc of `connect`
   says only "Construct for the API host". The generated `set_authorization` and
   `DigitalOceanConfig.authorization: HeaderValue` look like they take a whole header value.
   Only the first wire record answered it (the prefix is added).
3. **Generated endpoint docs are long, and the useful part is hard to find.** Each endpoint
   page repeats the format statement, `SendWith`/`Policy` wording, and
   `for<'__concord> u64: Clone` bounds with an explanation of the unused lifetime. The route,
   the query names and the sensitive `AUTHORIZATION` header are there, but buried.
   "its `Decode::ACCEPT`, if any, is sent" did not say whether an `accept` header goes out.
   I had to observe it.
4. **The pagination entry point sits outside the hierarchy.** Workflow 2 walks pages with
   `api.action_pages(droplet_id, size, cap)` on the client, but retrieves with
   `api.droplet(droplet_id).action(id)` on the scope. The droplet id is passed twice, and the
   traversal is not reachable from the `api.droplet(id)` scope where the single-page
   `actions()` lives. Mild, and it was predicted.
5. **A traversal always requires a page size.** `droplet_pages` and `action_pages` require a
   `PageSize`. There is no "provider default (20)" traversal, although the single-page
   endpoints omit `per_page` by default. Mild, and it was predicted.
6. **Naming the partial result takes three type parameters, one of them irrelevant.** To keep
   the prefix in an application error, I had to write
   `Incomplete<Droplet, Error, NextLink>`, importing `concord_pagination::Incomplete` and the
   opaque `NextLink`. The caller never uses `NextLink`. The SDK has a `Traversal<T>` alias but
   no matching alias for the incomplete result.
7. **An unsafe next link discards the valid records of the page that carried it.** This is
   the main surprise; see prediction mismatch M1. Page 1 with two valid droplets and an
   untrusted `next` gives `collected == []` and `PagesError::Provider { failed:
   Resume::Start }`. When the bad link is on page 2, page 2's own record is dropped and
   `failed` points at page 2 (`Resume::At(..)`). In both cases the reported retry point is
   the same page, which will fail the same way when fetched again. The only way I found to
   get those records back was to leave the traversal for the single-page call
   (`api.droplets().list().set_per_page(..).send()`). That call returns the records and the
   raw, untrusted link as a plain `String` (the test demonstrates it).
8. **Misleading `Display` for the unsafe-link stop.** `Incomplete` prints
   "incomplete after 0 collected: a page request failed (after 0 pages emitted)", but the
   request succeeded with a 200 and only the continuation was refused. The SDK's own
   `Error::UnsafeNext` says "unsafe pagination target", which is clear, but only through
   `incomplete.error.provider_error()`. Withholding the provider error is a documented
   redaction choice. The "request failed" wording is not accurate for this case.
9. **Models implement only `Debug` and `Deserialize`.** No `Clone` or `PartialEq`, so the tests
   compare fields one by one, and a selected `Droplet` cannot be kept after its `Vec` is
   consumed without moving it. Minor.
10. **Per-page metadata is lost by `.items()`.** Getting quota headers per page meant switching
    to `next_page()`, whose `Page.meta` is the HTTP `ResponseMeta`. The body `meta.total`
    (`CollectionMeta`) is not reachable from a traversal at all, only from the single-page
    call. This was predicted, and the docs are consistent with it.

No incident needed a source lookup. I did not open `src/`, `tests/`, `fixtures/`,
`FRICTION.md` or `PROVENANCE.md` at any point.

## Prediction vs observation

| # | Prediction | Observed | Verdict |
| --- | --- | --- | --- |
| P1 | `for_fixture` performs no I/O; control characters give `Credential`; a non-literal-loopback, `https`, path, userinfo or production origin gives `FixtureOrigin` | as predicted; an empty token is also `Credential` (not predicted, probed) | match |
| P2 | `authorization: Bearer <token>` is sent | sent exactly so | match (it was uncertain) |
| P3 | `accept: application/json`, no referer, no body, no content-type | as predicted | match |
| P4 | First list request `GET /v2/droplets?per_page=2`, `page` omitted | as predicted | match |
| P5 | Next link followed verbatim with the same credentials | `GET /v2/droplets?page=2&per_page=2`, same headers | match |
| P6 | No `next` (no links, `{}`, `pages: {}`, `next: null`, only first/prev, large `meta.total`) means completion after one request | all six shapes complete after exactly one request | match |
| P7 | Empty intermediate page with `next` continues | it continues (3 requests in workflow 2) | match |
| P8 | Cap reached with more pages gives `Bounded` and keeps the prefix; later calls give `Ended`; a cap hit exactly on the last page is completion | as predicted | match |
| P9 | Untrusted next host: `UnsafeNext`, zero requests to that host | `UnsafeNext`; zero requests to the untrusted host and nothing followed, for all nine link variants (other port, `localhost`, `https`, other collection, userinfo, fragment, relative, empty, backslash) | security: match |
| **M1** | …and `Incomplete.collected` holds **page one's droplets** `[web-01, web-02]` | `collected == []`; `failed: Resume::Start`. The page carrying the bad link fails as a whole, so its records are not in the prefix (same for workflow 2's link to `/v2/droplets/999/actions`, and for a bad link on page 2, which drops page 2's record) | **mismatch** |
| P10 | A 302 to the untrusted host gives `Error::Provider`, `HttpClass::Other`, status 302; the redirect is not followed | as predicted; `document` is `Err` (empty body) | match |
| P11 | 429 with `retry-after: 30` gives `RateLimited`, `retry_after_seconds() == Ok(Some(30))`, prefix kept, no replay | as predicted; `Problem.id == "too_many_requests"`, `request_id` kept; provider message not printed | match |
| P12 | 401/403/404 give a typed `Problem`; 5xx with HTML or empty body gives `Server` with `document == Err`; one attempt each | as predicted; a 502 whose document says `not_found` is still `Server` | match |
| P13 | Truncated or wrong-shape 2xx gives `Error::Concord` with `ErrorKind::Decode`; 2xx `text/html` gives `ErrorKind::Metadata` | as predicted; a droplet missing required fields is also `Decode`; `head()` keeps status 200 | match |
| P14 | `ratelimit-*` parsed strictly: `Ok(None)` when missing, `Repeated`, `Invalid` | as predicted; overflow gives `FieldError::Overflow`; bad metadata does not fail the call | match |
| P15 | Formatting never shows the token or provider text | `Debug`/`Display` of client, prepared request, errors, `Incomplete` and `Response` do not contain the token, the provider messages, HTML bodies or the untrusted port; the client prints `authorization: <redacted>` | match |
| P16 | Workflow 2 routes `.../actions?per_page=2`, then `?page=2&per_page=2`, then `.../actions/{id}`; `kind` comes from JSON `type`; `completed_at == None` while in progress | as predicted | match |
| P17 | After an error the traversal gives `Ended`, never `Ok(None)` | as predicted (checked after `Bounded`) | match |
| P18 | Predicted friction: helpers on the client, `PageSize` required, per-page meta lost by `.items()` | all three observed | match |

The one real mismatch is M1. The credential guarantee held in every variant I tried. The
partial-result guarantee is weaker than I had read it to be: "the collected prefix" stops at
the last page whose own continuation was valid, not at the last page that was received.

## What I would change (recorded only; shared crates and SDK `src/` were not edited)

- For M1: let the traversal return the page's records with a *poisoned* continuation, and fail
  on the following step, so that `collected` keeps the received records and `failed` names the
  refused continuation instead of a page that can never succeed. The other option is to state
  in the README and in `droplet_pages`/`action_pages` that an unsafe link fails the page that
  carried it, and that its retry point fails deterministically.
- Link `concord::Error` explicitly in `Error::Concord` and `ProviderError.document`. A
  `--no-deps` rustdoc does not disambiguate it. Also export an `IncompleteListing<T>` alias next
  to `Traversal<T>`.
- Say in the `connect`/`for_fixture` rustdoc that the `Bearer ` prefix is added.

## Assistance label

`docs-only`. I used the SDK README; the rustdoc of `concord-sdk-digitalocean`, `concord`,
`concord-pagination` and `concord-example-testserver`; and the two official DigitalOcean pages
(read 2026-11-07 through WebFetch, so the content was relayed by a summarising model). I read
no SDK source, tests, fixtures, FRICTION.md or PROVENANCE.md. Temporary probes (a diagnostic
`eprintln!` test and backups of `first_use.rs`) lived under `target/tmp` or were removed before
the final run.
