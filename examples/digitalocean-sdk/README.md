# concord-sdk-digitalocean

A bounded, read-only DigitalOcean slice using Concord's client → scope → endpoint
declaration and generated `send()` methods. Implements both connected workflows:

1. List droplets through body `links.pages.next`, then retrieve a selected droplet.
2. List that droplet's actions through the same pagination contract, then retrieve an action.

It also lists only the droplets carrying one tag, and a droplet's snapshots and backups, with
the same pagination contract.

| Generated call | HTTP route |
| --- | --- |
| `api.droplets().list()` | `GET /v2/droplets` |
| `api.droplets().list().set_tag_name(tag)` | `GET /v2/droplets?tag_name={tag}` |
| `api.droplet(id).get()` | `GET /v2/droplets/{id}` |
| `api.droplet(id).actions()` | `GET /v2/droplets/{id}/actions` |
| `api.droplet(id).action(action_id)` | `GET /v2/droplets/{id}/actions/{action_id}` |
| `api.droplet(id).snapshots()` | `GET /v2/droplets/{id}/snapshots` |

| Traversal (`Traversal<T>`) | Resume from a reported position | Pages of |
| --- | --- | --- |
| `droplet_pages(size, cap)` | `droplet_pages(size, cap).starting_at(from)` | `Droplet` |
| `tagged_droplet_pages(&tag, size, cap)` | `tagged_droplet_pages(&tag, size, cap).starting_at(from)` | `Droplet` |
| `action_pages(id, size, cap)` | `action_pages(id, size, cap).starting_at(from)` | `Action` |
| `snapshot_pages(id, size, cap)` | `snapshot_pages(id, size, cap).starting_at(from)` | `Snapshot` |

The models expose selected infrastructure, network and action fields. Additional JSON
fields are ignored; timestamps and status/type values retain their provider strings.
All action fixtures are synthetic; the permitted source pages do not specify the complete
Actions schema. See [PROVENANCE.md](PROVENANCE.md) for that evidence boundary.

## Calling

Depend on this package by path in this workspace. Concord and concord-pagination are
re-exported for result and error handling. This example compiles as a doctest without
executing a network request. Pass a running local fixture server's origin to run it:

```no_run
use concord_sdk_digitalocean::{DigitalOcean, PageSize};
use std::num::NonZeroUsize;

async fn inspect_fixture(origin: &str) -> Result<(), Box<dyn std::error::Error>> {
    let api = DigitalOcean::for_fixture("synthetic-local-token", origin)?;
    let cap = NonZeroUsize::new(10).unwrap();
    let size = PageSize::new(100)?;
    let droplets = api.droplet_pages(size, cap)
        .items().collect_complete().await?;
    if let Some(selected) = droplets.first() {
        let droplet = api.droplet(selected.id).get().send().await?;
        let actions = api.action_pages(droplet.value.droplet.id, size, cap)
            .items().collect_complete().await?;
        if let Some(selected_action) = actions.first() {
            let action = api.droplet(selected.id).action(selected_action.id).send().await?;
            assert_eq!(action.value.action.id, selected_action.id);
        }
    }
    Ok(())
}
```

Tagged droplets, resumed once after the cap, then each droplet's backups:

```no_run
use concord_sdk_digitalocean::{DigitalOcean, PageSize, TagName};
use concord_sdk_digitalocean::concord_pagination::PagesError;
use std::num::NonZeroUsize;

async fn production_backups(origin: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let api = DigitalOcean::for_fixture("synthetic-local-token", origin)?;
    let (tag, size, cap) = (TagName::new("env:prod")?, PageSize::new(50)?, NonZeroUsize::MIN);
    let mut droplets = Vec::new();
    let mut pages = api.tagged_droplet_pages(&tag, size, cap).items();
    loop {
        match pages.collect_complete().await {
            Ok(rest) => break droplets.extend(rest),
            Err(stop) if matches!(stop.error, PagesError::Bounded { .. }) => {
                droplets.extend(stop.collected);
                let from = stop.error.resume().cloned().expect("a bound reports its position");
                pages = api.tagged_droplet_pages(&tag, size, cap).starting_at(from).items();
            }
            Err(stop) => return Err(stop.into()),
        }
    }
    let mut backups = 0;
    for droplet in &droplets {
        let snapshots = api.snapshot_pages(droplet.id, size, cap).items().collect_complete().await?;
        backups += snapshots.iter().filter(|s| s.kind == "backup").count();
    }
    Ok(backups)
}
```

`DigitalOcean::connect(token)` constructs the production client without sending anything.
Its bearer token must already have the required read permissions. Token acquisition,
refresh and persistence belong to the caller. `for_fixture` accepts only a bare HTTP
origin using a literal loopback IP. Both constructors disable redirects, native retries,
Referer and proxies, and set a 30-second request timeout. Generated `new`, credential
setters, `with_base_url`, `prepare` and native exits remain available for advanced users;
when supplying a transport yourself, preserve those transport and destination rules.

List operations support `set_page(PageNumber::new(n)?)` and
`set_per_page(PageSize::new(n)?)`; without setters they omit both parameters and use
provider defaults. Page numbers start at 1 and page sizes range from 1 through 200.

