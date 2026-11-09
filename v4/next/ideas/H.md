# H. Documentation: short, from the declaration — merged ideas
Sources: HKL-claude.md (§ H1 to H4), HKL-codex.md (§ H1 to H3). 7 lane designs → 4 entries: 1 KEEP, 2 KILL, 1 MERGED. K6 (query-order wording, from the K family) is also merged into H1.

Owner decision for this phase: the generated docs are fixed, with a hard word budget per endpoint. H1 is that design.

## H1. The request line under a hard word budget — KEEP (rank 1)
Sources: HKL-claude.md § H1 and § H4 (setter lines); HKL-codex.md § H1; IDEAS.md H seeds 1 and 3; K6 merged here.
Fact: what an endpoint puts on the wire, already resolved from the declaration: method, URL template, query keys in wire order, header names, sensitivity, body and output formats, and the `send()` result.
Mechanism: the per-endpoint prose (`concord-macros/src/emit.rs:999-1642`) is replaced by the author's first paragraph plus 2–3 fact lines. The composition rules (escape set, query slots, `Accept`/`Content-Type` provenance, policy bypass, transport defaults) move to one shared `concord::api!` page, linked once. The budget:
- at most 150 generated words over an endpoint's navigation method, request type and `send` docs together (codex wording);
- enforced in CI by `tools/glue/rustdoc_metrics.py` (R4c, which exists);
- author prose counted separately and never truncated.

Each setter gets one line: "Sends `local=<value>` in the query." (claude H4).
Declaration: unchanged. The authors' mechanics notes become deletable (`nager-holidays-sdk/src/lib.rs:18-21, 30-34`):
```rust
/// Client for the unauthenticated Nager.Holidays Community v4 holiday endpoint.
pub client NagerHolidays() {     // the 4 lines on with_base_url / KeepHead / prepare() go
```
Generated page, Mastodon `ApiStatus` (≈330 words today, lane estimate from `emit.rs:1215-1548`):
```text
One status.
GET {base}/api/v1/statuses/{id}
AUTHORIZATION: <sensitive> · accept: application/json (from Json)
send() → Result<Status, Error> via MastodonPolicy · body ≤ 8 MiB · Requests: concord::api! § Requests
```
Call site: unchanged.
Glue removed: generated words per endpoint fall from 888.7 to ≤ 150 (checked: `v4/desirability/SUMMARY.md:57` gives 543.8 → 888.7). SDK doc lines become redundant:
- 81 lines explaining Concord mechanics (`decl_doc_mech`, checked with `tools/glue/scoreboard.py` at `ba10189b`);
- 104 declaration doc lines restating the route (lane count);
- 58 value docs that begin with their own wire key (lane).

The glue inventory counts only 120 lines as `glue:docs` (`GLUE.draft.md:15`), so the effect on SDK glue is small. The main effect is the generated text. On Concord's side, the doc emitter (`emit.rs:999-1642`, ≈640 lines) becomes a renderer of ≈150 lines (lane).
Counterexample:
- MediaWiki: four endpoints share one URL and differ by literals and a `query(..)` value. The line shows that without a rule: `GET {base}?action=query&format=json&formatversion=2&srsearch&srlimit=10&{continuation: Continuation}&list=search&srprop=`.
- Mastodon's placeholder base never appears (`{base}`); its real host needs C2.
- A policy that rewrites the URL (QuickBooks) is labeled "prepared request", not described as the final send.
Simple control: Nager's declaration keeps its shape (it may lose 9 mechanics doc lines). Its page becomes the author's doc plus three lines.
New concepts: none for authors or callers; one shared page and a CI word gate.  Breaking: doc text, and Concord's doc-text tests (`emit.rs:3411-4183`, the `request_summary_*` and `*_docs_*` tests) get rewritten.
Shares mechanism with: K2 (the route table and the line read the same resolved facts), K6 (the query-order paragraph goes), A, B, E and F (each later fact adds one slot to the line instead of a paragraph), L2 (an inline path also makes route restatements redundant).
Screen:
- C1: nothing added to any declaration.
- C4: the URL and the query order are visible on every endpoint page.
- C6: this is the point of the design: ≤ 150 words, and the rules on one page.
- C7: `prepare()`, `into_parts()` and `send_raw()` are explained once, on the shared page.
- C9: the word gate, plus the composition oracle (`tests-gen/oracle_composition.py`, which exists) checking each line against `prepare()`.
- Risk: H16s measured model readers doing better with the long prose. Edge answers move one click away.

## H2. The declaration excerpt is the doc — KILL (C6; no per-endpoint budget can hold)
Sources: HKL-claude.md § H2; HKL-codex.md § H2; IDEAS.md H seed 2.
Design: each endpoint page prints its resolved declaration chain (codex adds a composed method/URL line). Callers are not authors. To read the excerpt they must learn `each … else inherit` (Stripe uses it 19 times), `..Group` and `response F`: documentation of the DSL in order to read the docs (C6). The excerpt grows with chain depth and group size (Kubernetes `watch_pods`: ≈15 source lines before any prose), so the owner's fixed per-endpoint budget cannot hold. The one part that helps, the resolved method/URL line, is H1.

## H3. Routes as typed facts (one route table per client) — MERGED into K2
Sources: HKL-claude.md § H3.
Design: generated, value-free route constants. This is the same mechanism as codex's `RequestFacts` (HKL-codex.md § K3). Its documentation use, one generated route table per client from `ROUTES` replacing hand-written operation tables such as `mastodon-sdk/src/lib.rs:40-50` (11 lines, checked), is listed in K2.

## H4. Name the stream reader (`-> stream Item as Reader`) — KILL (removes almost nothing; C1)
Sources: HKL-codex.md § H3.
Design: an optional assertion that `Incremental<Item>::Records` is `Reader`, shown in the signatures. It removes ≈6 comment lines (Mastodon `streaming.rs:211-215`, checked; Kubernetes `watch.rs:31-33`) for new syntax. It restates an association already written in Rust (`PublicStreamFormat::Records = PublicStream`, `streaming.rs:193-195`), and it states a fact about the generated signature, not about the provider (codex calls it "the least compelling H candidate"). Printing the projected reader type is a renderer detail of H1. Its `as` would also collide with L3's `as Format`.
