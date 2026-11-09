# Intercom contacts SDK29

`concord-sdk-intercom` is a bounded Intercom **2.17** contacts SDK using Concord's
client → contacts scope → endpoint hierarchy. It lists contacts with provider
cursor pagination, searches by exact email, retrieves by Intercom ID, and creates by email with optional
name and role. Contact models intentionally expose only ID, role, external ID,
email and name; additional response fields are ignored.

**Live provider calls: NOT_RUN.** All executable workflows use the loopback
`concord-example-testserver` and authored fixtures. This work is **example-assisted**
through the required guide excerpts and testserver; the exact ledger is in
[PROVENANCE.md](PROVENANCE.md). No other SDK source file was opened directly.

The request contract comes from Intercom's [list](https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/listcontacts),
[get](https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/showcontact)
and [create](https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/createcontact)
references, read **2026-11-07**. Every request sends a sensitive Bearer authorization
header, `Intercom-Version: 2.17`, and `Accept: application/json`.

## Calling the slice

Use a local testserver's URL for `base_url`. The snippet is compiled as a doctest
but never executed by rustdoc. `with_token` takes the raw token, without `Bearer`.

```no_run
use concord_sdk_intercom::{Intercom, NewContact, PageSize, http_client, pagination::Resume};
use std::num::NonZeroUsize;

async fn workflows(base_url: &str, token: &str) -> Result<(), Box<dyn std::error::Error>> {
    let api = Intercom::with_token(http_client()?, token)?.with_base_url(base_url)?;

    let contacts = api.contacts().list()
        .set_per_page(PageSize::new(50)?)
        .pages(Resume::Start, NonZeroUsize::new(20).unwrap())
        .items().collect_complete().await?;
    if let Some(contact) = contacts.first() {
        let retrieved = api.contacts().get(&contact.id).send().await?;
        assert_eq!(retrieved.value.id, contact.id);
    }

    let created = api.contacts()
        .create(NewContact::with_email("created@example.test"))
        .send().await?.value;
    let retrieved = api.contacts().get(&created.id).send().await?;
    assert_eq!(retrieved.value.id, created.id);
    Ok(())
}
```

On success, `list().send()` returns one `Response<ContactList>`; `get` and `create` return
`Response<Contact>`. Each response holds `.value` and `.meta`. The generated
setters and `prepare()` remain available. List page sizes default to 10 and are
validated locally as 1..=150. The SDK preserves the provider's default identity
escaping; it does not HTML-unescape returned names or email addresses.

