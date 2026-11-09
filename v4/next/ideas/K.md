# K. Domain facts and request composition — merged ideas
Sources: HKL-claude.md (§ K1 to K5; § H3 is merged into K2), HKL-codex.md (§ K1 to K3). 8 lane designs → 6 entries: 3 KEEP, 2 KILL, 1 MERGED.

## K1. Declared ranges on values (`query limit?: u8 in 1..=40`) — KEEP (rank 1)
Sources: HKL-claude.md § K4.
Fact: a documented bound on an input. Page sizes: Mastodon 1..=40, FRED 1..=1000, Notion 1..=100, Intercom 1..=150, GitHub 1..=100, and so on.
Mechanism: `in a..=b` after a value's type, with constant bounds and any `PartialOrd` type. Outside the range, `prepare()` is a `Build` error naming the value, the bound and the endpoint, and nothing is sent. On an `each` value the bound applies to each item.
Declaration: `mastodon-sdk/src/api.rs:45-46`, plus the type at `model.rs:265-308`.
```rust
// before: plus TimelineLimit, its MAX, new, get, Display and InvalidLimit (37 non-blank lines)
/// `limit`: at most this many statuses (1 to 40; the server's default is 20).
query limit?: TimelineLimit,
// after
/// At most this many statuses (the server's default is 20).
query limit?: u8 in 1..=40,
```
Call site: `.set_limit(TimelineLimit::new(40)?)` → `.set_limit(40)`. The failure moves from construction to `prepare()` or `send()`.
Glue removed: the lane says 400–600 lines in ≈24 SDKs. Checked:
- Mastodon `model.rs:265-308` (37), FRED `lib.rs:149-184` (36), Notion `params.rs:5-40` (31) and Intercom `models.rs:7-30` (21): 125 lines.
- `(1..=N).contains` appears 38 times in 23 SDK crates. About 5 of those bound a length (PayPal ids, S3 bucket names), which a value bound does not cover. About 7 are 3-line inline checks inside wrappers. A typical newtype is 25–30 lines (Discord `MessageLimit`, Stripe `Limit`).

Corrected estimate: ≈300–450 lines. The classifier labels these lines model or behavior, not glue. By the charter's C2 definition (a provider fact restated by hand) they are glue.
Counterexample:
- MediaWiki `srlimit`: the maximum depends on the caller's rights (500, or 5000 for bots). Kubernetes `limit` has no documented maximum. Both stay undeclared.
- Mastodon's 40 may be an instance setting: an instance that raises it would see valid requests refused.
- An SDK that prefers to let the server answer simply declares no bound.
Simple control: unchanged.
New concepts: `in <range>`.  Breaking: none in Concord. Adopting SDKs change setter types (callers drop `X::new(..)?`) and lose typed range-error variants.
Shares mechanism with: family I (values and inputs; likely the same idea appears there and should be merged), A (range variants such as FRED `error.rs:47` and Intercom `Error::PageSize`), F (pagination reads the declared maximum page size), H1 (the line and the setter show `limit (1..=40)`).
Screen:
- C1: a documented fact.
- C2: a pre-send check like the empty-segment rule, not behavior.
- C4: visible on the value.
- C5: the error names the value, the bound and the endpoint, but it fails at `send()`, not at the setter: later than a newtype's constructor.
- C8: deterministic.
- C9: wire test that 41 is refused before I/O, and a canary that a sensitive value's bound error does not print it.

## K2. Typed route and key facts (`Route`, `ROUTES`, `request_facts()`) — KEEP (rank 2)
Sources: HKL-codex.md § K3; HKL-claude.md § H3 (merged from H).
Fact: each endpoint's method, route template (`api/v1/statuses/{id}`, with literal query selectors) and named query keys. These are static and contain no values.
Mechanism: constants generated from the resolved model:
- per endpoint, `ROUTE` / `request_facts()`: method, template, and whether the path is literal or a template;
- per client, `ROUTES` (navigation, route, output) and `declared_query_keys()`;
- in core, `Operation::route()`;
- one route table on each client's rustdoc.

