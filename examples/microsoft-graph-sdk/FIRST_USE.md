# First use: concord-sdk-microsoft-graph (independent consumer)

Runner: Claude Opus subagent (requested in the capsule: "Claude Opus subagent"; the harness
reported the model as `claude-opus-5-5`). Held-out wave, first pass against the library frozen
at `47a25fbf` plus the SDK author commit `e80ef9e3`. I did not write this SDK and did not read its
`FRICTION.md`, `PROVENANCE.md`, `src/` or tests unless an entry below says so.

Business outcomes:

- **A.** Sync users through delta and reuse the final link.
- **B.** Enumerate groups and their direct members.

Suite: `examples/microsoft-graph-sdk/tests/first_use.rs`, written from an empty file, against a
fake Graph I wrote on `concord_example_testserver`.

## Fixture provenance

All response bodies in `tests/first_use.rs` are inline and hand-authored from the official
Microsoft Learn pages fetched (as documentation, not API calls) on 2026-11-06:

- `concepts/delta-query-users.md` (git commit `90b4773e`, updated 2026-05-14): user ids, names,
  `$skiptoken`/`$deltatoken` values, the `@removed` reasons and the empty-final-page shape are
  copied from the walkthrough; only the origin is rewritten to the loopback server
  (`https://graph.microsoft.com` becomes `http://127.0.0.1:PORT`).
- `concepts/delta-query-overview.md` (same commit): "a page can't contain both" links, 410 Gone
  with a `Location` header for synchronization reset, `syncStateNotFound` for an expired token
  ("40X-series"), replays.
- `api-reference/v1.0/api/group-list.md` (git commit `11eac1f8`): group ids and the shape of the
  `$select=id,displayName` response; default/maximum page size 100/999 (the paging link itself
  is not shown in the doc, so the `$skiptoken` value on groups is invented).
- `api-reference/v1.0/api/group-list-members.md` (git commit `7aeb175f`): `@odata.context` of
  `directoryObjects`, `$count`/`ConsistencyLevel: eventual` for advanced queries, the limited-
  information behaviour (only `@odata.type` and `id`, other properties `null`). The docs'
  member example is "shortened for readability" and omits `@odata.type`; members with
  `@odata.type` are my assumption from the `directoryObject` collection type. The member
  `$skiptoken` value is invented.
- `concepts/errors.md` (git commit `70f4b4ee`): the `error { code, message, innerError }`
  envelope, Retry-After on 429/503.

Invented or altered (not in the documentation): the incremental round's first page (a replayed
user with `"surname": null` and an invented `$skiptoken`); in the documented "changes" response
the removed id is replaced by Diego Sicilian's id from the initial round so the removal hits a
stored user; all error `code` values (`serviceNotAvailable`, `TooManyRequests`, `resyncRequired`,
`Authorization_RequestDenied`, `Request_ResourceNotFound`, `generalException`) and their messages,
request ids and dates; the 400 status chosen for `syncStateNotFound`; device, contact and
future-type member ids and names; the `Location` value on the 410 (built from the documented
"empty `$deltatoken`" description). The fake does not reproduce Graph's real paging sizes.

The bodies are not vendor conformance evidence. No credentials: the token is
`synthetic-token-not-a-credential`, the context labels are `synthetic-tenant` /
`synthetic-app-permissions-v1`.

## Prediction A (written before the outcome A tests)

Request side:

- `GraphClient::for_loopback(ctx, token, "http://127.0.0.1:PORT/v1.0")` sends nothing.
- `client.users_delta(&[DisplayName, GivenName, Surname])` returns `Listing<UserDelta>` without
  I/O. Its first page is `GET /v1.0/users/delta` with one `$select` whose value is the sorted,
  deduplicated list plus `id` (`displayName,givenName,id,surname`); I expect `$` and `,` to be
  percent-encoded by the URL builder (`%24select=displayName%2CgivenName%2Cid%2Csurname`) but
  will assert on decoded pairs. `authorization: Bearer synthetic-token-not-a-credential`; no
  `ConsistencyLevel`.
- Every later page in the round is a `GET` of the exact `@odata.nextLink` target (no `$select`
  re-added). The next round starts with a `GET` of the exact saved `@odata.deltaLink` target.

Return side:

- `listing.pages().next_page().await` gives `Ok(Some(GraphPage))` per page: `items:
  Vec<UserDelta>`, `next: Some(NextLink)` on intermediate pages, `meta.delta_link: None`.
- The final page of a round has `next: None` and `meta.delta_link: Some(DeltaLink)`, even with an
  empty `value`; the call after it is `Ok(None)` (exhaustion).
- `UserDelta` keeps `Field::Missing` for unselected/omitted properties, `Field::Value` for
  reported ones, `removed: Some(Removed { reason })` for both `deleted` and `changed`.