`TagName::new(name)?` validates a tag before any request: an empty name (which would list
every droplet), whitespace and control characters are `Error::TagName`. The tag is sent
percent-encoded as `tag_name` and compared exactly; the provider matches its own spelling.
The provider documents that a tag filter needs the token's `tag:read` scope as well as
`droplet:read`; a refusal arrives as an ordinary HTTP failure.
Droplet name and GPU (`type`) filters, which the provider does not combine with `tag_name`,
remain outside this slice.

`Snapshot` exposes `id`, `name`, `created_at`, `regions`, `min_disk_size`, `size_gigabytes`
and `kind` (the JSON `type`: `snapshot` for user-created images, `backup` for automatic
ones, kept as an open string).

## Completion and trust

Every traversal requires a nonzero page cap. The shared pagination
driver returns `Ok(None)` only after the last provider page. Missing `links`, `pages`,
or `next` means completion (null is also accepted as absence). A total count or an empty
page never invents continuation: an empty page with `next` continues. Only `next` is
followed; `first`, `prev` and `last` are irrelevant to forward completion.

A present next link must be an absolute URL with the same scheme, host, effective port
and exact collection path as the original list endpoint, and the same `tag_name` values:
exactly the traversal's tag, or none for an unfiltered listing. A link that drops, changes,
repeats or adds the tag is refused instead of silently widening or narrowing the listing.
User information, fragments, control characters and backslashes are refused. Other query
data is preserved from the link.
Concord's `Prepared::follow` checks the origin again before execution. There is no native
URL rewriting or separate HTTP implementation. Redirects are returned as HTTP failures.
A malicious host named by a response never receives bearer credentials.

Reaching the cap with more pages returns `PagesError::Bounded`; a page failure returns
`PagesError::Provider`. Neither becomes completion on the next call. `collect_complete`
returns an `Incomplete` containing the collected prefix and the typed error. Dropping a
traversal cancels its owned work. A cyclic link eventually reaches the configured cap;
the adapter does not infer a stable snapshot or deduplicate changing provider results.
Continuation keys belong to their original client/collection/query and are opaque.

To continue after `Bounded` or `Provider`, pass `error.resume()` (the first unread page or
the failed one) as `from` to the matching `*_from` method with the same arguments. The key
is checked again before any request, by the same rules as a provider link: a key from
another collection, droplet, tag or origin is `UnsafeNext` with nothing sent. `size` applies
to `Resume::Start`; a resumed link keeps the page size it was issued with. Keys are
in-memory values only (no serialization), and resuming does not deduplicate records that
moved between pages. After `UnsafeNext` the reported position is the page that carried the
refused link (its records are not in the prefix); resuming fetches that page again, and
the same link fails the same way, so do not retry it in a loop.

For all four `*_from` walkers, use `concord_sdk_digitalocean::concord_pagination::Resume` for
the position. `Resume::Start` repeats the matching collection's first request with `size` and
its tag or droplet ID; `Resume::At(key)` takes the `NextLink` from `links.pages.next` in a
returned page's `next`.

## Failures and diagnostics

Every generated `send()` returns `Result<Response<T>, Error>`. Success bodies are strict
JSON, at most 1 MiB. HTTP failures retain status and metadata plus either a typed
`Problem { id, message, request_id }` or the error-document failure (strict JSON, at most
16 KiB). HTTP status is authoritative when a body is empty, malformed, interrupted,
oversized, of the wrong type, or contradicts the status. No 401, 429 or 5xx triggers replay.

`HttpClass` distinguishes 400, 401, 403, 404, 429, all 5xx, and other non-success statuses.
`Error::Concord` retains build/transport/media/decode/size-limit failures and any received
head. Invalid credentials, fixture origins, page inputs, tag names and unsafe next URLs have
separate static errors. `TransportSetup` reports a client-builder failure before I/O.

Response metadata exposes strict optional numeric `ratelimit-limit`,
`ratelimit-remaining`, `ratelimit-reset` and `retry-after` fields. Missing, repeated,
invalid and overflowing fields are distinct. Reset is a Unix timestamp for expiry of
the oldest counted request, not permission to refill an entire quota. No local limiter
or automatic wait is claimed.

Client/config/scope/request diagnostics redact credentials. SDK errors, error documents,
response wrappers, metadata and pagination keys withhold untrusted text, header values
and followed URLs from their formatting. Explicit model fields, `Problem` fields and
Concord's unredacted native accessors intentionally disclose their data. Do not log those
values or native responses indiscriminately.

## Scope and validation

Excluded: creating/deleting droplets, action execution, billing, metrics ingestion,
polling actions to completion, broad cloud coverage, name/GPU filters, snapshot
restore/deletion, automatic retry/refresh and distributed quota enforcement.

Live provider calls: **NOT_RUN**. All executed workflows use `concord-example-testserver`
on loopback. The production URL is inspected through `prepare()` only. These fixtures
test this SDK's contract, not the real provider's current behavior.

```sh
CARGO_BUILD_JOBS=4 cargo test --locked --offline -p concord-sdk-digitalocean
CARGO_BUILD_JOBS=4 cargo clippy --locked --offline -p concord-sdk-digitalocean --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-digitalocean --no-deps
```

See [FRICTION.md](FRICTION.md) for Concord costs and shared-capability decisions, and
[PROVENANCE.md](PROVENANCE.md) for the example-assisted source/fixture inventory.
