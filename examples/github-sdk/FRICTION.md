# Friction log: the GitHub SDK slice and its workflows

Author's honest account of building this SDK on Concord 4.2 (`concord`, `concord-extras::stream`)
and the 4.4 `concord-pagination` crate. The author did not design these APIs. Each entry gives
the provider contract, the public path used, the extra code, and a verdict: **flexibility** (the
documented boundary did its job), **friction** (repeated or avoidable work), or **defect**.
Line counts are of this repository at the commit that adds this file.

Summary: no handwritten-`reqwest` client was needed; all 23 operations are generated endpoints.
Everything outside a plain `send()` went through `Prepared::into_parts` + `Decoder`, which held up
well. The real cost was the same ~70-line metadata wrapper and ~90-line context mirror every SDK
writes, a raw-download loop that the stream module cannot help with, and two gaps in the
pagination item consumer.

## F1 — Response metadata next to the typed value (friction, repeated across SDKs)

**Contract.** GitHub puts what an application needs in headers: `x-github-request-id`,
`x-ratelimit-*`, `ETag`, `Link`.

**Path used.** `src/meta.rs::send_edited` (the native recipe of `examples/native/src/metadata.rs`,
re-implemented because that is an example crate, not a library):

```rust
let (client, mut request, decoder) = prepared.into_parts();
edit(&mut request)?;
let response = match client.execute(request).await {
    Ok(response) => response,
    Err(e) => {
        let Err(error) = decoder.finish(Err(e)).await else {
            unreachable!("`Decoder::finish(Err(_))` is always `Err`")
        };
        return Err(GitHubError::api(error, None));
    }
};
let meta = ResponseMeta::capture(&response);
match decoder.decode(response).await { /* value + meta, or error + meta */ }
```

**Extra code.** `meta.rs` is 277 lines; about 70 are this plumbing (`send_edited`,
`send_conditional`), the rest is GitHub's own header interpretation (which belongs in an SDK).
Every 4.4 SDK needs the same plumbing for its own provider headers.

**Proposal (core, small).** A head-observing send that keeps Concord's conversions:
`Prepared::send_with_head<H>(self, keep: impl FnOnce(&ResponseHead<'_>) -> H) -> Result<(T, H), (Error, Option<H>)>`
(or the same on `Decoder`). It would remove the plumbing above, the `unreachable!` and the
"was a head received" bookkeeping from each SDK.

**Status (Concord response boundary, `Received`).** Addressed by `Prepared::receive` /
`Decoder::execute` (one execution, any status, body unread) and `Received::decode`: `send_edited`,
`send_with_meta` and `send_conditional` no longer execute or convert anything themselves, and a
transport failure has no metadata by construction. The headers are still copied from the native
response (`Received::into_parts`, then `Decoder::receive`), because `Received::head` carries no
URL and relative `Link` targets resolve against the response URL.

## F2 — Converting a native transport error needs an async call and `unreachable!` (friction)

`Decoder::finish(Err(e))` is the only public way to turn a `reqwest::Error` into Concord's
redacted `Transport` error. It is `async` and returns `Result<T, Error>`, so each use is the
`let Err(error) = ... else { unreachable!() }` shown in F1. This SDK has it four times (`meta.rs`
twice, `download.rs` twice, the latter with `Decoder::<()>::drain()` for a request no declaration
owns). **Proposal:** a synchronous `Decoder::transport_error(&self, e: reqwest::Error) -> Error`
(or `concord::Error::from_transport_unlabeled`).

**Status.** `Decoder::transport_error` exists, and none of the four uses remains: `meta.rs` and
`download::locate` use `receive`/`Decoder::execute`; `download::stream_to` uses
`Decoder::incoming().execute(..)` and `Received::into_incoming` (status check, response unread).

## F3 — Following a provider's `next` URL with a declared endpoint (justified escape hatch)

**Contract.** GitHub: follow `Link: rel="next"` exactly; do not build page queries; the URL may
use another path form (`/repositories/{id}/issues?...`) and another pagination style per endpoint.

**Path used.** `src/paging.rs::fetch_page`: prepare the **same** generated operation (headers,
decoder, endpoint name), then replace its native URL:

```rust
send_edited(prepared, |request| {
    if let Some(key) = key {
        *request.url_mut() = key.url;   // checked first: same origin, under the base path
    }
    Ok(())
})
```

`RECIPES.md` warns against using a declared endpoint as a seed for another route. Here it is the
same endpoint and response type, so the declared headers and decoder are right; the risk the
recipe names (sending the token somewhere else) is handled by `GitHubContext::is_api_url` (origin
and base-path prefix) and a binding check (operation, repository, parent) before anything is sent.
Tests: `tests/paging.rs::{foreign_next_link_is_refused, link_outside_the_base_path_is_refused,
continuation_binding_is_checked}`.

**Verdict.** Flexibility, but undocumented: every Link/next-URL provider (GitHub, Salesforce
`nextRecordsUrl`, Shopify bulk URLs) will write this. **Proposal (docs):** a recipe "the same
endpoint at a provider-supplied URL" with the origin check, distinct from the seed-rewrite warning.

## F4 — `304 Not Modified` needs the native boundary (flexibility)

`send()` makes `304` a `Status` error (non-2xx), correctly. `send_conditional` (in `meta.rs`)
checks the status before handing the response to the decoder. A caller who only wants "unchanged
or not" can `match send()` on `error.status() == Some(304)`; one who wants the new `ETag` on `200`
needs F1's path. Fine as is.

## F5 — Redirect policy is client-wide; the token and the artifact `302` (friction, reqwest)

**Contract.** GitHub asks clients to follow redirects; the artifact download is a `302` to a
signed URL on another host, valid one minute.

**Observed** (`tests/download.rs::client_followed_redirect_is_refused`): with reqwest's default
policy the client follows the `302` itself; reqwest strips `Authorization` across hosts but
forwards `X-GitHub-Api-Version` and `User-Agent`, and the SDK never sees the `Location`.

**Path used.** The SDK exports `redirect_policy()` (follow same-origin, stop at cross-origin) for
the caller's `reqwest::ClientBuilder`, reads the `302` through `into_parts` (`download::locate`),
and fetches the location with a **new** native request carrying no header. If the client followed
the redirect anyway, `locate` refuses the response (`LocateError::RedirectFollowed`).

**Verdict.** Concord cannot fix a reqwest-level limitation (no per-request redirect policy), and
the native boundary worked. The friction is that correctness depends on how the caller built its
`reqwest::Client`, detectable only at run time. The caller guide's "redirects: reqwest forwards
custom headers" note was accurate and useful.

## F6 — The context pattern mirrors the declaration by hand (friction, repeated across SDKs)

**Path used.** `src/context.rs` (452 lines) follows `examples/sdk/src/context.rs`: it keeps the
token, API version and user agent next to the generated client so that undeclared operations
(`RepoContext::request`) get the same headers and path encoding. About 90 lines (`build`,
`push_segment`) duplicate what the macro already knows; `tests/operations.rs::undeclared_operation_through_the_context`
compares the two header sets so they cannot drift silently.

**Proposal (core).** A generated, redacted "native request under this scope" on every scope
handle (`GitHubRepo::native_request(Method, &[&str])`), reusing the scope's composed headers and
segment encoding. It would delete the mirror and the drift test from every SDK.

## F7 — Newtype values in headers need templates (minor friction)

`header(X_GITHUB_API_VERSION) api_version: ApiVersion` does not compile: direct header placement
takes the sealed `ToHeaderValue` (`HeaderValue`, `str`, `String`, integers, `bool`). The template
form works and reads fine:

```text
headers { X_GITHUB_API_VERSION = "{api_version}"; IF_NONE_MATCH = "{if_none_match}"; }
```

An optional value in a template (`if_none_match?: ETag`) correctly sends no header when absent.

## F8 — A raw (non-record) streamed download gets no help from `concord::stream` (friction)

**Contract.** The artifact is a ZIP: no record codec applies; bytes must go to a sink with a byte
bound and honest partial-progress errors.