- `DeltaLink::to_json()` -> string; `client.restore_delta(&json)` -> `DeltaLink` equal to the
  original; `client.resume_users(&restored)` -> listing whose first request is the delta link.

Completion and failure:

- Completion: `Ok(None)` after the page carrying the delta link; `collect_complete()` -> `Ok(pages)`.
- Failed page (503 with Retry-After): `Err(PagesError::Provider { error: GraphError::Provider {
  meta, body: ErrorBody::Parsed(..) }, failed: Resume::At(next_link), .. })`;
  `error.class() == ErrorClass::Service`; the next call is `Err(PagesError::Ended { .. })`, never
  `Ok(None)`. `collect_complete()` -> `Err(Incomplete { collected: prefix, error })` and
  `listing.pages_from(error.resume().cloned().unwrap())` continues from the failed link.
- Expired checkpoint: 410 Gone with `Location`, or a 400 `syncStateNotFound` document ->
  `class() == SyncStateExpired`; the SDK does not follow `Location`, so exactly one request.
- A body cut off mid-page -> `GraphError::Concord { meta: Some(..) }` (no items from that page).
- A page with both links or neither link -> `GraphError::Protocol { problem:
  DeltaLinkCardinality }`.
- A delta link to another origin -> `GraphError::Link { problem: UnsafeDestination }`, no request
  sent to it. A checkpoint restored under another `AuthContext` -> `Link { BindingMismatch }`.

## Prediction B (written before the outcome B tests)

Request side:

- `client.groups(&[GroupField::DisplayName])` -> `Listing<Group>`; first page `GET /v1.0/groups`
  with `$select=displayName,id` (percent-encoded like users), no `$count`, no `ConsistencyLevel`.
  Continuations are the exact `@odata.nextLink` targets.
- `client.direct_members(&group.id, &[MemberField::DisplayName])` -> `Listing<DirectoryObject>`;
  first page `GET /v1.0/groups/{id}/members` with `$count=true` and `$select=displayName,id`, plus
  header `consistencylevel: eventual` on the first page **and** on every next-link request.
  Without a selection: no `$select`, no `$count`, no header.
- The group id is a path segment: a value with `/` is percent-encoded (`a%2Fb`), never a new
  segment. (Unsure: the SDK might instead reject it as configuration.)

Return side:

- Groups: `Group { id, display_name: Some(..), mail/mail_enabled/...: None when not selected,
  additional }`; README pattern `.pages().items()` -> `PageItems` whose `next().await` yields
  `Ok(Some(Group))` until `Ok(None)`.
- Members: `DirectoryObject { id, object_type: "#microsoft.graph.user", display_name, mail,
  additional }`; `kind()` -> `User`, `Group` (nested group, not traversed), `Device`,
  `OrganizationalContact`, `ServicePrincipal`, `Other("#microsoft.graph.futureThing")`. A
  limited-information member (`displayName: null`) stays in the results with `display_name: None`.
- `.items().collect_complete()` -> `Ok(Vec<DirectoryObject>)`; `.items().into_stream()` is a
  `futures` `Stream<Item = Result<DirectoryObject, PagesError<..>>>`.

Completion and failure:

- Completion: `Ok(None)` from `PageItems::next` after the last item of the page without a next
  link; the stream ends with `None` after the last `Ok`.
- A member listing failing with 403 (`Authorization_RequestDenied`) -> `PagesError::Provider` with
  `class() == Permission`; the README aggregate returns that error (no partial aggregate). A
  missing group -> 404 `Request_ResourceNotFound` -> `NotFound`.
- A stream whose second page fails yields page-1 items, then one `Err`, then `None`.
- `max_pages(1)` with a next link -> `PagesError::Bounded { resume: Resume::At(next) }`;
  `pages_from(resume)` continues and reaches exhaustion.
- A member without `@odata.type` -> `Protocol { InvalidObject }`; a groups page carrying a
  `@odata.deltaLink` -> `Protocol { UnexpectedDeltaLink }`; a next link equal to the request URL
  -> `Link { DidNotAdvance }`; a non-JSON 502 body -> `Provider { body: ErrorBody::Malformed }`
  with `class() == Service`; a 302 -> `Provider` with `class() == OtherHttp` and the
  `Location` not followed.

Added after the first outcome B run, before writing its test: a member listing whose second
page fails can be staged (`collected` pages + `error.resume()` as `NextLink::to_json()`) and
resumed in a new client by `direct_members(same group, same fields)?.restore_next(json)` and
`pages_from(Resume::At(..))`; the resumed request is the exact link with `consistencylevel:
eventual` and the refreshed token. Restoring that link on another group's member listing or on the
groups listing is `Link { BindingMismatch }` and sends nothing.

## Trace

