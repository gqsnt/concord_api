# Microsoft Graph SDK

`concord-sdk-microsoft-graph` is an unpublished, bounded Graph **v1.0** client built
with `concord::api!`. It provides users delta, group listing, and direct group
members, with typed records, selected fields, opaque continuations, and errors
that retain response metadata. All verification is offline; the hand-authored
fixtures are **not vendor conformance evidence**.

It excludes directory mutations, transitive traversal, group delta, beta APIs,
advanced filters/search/casts, token acquisition, and automatic synchronization
reset. Production construction supports the global Graph origin only; sovereign
cloud endpoints are outside this slice.

## Construction and authorization

```rust
use concord_sdk_microsoft_graph::{AuthContext, GraphClient, GraphError};

fn example_client() -> Result<GraphClient, GraphError> {
    let context = AuthContext::new("synthetic-tenant", "synthetic-app-permissions-v1")?;
    GraphClient::new(context, "synthetic-token-not-a-credential")
}
```

Construction sends nothing. A real application supplies a token obtained by its
own authorized authentication flow. The context labels identify the tenant and
principal/permission profile; they **do not verify token claims**. Keep these
labels accurate, change the profile label when its authorization scope changes,
and partition cached users, memberships and saved links by the same context.
Token refresh under the same context can reuse a checkpoint.

The SDK owns its reqwest client: redirects are disabled, requests have a 30-second
timeout and connections a 10-second timeout. Credentials are marked sensitive.
Only `https://graph.microsoft.com/v1.0` is used in production.
`GraphClient::for_loopback(context, token, "http://127.0.0.1:PORT/v1.0")` accepts
literal loopback IPs for synthetic tests, disables proxies there, and never accepts
an arbitrary HTTP host. Caller-supplied transport and timeout customization are
not exposed in this slice.

Followed links must be absolute, have the same origin and collection path, and
contain no user information, fragment, whitespace or backslash. The fully qualified
`users/microsoft.graph.delta` path is also accepted for users delta. Query tokens,
ordering, duplicates and percent escapes are retained: the SDK never extracts a
skip/delta token or reattaches `$select`. Returned next links, stored links and
final delta links all receive the same destination checks. A same-origin link to
another collection is refused. HTTP redirects are returned as errors.

## Workflow A: enumerate users, commit, and later sync