**Path used.** `src/download.rs::stream_to`: `ByteSource::next_chunk` on the response (useful: its
`SourceError` has the URL removed), plus a hand-written status check, `Content-Length` check, byte
counting, sink hand-off and a `StreamError<E>` type (~200 of the file's 494 lines). The stream
module's `open_response`, `Driver` and `OpenError`/`ReadError` constructors are crate-private, so
an SDK cannot reuse the opening/position machinery for raw bytes.

**Proposal (extras).** A public raw-bytes reader: `concord::stream::RawBytes::open(response,
operation, limits) -> Result<RawBytes, OpenError>` with `next() -> Option<Result<Bytes, ReadError>>`
and positions, so raw downloads share `OpenError`/`ReadError` with the record readers.

## F9 — `concord-pagination` `Items` drops page metadata (friction)

GH-A walks the issue listing with `resume_items` (it needs item-level continuations). `Items`
yields items only; the `Page::meta` (`PageInfo`: request id, rate limit) of each page is dropped,
and `ItemsReport` does not keep it. The workflow records metadata through a side channel inside
the factory future (`examples/github-app/src/requests.rs::record_page`, an `Arc<Mutex<Vec<_>>>`):

```rust
let mut items = resume_items(start, budget, |key| {
    let page = pages.fetch(key);
    let log = parent_log.clone();
    async move { record_page(operation::ISSUES, page.await, &log) }
});
```

**Proposal.** `Items::page_meta(&self) -> Option<&M>` for the page currently being consumed (or
`next_with_page() -> Option<Result<(T, &M), E>>`).

## F10 — No checkpoint "at the item just emitted" (friction, observed)

After a child collection fails, GH-A must resume *at* the parent item whose child failed (re-emit
it). `Items::checkpoint()` after `next()` already counts that item as emitted, and when it was the
last item of its page the checkpoint has moved to the next page, whose predecessor key is gone. The
workflow therefore snapshots the checkpoint **before** each `next()`. Observed consequence
(`tests/gh_a_activity_audit.rs::child_failure_after_successes_then_resume`): when `next()` crossed
an empty page inside the same call, the snapshot names the empty page, and resuming costs one extra
request. Correct, but less precise than it could be. **Proposal:** `Items::next_with_checkpoint()`
returning the item together with the `ItemCheckpoint` that re-emits it.

## F11 — Filter records repeat the declaration (friction, maintenance)

`IssueFilter`/`RunFilter` plus the `RepoContext::issues`/`runs` wrappers repeat each optional
query value three times (declaration, record field, `maybe_*` line). Adding a GitHub filter means
editing three places. A record field without a mapping is a compile error, because the wrapper
destructures the record exhaustively (`let IssueFilter { state, labels, since, .. all fields } =
filter.clone();`); a declared parameter without a record field compiles silently (the generated
setter still exposes it, the record just does not). The 4.3 generated-inputs experiment addresses
exactly this duplication.

## F12 — Things that worked without friction

- 23 operations, two nested scopes (`repo`, `actions`), header composition, `#[sensitive]` token:
  the declaration compiled on the first attempt; every request target in
  `tests/operations.rs::every_operation_on_the_wire` matched expectations except one where the
  expectation was wrong (`*` is not percent-encoded in form-encoded queries).
- `Decoder::<()>::drain()` gave redacted transport and status errors for the download host
  request, which no declaration owns.
- `concord_pagination::sequential` with a borrowing closure (`|key| pages.fetch(key)`), mandatory
  budgets and the `SequentialReport { failed, resume }` split mapped directly onto GH-A's child
  collections and GH-B's run/job/artifact scans; early stop (`break` + `finish`) was natural.
  **Later status (`concord_pagination::Pages`):** every listing now also has ordinary
  traversals (`LinkPages::pages`/`items`, owned, a cap or a failed page is a typed error), and
  GH-B's three scans use them: their end comes from the traversal (`Ok(None)`, `Bounded`,
  `Provider`) rather than from a separately read report, and each page's metadata from the page
  itself (no `record_page` side channel). GH-A stays on the engine: its parent resumes from an
  item checkpoint (`resume_items`), which the facade cannot start from, and its continuation is
  assembled from the engine's reports.
- `Page<T, K, M>` with a typed `M` let the SDK keep `ResponseMeta` per page without wrappers.
- Typed errors stayed typed end to end (`GitHubError::Api` keeps the Concord error and its unread
  response; the workflow outcomes keep `GitHubError`, `LocateError`, `StreamError<E>`).
