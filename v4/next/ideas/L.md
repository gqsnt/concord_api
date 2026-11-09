# L. Declaration syntax and conventions — merged ideas
Sources: HKL-claude.md (§ L1 to L4), HKL-codex.md (§ L1 to L3). 7 lane designs → 5 entries: 2 KEEP, 3 KILL, 0 MERGED.

Owner direction: family L removes little glue, so it keeps at most the 1–2 ideas that reduce declaration lines without hurting rule C4. Both lanes agree that L removes no SDK glue. Its only measurable effect is declaration lines (R4a: 2,079 lines over 316 endpoints = 6.58, checked with `tools/glue/scoreboard.py`).

## L1. One line per short endpoint: layout, no language change — KEEP (rank 1)
Sources: HKL-claude.md § L1; IDEAS.md L seed 3; friction L7 (rustfmt does not format `api!` bodies).
Fact: none new.
Mechanism: the AUTHOR_GUIDE and CHEATSHEET show the one-line body first. Optionally, a small check over `tools/glue/decl.py` flags a short body spread over several lines when it fits in 100 columns. No macro change.
Declaration: `mastodon-sdk/src/api.rs:71-79`.
```rust
// before
GET status(id: StatusId) -> Status {
    path "api/v1/statuses/{id}";
}
// after
GET status(id: StatusId) -> Status { path "api/v1/statuses/{id}"; }
```
Call site: unchanged.
Glue removed: 0 glue. The lane claims −87 declaration lines (40 of 85 bodies fit). I could not reproduce that. My recount over `examples/*-sdk/src` finds 81 multi-line bodies holding only `path`, `request` or `response`:
- 12 fit in 100 columns counting indentation (−27 lines);
- 28 fit if indentation is ignored (−66 lines).

R4a falls by 0.09 to 0.21 lines per endpoint.
Counterexample: long signatures (FRED `search` at 129–134 columns, the Registry) stay multi-line.
Simple control: unchanged (Nager's one-line form would be 121 columns).
New concepts: none.  Breaking: none.
Shares mechanism with: L2 (an inline path lets ≈9 more bodies fit).
Screen:
- C1, C2, C4 and C6: untouched.
- C3: the controls are unchanged.
- METHOD (no new instruments): the check is optional and small; the guide alone suffices.

## L2. Inline path for a path-only endpoint — KEEP (rank 2)
Sources: HKL-codex.md § L1 (`-> T at "p";`); HKL-claude.md § L2 (`GET("p") name(..)`); IDEAS.md L seed 1.
Fact: the endpoint's path, written on the endpoint's line.
Mechanism: an additive short form that lowers to a body holding only `path`. The block form stays for every other statement, and a missing path still means "the owner's URL". Variants:
- a suffix, `-> T at "p";` (codex);
- the method slot, `GET("p") name(..) -> T;` (claude), mirroring `query("key") value`. If the `path` statement were retired, the controls would change form (C3), so only the additive variant is kept.
Declaration: `mastodon-sdk/src/api.rs:51-53, 71-79`.
```rust
GET public_timeline(..TimelineOptions) -> Vec<Status> at "api/v1/timelines/public";
GET status(id: StatusId) -> Status at "api/v1/statuses/{id}";
// or, method-slot variant:
GET("api/v1/statuses/{id}") status(id: StatusId) -> Status;
```
Call site: unchanged.
Glue removed: 0 glue. Beyond L1 it saves ≈7 characters on each path-only body (153 bodies, lane count), and ≈9 more multi-line bodies then fit in 100 columns (−18 lines, my recount). The 104 declaration doc lines that restate the route (lane) become plainly redundant, as they do with H1.
Counterexample: MediaWiki's four pathless endpoints stay `GET search(…)`: with no `at` (or no parentheses after the method), the endpoint still means the owner's URL. Trailing slashes stay literal (`at "v2/"`, PokeAPI's `pokemon/{id}/`).
Simple control: unchanged in the additive variant.
New concepts: `at "p"` (or `METHOD("p")`).  Breaking: none (additive).
Shares mechanism with: H1 (the route visible on the line), L1.
Screen:
- C1: the path is a provider fact.
- C3: the additive variant leaves the controls unchanged.
- C4: the path is explicit, and easier to find.
- C5: both forms on one endpoint is an error at the second one.
- C6: two spellings for one fact. This is the cost the owner weighs.

## L3. Formats written on the value (`-> Vec<u8> as Binary`) — KILL (removes almost nothing; C6)
Sources: HKL-claude.md § L3.
Design: the 64 endpoint-level `request` and `response` statements move onto the type. That saves ≈30 lines at 100 columns, overlapping L1, and no glue. Formats would then live in two places (statement defaults and `as`), and `as` reads like a Rust cast and collides with the stream-reader `as` (H4).

## L4. Path from the endpoint's name (an implicit default, or `@name`) — KILL (C4; the explicit variant removes almost nothing)
Sources: HKL-claude.md § L4; HKL-codex.md § L2; IDEAS.md L seed 2.
- (a) Implicit "path = name" breaks C4. 90 of the 316 endpoints have no path and mean the owner's URL (MediaWiki 4 of 4, FRED `metadata`, Shopify's GraphQL endpoints). A default moves them all, or needs `path ""` everywhere. The related "an unplaced value is a query parameter" removes the "value is never used" error, which catches a forgotten hole (C5).
- (b) Explicit `GET @pods(..)` applies to ≈42 endpoints (checked; the lane says 46). Each saves ≈10 characters and no lines, and renaming the Rust method then changes the URL.

## L5. One route with named representation rows (FRED observations) — KILL (removes almost nothing; C6)
Sources: HKL-codex.md § L3.
Design: `GET "observations" (inputs) { observations -> …; observations_json -> … { … }; export_csv -> … { … } }`. It removes 2 repeated input lists and 2 path facts in one SDK, while the total line count may grow (the lane's own words). It is a fourth grouping construct, next to scopes, parameter groups and blocks. The lane notes that FRED's own friction log accepts separate methods.