Graph distinguishes a next page from a completed delta round, including when a
page is empty. Selected properties define the tracked changes. See the
[delta overview](https://learn.microsoft.com/en-us/graph/delta-query-overview) and
[users delta walkthrough](https://learn.microsoft.com/en-us/graph/delta-query-users).

```rust,no_run
use concord_sdk_microsoft_graph::{DeltaLink, GraphClient, Listing, UserDelta, UserField};

async fn read_round(
    listing: Listing<UserDelta>,
) -> Result<(Vec<UserDelta>, DeltaLink), Box<dyn std::error::Error>> {
    // This small example collects a complete bounded round. On error, the returned
    // Incomplete contains the page prefix and failed/unread continuation.
    let pages = listing.pages().collect_complete().await?;
    let mut changes = Vec::new();
    let mut checkpoint = None;
    for page in pages {
        changes.extend(page.items);
        if let Some(delta) = page.meta.delta_link {
            checkpoint = Some(delta);
        }
    }
    Ok((changes, checkpoint.ok_or("completed round had no delta checkpoint")?))
}

async fn initial_and_later(
    client: &GraphClient,
) -> Result<(), Box<dyn std::error::Error>> {
    let (initial_changes, checkpoint) = read_round(client.users_delta(&[
        UserField::DisplayName, UserField::Mail,
    ])?).await?;
    let saved_checkpoint = checkpoint.to_json();
    // Application: merge initial_changes and atomically persist saved_checkpoint.
    // Do not publish a checkpoint before its associated changes are committed.
    let _ = initial_changes;

    // Later, possibly after rebuilding a client with a refreshed token:
    let restored = client.restore_delta(&saved_checkpoint)?;
    let (new_changes, next_checkpoint) = read_round(client.resume_users(&restored)?).await?;
    // Application: merge new_changes and commit next_checkpoint.to_json() together.
    let _ = (new_changes, next_checkpoint);
    Ok(())
}
```

For large rounds, call `pages.next_page().await` and stage each page durably instead
of collecting. Only final-page `page.meta.delta_link` is a completed checkpoint.
Use the page interface for delta: `.items()` intentionally drops page metadata.
Do not derive exhaustion from the number of items or treat a next link as a final
checkpoint.

Consecutive empty pages can occur in either an initial or incremental round. Each
counts toward the page cap. If a cap or failure occurs after an empty page, retain
the staged changes and resume with the error's next-page link; an empty page does
not authorize committing a completed delta checkpoint unless it has a delta link.

`UserDelta` is a sparse change, not a full replacement user. `Field::Missing`
means retain a prior property, `Field::Null` clears it, and `Field::Value` supplies
the new value. Key application merges by `id`, handle replayed changes, and apply
`@removed` markers. Both `changed` and `deleted` removal reasons are preserved,
as are unknown reasons and unmodeled user properties. A collected round is not
an automatic deduplicated or merged directory snapshot. `$select` values are
typed, deduplicated and sorted; nonempty selections include `id`. An empty list
omits `$select`. Resuming a delta reuses its saved selection without modification.

## Workflow B: groups and their direct members

[Group listing](https://learn.microsoft.com/en-us/graph/api/group-list?view=graph-rest-1.0)
and [direct members](https://learn.microsoft.com/en-us/graph/api/group-list-members?view=graph-rest-1.0)
are separate paged operations. This example joins only typed user members to an
application's accessible-user index from the same authorization context, while
retaining every returned member object.

```rust,no_run
use std::collections::BTreeSet;
use concord_sdk_microsoft_graph::{
    DirectoryObject, GraphClient, Group, GroupField, MemberField, ObjectKind,
};

struct GroupMembership {
    group: Group,
    members: Vec<DirectoryObject>,
    accessible_user_ids: Vec<String>,
}

async fn memberships(
    client: &GraphClient,
    active_accessible_users: &BTreeSet<String>,
) -> Result<Vec<GroupMembership>, Box<dyn std::error::Error>> {
    let mut output = Vec::new();
    let mut groups = client.groups(&[GroupField::DisplayName])?.pages().items();
    while let Some(group) = groups.next().await? {
        let members = client.direct_members(&group.id, &[MemberField::DisplayName])?
            .pages().items().collect_complete().await?;
        let accessible_user_ids = members.iter()
            .filter(|m| m.kind() == ObjectKind::User && active_accessible_users.contains(&m.id))
            .map(|m| m.id.clone()).collect();
        output.push(GroupMembership { group, members, accessible_user_ids });
    }
    Ok(output)
}
```

The group example returns no successful aggregate when an inner member listing
fails; applications needing resumable cross-group work should stage each group
and its per-listing continuation. The SDK does not promise an atomic directory
snapshot across groups and members.

`DirectoryObject` preserves the exact `@odata.type`, id and additional properties.
`kind()` recognizes user, group, device, organizational contact and service
principal, and exposes unknown strings as `Other`. Null/missing names do not prove
inaccessibility: selection, permissions and actual data can all cause them. An
id/type-only object remains in the results. Nested groups are returned without
traversal. Selected member requests include `$count=true` and
`ConsistencyLevel: eventual`; continuation calls retain the header.

Unknown types also remain in the results when only id/type or null profile fields
are supplied. Inspect `object_type` or `ObjectKind::Other(raw_type)` and retain the
object while following its page's next link. Classify members by type rather than
requiring a name or mail address; additional null properties remain in `additional`.

Permissions and tenant roles determine visibility; use the official endpoint
permission tables for the application's authorization flow. Hidden memberships
require additional permission. The v1.0 members endpoint documents a **service
principal omission**, and group listing excludes dynamic distribution groups.
No beta/expand fallback is attempted. Exhaustion means every returned page was
read, **not complete or transitive directory membership**. Replication delays also
apply.

## Errors, bounds, and partial outcomes

`GraphError` distinguishes configuration, continuation, protocol, Concord, and
provider failures. Provider failures expose the parsed code/message, nested
`innerError`/`innererror` details, actual status, all headers (including request IDs,
`Retry-After` and `Location`) and generated operation identity. `class()` identifies
400, 401, 403, 404, 429, 5xx and expired sync state. Unknown codes remain available.
See [Graph error shapes](https://learn.microsoft.com/en-us/graph/errors).

A malformed error document is `ErrorBody::Malformed`; an oversized or interrupted
one is `Unavailable` with the Concord error. Both retain the HTTP response head.
Success decoding and protocol failures also retain their head. No status is
fabricated, and 410 or `syncStateNotFound` never triggers a reset or follows
`Location`. Retry/backoff, refresh, restart approval and durable storage belong
to the application. There are no automatic SDK retries.

Each listing has a default 1000-page cap, adjustable with
`.pages().max_pages(NonZeroUsize::new(limit).unwrap())`. Empty pages count.
`Ok(None)` means provider exhaustion; a cap returns `PagesError::Bounded`, a failed
page returns `Provider`, and subsequent calls after either error return `Ended`.
`collect_complete()` retains the emitted prefix in `Incomplete.collected` on
failure. Its error retains the failed or unread `Resume<NextLink>`, which can be
passed to the same listing's `pages_from`. `NextLink::to_json()` and
`listing.restore_next()` support durable page-boundary resume; preserve the
associated staged prefix too. These bindings prevent accidental context/selection
mixing, not malicious modification or mixing application staging from separate
rounds of the same query. They are not signed capabilities.

The `.items().into_stream()` adapter implements standard Rust `Stream` and emits
an error before ending. A stream's `None` after an error does not imply completion.
Canceling a pending `next_page()` keeps its request in flight for the next call;
dropping the traversal cancels owned work without detached requests. Calling
`finish()` distinguishes interruption, bounds, failure and exhaustion. Direct
`fetch()` futures do not retain canceled work and should not be used as a
cancellation-safe traversal substitute.

JSON bodies are finite and buffered **one page at a time**, not parsed as an
incremental JSON event stream. Defaults cap success bodies at 4 MiB and error
bodies at 64 KiB; `with_body_limits` changes these before constructing listings.
Collections additionally accumulate requested results in application memory.
Immediate next-link loops fail; longer cycles are bounded by the page cap.

SDK diagnostics redact credentials, stored links and provider error values.
Explicit `url()`, `to_json()`, model data, provider fields and metadata headers
are unredacted; protect them in application storage and logs.

## Reproducible verification

From the workspace root, use the prescribed shared cache for every cargo command:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-microsoft-graph
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo clippy --locked -p concord-sdk-microsoft-graph --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo fmt -p concord-sdk-microsoft-graph
```

Tests use only `examples/testserver` and synthetic credentials. Fixtures and source
versions are documented in `PROVENANCE.md`; author observations are in
`FRICTION.md`. Focused wire regressions in `tests/maintenance.rs` cover consecutive
empty delta pages, caps and failed continuations, and limited/unknown member types.
The maintenance scope and change counts are recorded in `MAINTENANCE.md`.
The README's Rust examples are compiled as crate documentation tests.