`list().pages(from, max_pages)` preserves the template's page size and replaces
its cursor with `from`. It follows **only** `pages.next.starting_after`, encoded
as one query value; it never treats a cursor as a URL, increments a page number,
or trusts `total_count` to decide completion. An empty intermediate page still
continues. Explicit `pages: null`, or missing/null `next`, is terminal; a missing
`pages` field fails decoding. Empty and immediately repeated cursors fail; longer
cycles are bounded by the mandatory page cap. The official [list pagination
guide](https://developers.intercom.com/docs/build-an-integration/learn-more/rest-apis/pagination-cursor)
describes the continuation shape (read 2026-11-07).

The shared traversal reports a cap or failed page as an error, retains the
collected prefix in `Incomplete`, and exposes the failed/resume key. Resuming is
an explicit caller action with the same base, credentials, version and page
size; changing those invalidates that context. A listing that changes during
pagination is not a stable snapshot. Streaming pages/items avoids retaining an
entire listing; collecting uses memory proportional to all yielded contacts.

## Exact-email search

`contacts().search(EmailSearch::new(email)).send().await?` returns one
`Response<ContactList>`. The typed input fixes the query to `field: "email"`,
`operator: "="`, and the supplied email as `value`. It sends the email verbatim;
equality and email validation belong to Intercom. Search defaults to **50** per
page; `with_per_page(PageSize::new(n)?)` accepts 1..=150. The request is
`POST /contacts/search` with `query` and `pagination` JSON objects. Both
`per_page` and optional `starting_after` live inside `pagination`, with no URL
query parameters. See [Search contacts](https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/searchcontacts)
and [Pagination for Search APIs](https://developers.intercom.com/docs/build-an-integration/learn-more/rest-apis/pagination#pagination-for-search-apis),
read **2026-11-07**.

```no_run
use concord_sdk_intercom::{EmailSearch, Intercom, PageSize, pagination::Resume};
use std::num::NonZeroUsize;

async fn find_matches(api: &Intercom) -> Result<(), Box<dyn std::error::Error>> {
    let search = api.contacts().search(
        EmailSearch::new("person@example.test").with_per_page(PageSize::new(50)?)
    );
    let cap = NonZeroUsize::new(20).unwrap();
    match search.clone().pages(Resume::Start, cap).items().collect_complete().await {
        Ok(contacts) => { /* all matches in this traversal */ }
        Err(incomplete) => {
            let prefix = incomplete.collected;
            let resume = incomplete.error.resume().cloned();
            if let Some(error) = incomplete.error.provider_error() {
                // Inspect a 429 here; a missing reset remains unknown. Do not auto-retry.
                let status = error.status();
            }
            // Keep prefix and resume. After an explicit caller decision, a new
            // search.pages(resume.unwrap(), cap) continues the same query.
        }
    }
    Ok(())
}
```

Search shares listing's bounded traversal, metadata and failure contract.
`pages(from, cap)` replaces any cursor set with `with_starting_after`; it keeps
the email and page size on every page. Reuse a resume key only with the **same
email**, credentials, base, version and page size. A headerless 429 preserves
the collected prefix and failed cursor, reports `HeaderNumber::Missing` reset
and no Retry-After, and stops without retrying. Search is not a snapshot, and
recently created contacts may take time to appear in results. Retrieve a newly
created contact by its returned ID as shown above.

## Failure semantics

`send()` returns `Result<Response<T>, Error>` and performs one application execution. `http_client()` disables reqwest
retries and redirects and sets a 15-second timeout. A caller-supplied transport
can change those guarantees. GETs, searches and creates stop on failures; the SDK
never refreshes credentials or retries automatically. A lost create response
may mean the contact was created: callers must reconcile before deciding to
issue another create. A 410 merged-contact response remains an error; its raw
canonical `Link` is available in metadata and is not followed.

`Error::Provider` retains the non-2xx status, metadata, and either a typed
`error.list` document or its decoding failure. This covers 400, 401, 403, 404,
405, 406, 408, 409, 410, 422, 429, 500, 502, 503, 504 and unknown statuses. Codes
are extensible strings or integer validation codes, and absent request IDs and
messages are supported. The [HTTP response](https://developers.intercom.com/docs/references/rest-api/errors/http-responses),
[error code](https://developers.intercom.com/docs/references/rest-api/errors/error-codes)
and [error object](https://developers.intercom.com/docs/references/rest-api/errors/error-objects)
references were read 2026-11-07.

Success bodies are limited to **2 MiB** and error documents to **64 KiB**.
Strict JSON media checking runs before body reading. Malformed, wrong-shape,
truncated, oversized and wrongly labelled bodies preserve the response status
and the precise Concord failure stage. `Error::Concord` also covers pre-I/O
build errors and transport failures. `prepare()?.send()` keeps the declared
success bound but uses Concord's own status/error behavior, bypassing the SDK
error-document policy.

On a 429, inspect `provider.rate_limits()`: each header is `Missing`, `Invalid`,
or `Value`. `reset_unix_seconds` contains only a valid provider-supplied integer.
**A missing reset remains unknown.** There is no invented reset, zero-delay
retry, rate wait, or background activity. `provider.retry_after(now)` uses the
shared strict Retry-After parser, returning an optional delay or a parse error;
it does not schedule a retry or supply a reset value. Intercom explicitly warns
that some 429s omit rate headers in its [rate-limit reference](https://developers.intercom.com/docs/references/rest-api/errors/rate-limiting)
(read 2026-11-07).

SDK `Debug` and error `Display` withhold credentials, cursors, contact IDs,
contact data, provider messages and header values. Explicit access to fields,
headers, raw Concord error bodies/sources and native requests is unredacted.
`serde_json::Value` obtained through explicit field access has its normal Debug.
The base URL is public configuration and must not contain a secret.

Conversations, webhooks, merges, canonical-link following, external-ID lookup,
updates/deletes, search filters other than exact email, OAuth acquisition/refresh, distributed admission, and
automatic replay of ambiguous creates are excluded.

## Verification

From the workspace root, with `CARGO_BUILD_JOBS=4` and its default `target/`:

```sh
cargo test --locked -p concord-sdk-intercom
cargo clippy --locked -p concord-sdk-intercom --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-intercom --no-deps
```

The initial build and first checks used offline mode; see [VALIDATION.md](VALIDATION.md) for the
actual command results, scope checks and resource observations.
The search maintenance changes and their validation are recorded in
[MAINTENANCE.md](MAINTENANCE.md).