1. 08:28Z. First entry point: `examples/microsoft-graph-sdk/README.md`, then
   `cargo doc -p concord-sdk-microsoft-graph --no-deps` (read as text: every public item page),
   `cargo doc -p concord-pagination --no-deps` (`Pages`, `PagesError`, `Incomplete`, `Resume`,
   `PageItems`, `SequentialReport`, `Outcome`, `Progress`), `cargo doc -p concord --no-deps
   --features csv,jsonl` (`ResponseMeta`, `Operation`, `Error`), `crates/concord-pagination/README.md`,
   and `examples/testserver/src/lib.rs` doc comments (it is the dev-dependency test server, not
   the SDK). Provider pages listed above.
2. ~08:33Z. Prediction A written (section above) before any test code.
3. 08:35Z. Wrote the outcome A half of `tests/first_use.rs` (fake Graph, application merge
   store, `read_round` page-staging helper adapted from the README's Workflow A example). First
   `cargo test --locked -p concord-sdk-microsoft-graph --test first_use`: compiled with no
   diagnostics and 9/9 passed on the first run. No source read.
4. 08:37Z. Observation pass (temporary `eprintln!`, removed afterwards) to record wire details:
   first users request target is `/v1.0/users/delta?%24select=displayName%2CgivenName%2Cid%2Csurname`
   (as predicted, `$` and `,` encoded), request headers are `authorization`, `accept:
   application/json`, `host`; continuation targets are byte-identical to the returned links
   (`$skiptoken=` unencoded). A cut-off body is `GraphError::Concord { error: Transport
   ("reading the response body"), meta: Some(200) }` and Display names the operation
   `GraphApi.users_delta`. `restore_delta` of a stored next link is `Link { BindingMismatch }`
   (not `MalformedStorage`). The stored checkpoint JSON is versioned and contains the context
   labels, the base URL (with the loopback port), the selection and the opaque URL; no token.
   All predictions for A held; the resumed delta listing reports its failed page as
   `Resume::Start` (its first request is the delta link), which matches `pages_from` rustdoc.
5. 08:38Z. Prediction B written (section above) before any outcome B code.
6. 08:38Z. Wrote the outcome B half (README Workflow B `memberships()` copied as application
   code, fake directory routes). Distinct compiler diagnostics, all from my own test code:
   - ``error: prefix `user` is unknown`` (x20) and ``expected one of ... found `","id` ``: the
     member fixtures contain `"#microsoft.graph...`, which closes an `r#"..."#` raw string.
     Fixed with `r##"..."##`.
   - ``error[E0599]: no method named `with_status` found for struct `Reply` ``: I assumed a
     test-server builder; `Reply.status` is a public field, used struct update instead.
7. 08:38Z. First B run: 16 passed, 1 failed. Prediction miss: a member without `@odata.type` is
   `GraphError::Concord { error: kind Decode ("the JSON does not match the expected type"),
   meta: Some(200) }`, not `Protocol { InvalidObject }`. Added an empty-string `@odata.type`
   case, which is `Protocol { InvalidObject }`. The case now asserts the observed Decode kind,
   which needs `concord::ErrorKind` (named through the SDK package's own `concord` dependency).
   Second run: 17/17.
8. 08:39Z. Observation pass (temporary `eprintln!`, removed): the odd group id `odd/id ?#` is
   sent as one segment `/v1.0/groups/odd%2Fid%20%3F%23/members?%24select=displayName%2Cid&%24count=true`
   (as predicted, so the test now asserts that branch only). The README aggregate's
   `Box<dyn Error>` displays as `incomplete after 0 collected: a page request failed (after 0
   pages emitted)`. Every other B prediction held: lazy interleaving (groups page 2 is
   requested only after group 2's members), exact next links, `$count=true` +
   `consistencylevel: eventual` on first and continuation member requests, no header or
   `$count` without a selection, `ObjectKind::OrganizationalContact` for `#microsoft.graph.orgContact`,
   `Other(..)` for an unknown type, 302/202/non-JSON 502/`DidNotAdvance`/`UnexpectedDeltaLink`
   typed as predicted.
9. 08:39Z. Added the member-continuation resume test (prediction above): 18/18.
10. 08:40Z. `cargo clippy --locked -p concord-sdk-microsoft-graph --test first_use -- -D
    warnings`: `clippy::result_large_err` on my `read_round(..) -> Result<Round,
    PagesError<GraphError, NextLink>>` ("the `Err`-variant is at least 256 bytes") and
    `clippy::type_complexity` on my case table. Boxed the traversal error in the application
    signature (`type PagesFailure = Box<PagesError<GraphError, NextLink>>`) and added a `Case`
    alias. `rustfmt --edition 2024` on `tests/first_use.rs` only (not `cargo fmt -p`, which would
    touch the SDK's `src/`). Clippy clean; 18/18 three times in a row; the SDK's own suites
    (`protocol` 17, `workflows` 9, doc-tests 3) still pass.

Interventions: none. I did not read the SDK's `src/`, its tests, `FRICTION.md` or
`PROVENANCE.md`, and used no native escape (no reqwest, no hand-built request, no handwritten
operation name). Other provider SDKs under `examples/` were not needed.

## Working path

- Construction: `AuthContext::new(tenant, principal)?` + `GraphClient::for_loopback(ctx, token,
  "http://127.0.0.1:PORT/v1.0")?`; a refreshed token is a new client with the same context.
- A: `client.users_delta(&fields)?` -> `listing.pages()` -> `next_page().await?` until `Ok(None)`,
  staging `page.items` and keeping `page.meta.delta_link` from the final page;
  `checkpoint.to_json()` committed with the merged changes; later
  `client.restore_delta(&json)?` -> `client.resume_users(&checkpoint)?` -> same loop. Large
  rounds: persist `page.next.to_json()`, then `listing.restore_next(&json)?` +
  `listing.pages_from(Resume::At(next))`. Failure: `collect_complete()` -> `Incomplete {
  collected, error }`, `error.resume()` -> `pages_from(..)`; expiry is `error.class() ==
  ErrorClass::SyncStateExpired` and the restart is the application's `users_delta` call.
- B: the README `memberships()` as written; `DirectoryObject::kind()` for the join; for staged
  per-group work, `direct_members(id, &fields)?.pages().collect_complete()` and the same
  `restore_next`/`pages_from` resume.

## Friction (concrete, with reproductions)

1. **The traversal error type is large and unnamed.** Every listing fails with
   `PagesError<GraphError, NextLink>`; the SDK aliases the page and traversal
   (`GraphPage`, `GraphPages`) but not this error. An application function returning it
   unboxed fails `clippy -D warnings` with `result_large_err` (at least 256 bytes). Reproduction:
   `async fn f(l: &Listing<UserDelta>) -> Result<(), PagesError<GraphError, NextLink>> {
   l.pages().next_page().await?; Ok(()) }` under `cargo clippy -- -D warnings`. Workaround in
   the suite: `type PagesFailure = Box<PagesError<GraphError, NextLink>>` and `.map_err(Box::new)?`.
2. **Reaching the `GraphError` takes a pattern match every time.** `PagesError` exposes
   `outcome()`, `resume()` and `progress()` but no accessor for the provider error, and its
   variants are `#[non_exhaustive]`, so each call site writes `let PagesError::Provider { error,
   .. } = &e else { .. }` before it can ask `error.class()`. The suite does this 12 times; the
   question an application asks most often ("is this 429/410/403?") is two levels down.
3. **The README Workflow B aggregate loses the failure's type.** It returns `Box<dyn Error>`;
   when a member listing fails with 403, the error displays only `incomplete after 0 collected:
   a page request failed (after 0 pages emitted)`: no class, no group. To recover the class the
   application must downcast to the exact `Incomplete<DirectoryObject, GraphError, NextLink>`
   (a `pagination` type), and a failing groups page would instead be a
   `PagesError<GraphError, NextLink>`: two different downcast targets for one function.
   Reproduction: `b_a_failing_member_listing_fails_the_aggregate_with_a_typed_error`.
4. **A member without `@odata.type` is a decode failure, an empty one is a protocol failure.**
   The `ProtocolProblem::InvalidObject` rustdoc ("An item has an empty identifier or required
   directory type") led me to predict `InvalidObject` for a missing type; the observed result is
   `GraphError::Concord` with `ErrorKind::Decode`, while `"@odata.type": ""` is `InvalidObject`.
   Both are `ErrorClass::Local` and both drop the whole page. Distinguishing the decode case
   requires naming `concord::ErrorKind`, which the SDK does not re-export (it re-exports only
   `pagination`), so an application needs its own `concord` dependency. Note that the members
   doc example (marked "shortened for readability") omits `@odata.type`; a fake built verbatim
   from it fails every page. Reproduction: the first two cases of
   `b_contract_violations_and_unusable_responses_are_typed`.
5. Minor, documented and kept as an observation: delta completion is assembled by the
   application. `pages().collect_complete()` returns pages and the checkpoint has to be taken
   from the last page's `meta.delta_link`; `.items()` drops it. The README's `read_round` helper
   is the pattern every consumer copies; it worked unchanged.

No friction found in: request shapes, link exactness, header retention on continuations,
context/selection binding of stored links, redaction in `Display`/`Debug`, 410/`syncStateNotFound`
classification without automatic reset, the stream's error-before-end behaviour, and the page
cap. Outcome A compiled and passed on the first attempt.