The variants differ in scope: claude adds the table and `Route::matches(&Url)`; codex adds the query-key vocabulary and the literal path.
Declaration: unchanged. SDK before (`mediawiki-action-api-sdk/src/models.rs:33-55`, a hand-kept list of 14 keys) → after:
```rust
if key.is_empty() || matches!(key.as_str(), "action" | "format" | /* 12 more */ "cmlimit") {
    return Err(D::Error::custom("continuation collides with a fixed query input"));
}
// after
if key.is_empty() || crate::MediaWiki::declared_query_keys().contains(key.as_str()) { /* same */ }
```
Call site:
```rust
let route = err.operation().and_then(|o| o.route()).map(Route::template);  // secret-free
tracing::warn!(url.template = route, "request failed");
```
Glue removed: ≈40 lines directly. Checked:
- MediaWiki `models.rs:33-55` (23 non-blank lines, ≈16 saved);
- Mastodon's hand-written operation table, `lib.rs:40-50` (11);
- PokeAPI's path check, `lib.rs:247-261` (15, with `matches`; shared with G2);
- OpenAI's route literals at `lib.rs:109` and `159` (2, replaced; no net saving).

The 120 `glue:docs` lines could link the table instead, but H1 already makes them redundant.
Counterexample:
- Mastodon's streaming host and Registry's `Location`: the route names the declared request, not the URL a policy actually used.
- A key set must never be used to serialize or document query order.
- A route string invites `if route == "…"` dispatch, which `operations Name;` does better with an exhaustive `match`.
Simple control: declaration unchanged. `NagerHolidays::ROUTES` has one row.
New concepts: `concord::Route`, `ROUTE`/`ROUTES`, `Operation::route()`, `declared_query_keys()`.  Breaking: none (`EndpointMeta` is `__private`). Generated accessor names can collide with endpoint names; diagnose both.
Shares mechanism with:
- H1: the same resolved facts. The table is H's route index.
- G2: `matches` is the path-only subset of `from_url`.
- J: `url.template` as a secret-free span attribute.
- E: Discord keys its limiter on `RouteKey { name, method }` (`discord-rest-sdk/src/limiter.rs:78-84`; Discord's buckets are really per route template).
- M: a snapshot test of `ROUTES`.
- F: a link can be checked against its route.
Screen:
- C1: no words added.
- C2: typed access to the facts.
- C4: the route is the declared path.
- C8: constants.
- C9: an oracle test that `ROUTES` matches `prepare()`'s path with holes substituted, and a canary that no route holds a planted secret.

## K3. Values that span several path segments (`{each name}`) — KEEP (rank 3)
Sources: HKL-claude.md § K3; friction K3 (amazon-s3, distribution-registry, open-library).
Fact: a provider identifier can span several path segments, each escaped on its own: a Docker repository name `acme/tools/app`, an S3 object key `photos/Jan/sample.jpg`.
Mechanism: `{each x}` in a path sends one segment per item of `x` (`IntoIterator` of `Display` items, as `each` already works for query and headers). An empty, `.` or `..` item, or no item at all, is a `Build` error.
Declaration: `distribution-registry-sdk/src/api.rs:32-106`.
```rust
// before: a second client `Repository` with base "https://registry.invalid/v2/repository",
//         rebased per repository with with_base_url
scope repository(name: RepositoryName) {        // after: a scope of the one client
    path "v2/{each name}";
    GET tags(query n?: PageSize, query last?: String) -> TagsBody { path "tags/list"; response Json; }
    /* the same eight endpoints */
}
```
Call site: `let repo = registry.repository(&name)?;` (fallible: it builds a base URL) → `let repo = registry.repository(&name);`.
Glue removed: the lane says 25–30 lines in Registry. Checked: `client.rs:110-124` (15 → ≈5), `api.rs:51-58` (7, the second client's header) and `api.rs:7-17` (11, the "three clients" explanation): ≈28 lines.
Correction: the lane lists S3 object keys as a case that must stay one segment (`%2F`). S3's own FRICTION §2 says the opposite: SigV4 expects `/` raw in the key. The SDK turns `%2F` back into `/` after `into_parts` (`amazon-s3-sdk/src/sigv4.rs:202-206`) and forbids an endpoint path of its own for that reason. So S3 is a second witness, for keys without empty or dot components; S3 allows `a//b`, and such keys keep the native path.
Counterexample: GitLab project paths must stay one segment (`%2F`). `each` is opt-in per hole, so `{key}` is unchanged.
Simple control: unchanged.
New concepts: `each` inside path templates.  Breaking: none.
Shares mechanism with: C2 (one client less that rebuilds a base per value with `with_base_url`), G1 (Registry's `Repository` wrapper keeps only its token role).
Screen:
- C1: a fact about the identifier's shape.
- C4: visible at the hole.
- C5: the error names the item's position, not a sensitive value.
- C6: one sentence, reusing `each`.
- C9: a wire test with `acme/tools/app`, and refusal of `..` and empty items before I/O.

## K4. A shared prefix or shared statements without navigation (`path "api/v1" { … }`, `prefix`, `with { … }`) — KILL (removes almost nothing for a new concept)
Sources: HKL-claude.md § K1 and § K5; HKL-codex.md § K1; IDEAS.md K seed 1.
Variants:
- (a) a `path "p" { … }` block whose holes read the enclosing owner (claude K1);
- (b) `prefix "p" { … }`, literal only (codex K1);
- (c) `with { query …; response …; <endpoints> }` (claude K5). It sends the `with` query keys before the endpoint's parameters: FRED's `file_type=json` moves first, a wire change.

Screen: it removes 0 lines of SDK glue. Mastodon's 6 `api/v1` literals become 1 at a cost of 2 more lines (codex count). The 18 version literals in endpoint paths (D2a, checked with the scoreboard) fall to ≈8. Variant (c) saves ≈15 declaration lines in 4 SDKs. Kubernetes's context wrapper goes just as well with today's `{scope.value}` references (G4), which keep the group navigation and avoid the clash a flat namespace scope would hit on `events` (core and `events.k8s.io` both have one).
Recorded limit: friction K5 (Datadog: mixing v1 and v2 forces the version into every path) and friction L1 (8 SDKs repeat statements on sibling endpoints) stay open.

## K5. Fixed query literals inside parameter groups (`query("watch") = "1"`) — KILL (removes almost nothing; C6)
Sources: HKL-codex.md § K2.
Design: a nameless literal entry in `params`, spliced where it is written, so Kubernetes keeps `…&allowWatchBookmarks=true&watch=1`. It saves 5 declaration lines in Kubernetes (6 `"watch" = "1"` blocks, checked) and 3 repeated facts in MediaWiki (possibly with a line added), and removes no glue. It widens `params` from "a set of setters" to "setters and constants", the risk the lane names itself.

## K6. Query order is declaration order: keep the rule, delete its explanation — MERGED into H1
Sources: HKL-claude.md § K2; HKL-codex.md § Selection; IDEAS.md K seed 2.
Checked: no `query` key is appended, overridden or removed across owners in the 55 SDKs. The only cross-owner edits are Stripe's 19 `override "stripe-account"` headers. So for every SDK request, the wire order already equals reading order. Both lanes reject a semantic change (codex: the demo's `tag=a&tag=b&tag=c&v=2&limit=10` would change), and the "literal pairs first" alternative would move up to 36 endpoints in 15 SDKs. What remains belongs to H1: delete the ≈60-word slot paragraph (`emit.rs:1211`, checked), show the order on the request line, and restate the rule as one sentence in AUTHOR_GUIDE §4.7.
