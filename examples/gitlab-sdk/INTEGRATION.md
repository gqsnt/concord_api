# Integration record

## Between waves 2-3

Coordinator integration of the between-wave library refinements (`send Policy;`,
`Prepared::follow`, `Received::decode_limited`) at Concord `1f71432c`. Role: integrator,
Claude Opus 5.5 agent session (requested model; this record does not re-derive it from
harness metadata). Not an independent review: nobody has reviewed this change yet.

**Verdict: adopted (send policy and `Prepared::follow`).** The client declares
`send SendPolicy;`, so every generated `send()` returns `Result<Response<T>, GitLabError>`, and
the `Call` trait (with its import) is gone. Decisive reason: the consumer's friction
("choosing `.call()` for SDK errors", FIRST_USE.md) and the author's friction 8 (two error
types per operation) go away because `send()` itself is the SDK path; nothing is left to choose
or import. Linked pages use `Prepared::follow` instead of `into_parts` + `url_mut` +
`Decoder::execute`. The SDK's link rules are unchanged, and the library now checks the origin a
second time. `decode_limited` was not adopted: GitLab success bodies are bounded by `per_page`
(at most 100 issues), and no friction asked for a cap.

### Declaration

Before (`src/api.rs`, `1f71432c`):

```rust,ignore
    pub client GitLab(
        #[sensitive] header(PRIVATE_TOKEN) token: HeaderValue,
    ) {
        base "https://gitlab.com/api/v4";
        format Json;
```

After:

```rust,ignore
    /// Every operation's `send()` returns `Result<`[`Response<T>`](crate::Response)`, `
    /// [`GitLabError`](crate::GitLabError)`>` ([`SendPolicy`]). `with_base_url` and `prepare()`
    /// return `concord::Error` (no request is sent).
    pub client GitLab(
        #[sensitive] header(PRIVATE_TOKEN) token: HeaderValue,
    ) {
        base "https://gitlab.com/api/v4";
        send SendPolicy;
        format Json;
```

The three operations and the project scope are unchanged.

### Adapter / policy code

Before (`src/call.rs`, docs elided):

```rust,ignore
pub trait Call<T>: Endpoint<T> {
    fn call(self) -> impl Future<Output = Result<Response<T>, GitLabError>> + Send;
}

impl<T, E> Call<T> for E
where
    T: Send + 'static,
    E: Endpoint<T>,
{
    fn call(self) -> impl Future<Output = Result<Response<T>, GitLabError>> + Send {
        let prepared = self.prepare();
        async move {
            let received = prepared?.receive().await?;
            finish(received).await
        }
    }
}
```

After (`src/send.rs`, renamed from `src/call.rs`; docs elided):

```rust,ignore
pub struct SendPolicy;

impl<T: Send + 'static> SendWith<T> for SendPolicy {
    type Output = Result<Response<T>, GitLabError>;

    async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        finish(received).await
    }
}
```

`finish`, `decode` and `api_error` (status branch, bounded error document, decoding) are
unchanged and still shared with the page traversal.

Link following, before (`src/pages.rs` `fetch_prepared`):

```rust,ignore
    let (client, mut request, decoder) = prepared?.into_parts();
    let first = request.url().clone();
    // ... continuation binding, check_link ...
        *request.url_mut() = key.url;
    }
    let mut requested = request.url().clone();
    requested.set_fragment(None);

    let received = decoder.execute(&client, request).await?;
```

After:

```rust,ignore
    let mut prepared = prepared?;
    let first = prepared.request().url().clone();
    // ... continuation binding, check_link (unchanged) ...
        prepared = prepared.follow(key.url.as_str())?;
    }
    let mut requested = prepared.request().url().clone();
    requested.set_fragment(None);

    let received = prepared.receive().await?;
```

Lines of SDK adapter code (counted by script: non-blank lines that are not `//` comments,
total in brackets):

| | before | after |
| --- | --- | --- |
| `Call` trait + impl vs `SendPolicy` (`call.rs` 1-47 vs `send.rs` 1-38) | 20 (47) | 11 (38) |
| shared `finish`/`decode`/`api_error` (`call.rs` 48-92 vs `send.rs` 39-83) | 39 (45) | 39 (45) |
| `fetch_prepared` and below (`pages.rs` 129-175) | 43 (47) | 43 (47) |

The link code has the same length. It no longer leaves `Prepared`: the request keeps
Concord's redaction (`Debug` withholds a followed URL), and the native calls (`into_parts`,
`url_mut`, `Decoder::execute`) are now `follow`, then `receive`.

What remains of the author's friction 4: each linked page still prepares the listing first,
building the first-page URL and query and then discarding them, because `follow` is a method
of `Prepared`. That costs a URL build per page and no I/O. Not a library gap for this slice.

### One caller call

Before (`src/lib.rs` crate example):

```rust,ignore
use concord_sdk_gitlab::{Call, GitLab, IssueIid, IssueState, PrivateToken, ProjectRef};
// ...
let first = project.issue(IssueIid::new(1)?).call().await?;
```

After:

```rust,ignore
use concord_sdk_gitlab::{GitLab, IssueIid, IssueState, PrivateToken, ProjectRef};
// ...
let first = project.issue(IssueIid::new(1)?).send().await?;
```

Same result type (`Result<Response<Issue>, GitLabError>`), no trait import. The Concord-only
result is `project.issue(iid).prepare()?.send().await` (`Result<Issue, concord::Error>`).

### What a maintainer edits to add an operation

Before and after: the declaration only. The generic `impl<T: Send + 'static> SendWith<T>` covers
every output, as the blanket `Call` impl did. What changes for the maintainer is the callers'
side: a new operation's `send()` is already the SDK path, so README examples need no `Call`
import and there is no `send()` that loses GitLab's error document.

### Safeguards re-verified (mutation runs against the migrated code)

Each mutation was applied to the committed code, the three test targets were run
(`cargo test --locked -p concord-sdk-gitlab --tests --no-fail-fast`), and the file was restored
from the commit. None of these mutations is committed.

| Mutation | Tests that failed (caught) |
| --- | --- |
| M1 `check_link`: origin comparison removed | `protocol::a_link_to_another_origin_is_never_requested`, `first_use::a_foreign_link_is_not_requested` |
| M2 `check_link`: any collection accepted | `protocol::a_link_to_another_collection_is_refused`, `first_use::a_bad_next_links_fail_instead_of_silently_ending_or_looping` |
| M3 continuation binding (`key.listing != listing`) disabled | `protocol::a_continuation_of_another_listing_is_refused_before_sending`, `first_use::a_continuation_from_another_project_is_refused_without_io` |
| M4 `NotAdvancing` check disabled | `protocol::a_link_that_does_not_advance_is_refused`, `first_use::a_bad_next_links_fail_instead_of_silently_ending_or_looping` |
| M5 `MissingNextLink` (`x-next-page` without `next`) disabled | `protocol::x_next_page_without_a_next_link_is_refused`, `first_use::a_bad_next_links_fail_instead_of_silently_ending_or_looping` |
| M6 `redirect_policy()` follows a redirect that leaves the origin | `protocol::a_same_origin_redirect_is_followed_and_a_foreign_one_is_not`, `first_use::b_documented_redirect_policy_does_not_forward_token_to_another_origin` |
| M7 malformed `Link` read as "last page" | `protocol::a_malformed_link_is_an_error_not_the_last_page`, `first_use::a_bad_next_links_fail_instead_of_silently_ending_or_looping` |
| M8 `follow` line removed (link not followed) | 6 first_use, 1 protocol, 3 workflows tests (including `changed_link_query_survives_cap_and_failed_retry`) |

The rest of each run passed.

Second origin check, observed with a temporary probe test (deleted, not committed) under M1:
the first page returns the foreign `next` link, and the next page fails before any I/O with
`GitLabError::Request` kind `Build`, "followed URL has another origin than the endpoint's". The
foreign server received **0** requests. Under M1 the SDK reports the wrong problem (`Build`
instead of `ForeignLink`), so the tests still fail, but the token stays on the API origin.
Control: the same probe and the same M1 applied to the `1f71432c` sources (native `url_mut`
replacement; restored right after) let the second page succeed. The foreign server received
**1** request, and it carried `private-token`.

### Test changes, each explained

No expected value was changed.

1. `.call()` became `.send()`, and `Call` was removed from the imports: 15 call sites in
   `tests/protocol.rs`, 2 in `tests/first_use.rs` (one is in item 2), 1 in `README.md`, 1 in
   the crate example, 2 in `src/context.rs`. Mechanical; result type unchanged.
2. `tests/first_use.rs`: `b_single_calls_require_call_for_the_sdk_error_document_and_metadata`
   was renamed to `b_single_calls_send_the_sdk_error_document_and_metadata`. Its Concord-only
   half (`issue.clone().send()` giving `concord::Error` kind `Status`, 404) now calls
   `issue.clone().prepare().unwrap().send()`. Its SDK half (`issue.call()`, now `issue.send()`)
   is unchanged: `GitLabError::Api`, 404, `"404 Not Found"`, request id `first-use-call`, two
   requests. One `panic!` message changed from "Call adds..." to "send() adds...".
3. The maintenance test `changed_link_query_survives_cap_and_failed_retry` and every other
   workflow test are unmodified and pass. Exact linked targets survive `follow`, including
   repeated keys, an empty value, `+` and an encoded cursor.

### Verification (observed)

At `1f71432c` before the change: 18 first_use + 23 protocol + 11 workflows + 9 doctests passed.

After the change (`CARGO_BUILD_JOBS=6`):

- `cargo test --locked -p concord-sdk-gitlab`: 18 first_use + 23 protocol + 11 workflows + 9
  doctests passed, 0 failed.
- `cargo clippy --locked -p concord-sdk-gitlab --all-targets -- -D warnings`: clean.
- `cargo doc --locked -p concord-sdk-gitlab --no-deps`: no warnings. One
  `redundant_explicit_links` warning came up during the change and was fixed.
- `cargo fmt -p concord-sdk-gitlab -- --check`: clean.
- Mutation runs M1-M8 above.

Offline only; no GitLab instance was contacted.
