# Wave 1 synthesis (SDK21–25) and the changes made before Wave 2

Wave 1 froze Concord at `56a16ee3` and built five SDKs on it: SDK21 Open Library (simple
control), SDK22 NASA NeoWs, SDK23 USGS Earthquake Catalog, SDK24 Discord REST, SDK25 FRED. Each had
an author, a fresh docs-only consumer and an independent maintainer (15 roles, ledger in
[`../../MODELS.md`](../../MODELS.md)); 18 findings are in [`FINDINGS.md`](FINDINGS.md) and the
candidate list in [`CANDIDATES.md`](CANDIDATES.md). This file gives the dispositions. Every
change below was compiled and tested against the SDKs that motivated it; counts are from the
commits named.

## What Concord learned

**1. A finite-body bound is a fact of the declaration, not of a policy.** Four of five Wave-1
authors wrote a send policy whose first job was `decode_limited(n)`, as ten SDK20 SDKs had; and
the native `prepare()?.send()` silently skipped that bound (SDK21, SDK22, SDK25 F4, SDK23).
Change (MACRO + CORE_CHANGE, `b12-limit`): `limit EXPR;` on a client, scope or endpoint, carried
by the `Decoder`, honoured by generated `send()`, `Received::decode()` and `prepare()?.send()`;
rejected at compile time on streams and drains (`ui/limit_on_stream_or_drain`). Adoption: 12 SDKs
(`b12-x`, `b12-y`). Drive, Snowflake and Graph keep runtime limits from their configuration —
`limit` takes a constant expression and is not a configuration mechanism.

**What it did not do — the main lesson for Wave 2.** No SDK could delete its policy. After the
bound moved, every remaining policy exists for one of two reasons: mapping `concord::Error` into
the SDK's own error vocabulary (NASA's `Bounded` is now three lines plus `metered`, which copies
quota headers), or turning a non-2xx status into a typed provider document (Open Library 3 sites,
FRED 4, USGS 3, Airtable/World Bank/Dropbox 1 each). Across the 12 adopters, `decode_limited`
call sites went 13 → 0, but send policies went 8 → 8 (the other four SDKs decode in hand-written
methods, also kept for error mapping). The next core/macro question is therefore **declared error documents and SDK
error mapping**, not more policy helpers (W2 candidate K below).

**2. Generated docs must name what a caller sees.** Five witnesses (SDK22 consumer, SDK21
maintainer, SDK22 maintainer, SDK23 consumer, SDK24 consumer) read
`<Bounded as SendWith<T>>::Output` and went looking for the policy. Change (MACRO docs,
`b12-limit`): the generated `send()` links the policy type and says where its output is defined;
endpoint docs list included parameter groups and a declared `limit`; `prepare()` says which policy
it bypasses. The rustdoc artifact `where for<'__concord> NonZeroU64: Clone` (SDK23 consumer) is a
macro defect still **open** (W2 backlog).

**3. Reading one header field strictly is core work.** GitHub and GitLab accepted `+5` and
conflated absent with unparsable; NASA and Discord each wrote a parser; SDK22's maintainer asked
for one. Change (CORE, additive): `ResponseHead::single(name)` and `unsigned(name)` with
`FieldError { Repeated, Invalid, Overflow }` (safe `Display`). Adoption: NASA (parser removed,
semantics unchanged), GitHub (5 fields), GitLab (7 fields) — the latter two now refuse `+N` and
repeated fields, with tests. Friction found in adoption: an SDK holding only a `HeaderMap` builds
a `ResponseHead` with an invented HTTP version to call `unsigned` (GitHub `meta.rs`). Candidate L.

**4. Some query keys belong to the value.** Discord's `around`/`before`/`after` are mutually
exclusive; the SDK hid three optional values in a private `mod raw` and added forwarding wrappers
for the client, the channel and the request (+245 lines) — and the raw setters still existed.
ClickHouse's `param_<name>` keys are named at run time and were appended to the prepared URL
through native access (F3-4, SDK20 W4-7). Change (MACRO + CORE, `b12-pairs`): `query(..) x?: T`
with `T: QueryPairs`. Endpoints only; never `#[sensitive]`; no key, `each` or `header(..)`; an
empty key is a `Build` error; the endpoint's `Debug` withholds the value, and `Prepared`'s `Debug`
redacts any run-time key on the well-known secret list (a leak found while writing the tests).

*Adoption, measured by another model* (`b12-pairs-adopt`, gpt-6.1-terra): Discord's `api.rs` went
from 316 to 177 lines (src −221/+92 including moved docs); the raw module and all three wrapper
layers are gone; `anchor()` stays as a 4-line compatibility method. ClickHouse's URL-mutation
helper is gone (−17/+8 production lines); validation and sorted pairs stay in the SDK because
`QueryPairs` is infallible by design. Existing assertions unchanged in both.

*The adoption changed the design.* As first built, pairs were written after every keyed value;
Discord's frozen tests pin `before=..&limit=..`, so the adopter had to make `limit` a
`QueryPairs` type too — losing its keyed placement and its `Debug`. Now a `query(..)` value is
written **where it is declared** among the endpoint's parameters (after inherited keys, before
`query { }` block entries), and Discord's `limit` is an ordinary keyed value again
(`pairs_keep_their_declared_position_among_the_endpoints_keys`). Disposition: **accepted**
(MACRO + CORE). Cost: one trait, one writer type, one placement form; a `Contrib::param` marker in
the macro. Reversal: if a later SDK needs fallible pair construction inside Concord, or pairs on
scopes (rejected today because other endpoints' requests cannot know them).

**5. Confirmations.** Parameter groups (F3-1) were used unprompted by the SDK25 author
(`params OffsetOptions`), and the SDK25 maintainer added a group to three endpoints in one edit; a
deliberately missed inclusion failed with E0277 through generic code. `Sends<O>` (F3-2) was
adopted unprompted by the SDK22 maintainer for `send_metered()` without breaking the frozen
consumer. Client → scope → endpoint held for all five providers; SDK22's ceremonial `scope neows`
is an SDK style choice the guide already allows avoiding.

**6. Simple control.** SDK21 Open Library's consumer got a clean first use from README and
rustdoc (predictions matched, including empty pages and 429 metadata). After all changes the SDK
differs by one `limit` line, one `decode()` call and one new test (+18/−1). None of the new
concepts is required to write a simple SDK; each is opt-in where its problem exists.

## Dispositions

| # | Candidate | Disposition | Evidence and reversal |
|---|---|---|---|
| A | Declared finite-body limit | **MACRO + CORE_CHANGE, accepted** | 12 adopters; native bypass closed (`b12_declared_limit.rs`); reverse if a runtime-configured limit becomes the common case (3 of 15 today). |
| B | Generated docs (policy link, groups, limit, prepare bypass) | **MACRO, accepted** | 5 consumer/maintainer witnesses; W2 consumers check it. |
| C | Strict header-field readers | **CORE, accepted** | NASA/GitHub/GitLab adoption; L refines the receiver. |
| D | Public media/charset check | **EXTERNAL (`concord-codecs::check_content_type`), accepted** | FRED dropped its parser and `mime` (+16/−48 production lines). |
| E | Offset continuation helper | **EXTERNAL (`concord-pagination::next_offset`), accepted** | FRED and USGS keep only provider projections; 8 unit tests incl. overflow. |
| F | One operation, several representations | **EXPERIMENT continues** (F2-5) | Witnesses 4–5 (SDK25 F5, SDK23 (1)); groups removed the parameter repetition, but route, limit, group membership and format/query pairing still repeat per representation. Needs a compiled variant syntax compared with separate endpoints, and a single-format control that does not get longer. |
| G | Rate admission (`concord-ratelimit`) | **EXTERNAL pilot, not accepted** | The Discord migration grew `limiter.rs` from 505 to 625 lines: the crate's `Observation::Window` charges admissions, Discord's tested policy only classifies open/exhausted windows, so the adapter kept a second map; `Error::Gated` does not say whether the global gate or a key refused, so the SDK probes twice; the crate's `Duration` epoch needs an `Instant` adapter. Migration **not merged** (kept outside the repo as evidence). The attempt found a real defect in Discord's own limiter — a late success reopened an exhausted window — fixed in the SDK with two tests that fail without the fix. W2 needs a second provider (HubSpot, Intercom, Zendesk) before the crate's API is revised. |
| H | Docs: version prefix in `path`, query-credential transport, Error accessors | **DOCS, done** | AUTHOR_GUIDE §3 (compiled transport recipe), CALLER_GUIDE §8. |
| I | XML codec | **EXTERNAL, waiting** | One provider family (FRED + S3 `ListXml`); wait for a third unlike witness. |
| J | Bounded ZIP member reader | **OMIT** | FRED only. |
| F3-4 | Dynamic query keys (`query(..)`) | **MACRO + CORE, accepted** (item 4) | Discord anchors and ClickHouse bindings adopted by an independent lane; declared-position rule came from that adoption. |

## New candidates for Wave 2 (evidence first)

| # | Candidate | Witnesses | Placement question |
|---|---|---|---|
| K | Declared error documents / SDK error mapping (why policies remain) | 8 policies + 4 hand-written send paths in 12 adopters; F2-3 baseline ~22 sites | MACRO statement (`error Problem;`?) + CORE vs keep `Received::document` in policies; counterexample: SDKs whose error depends on status *and* body (Discord 429, FRED media inconsistency) |
| L | Header-field readers on `HeaderMap`/`ResponseMeta`, not only `ResponseHead` | GitHub adoption | CORE: receiver choice, no new type |
| M | Policy access to declared values (resource id, path template) instead of re-parsing the URL | SDK24 (`resource_of`, 22 lines), SDK23 maintainer | CORE `Operation` data vs SDK; `Operation: Hash` (SDK24 `RouteKey`) is the small part |
| N | Typed domain-input projection (struct → declared values) | SDK23 maintainer, Kubernetes `ListOperation` | compare with groups (F3-1) |
| O | `concord-retry::http::retry_after` fractional seconds | SDK24 (Discord bodies) | EXTERNAL, opt-in parse mode |
| P | Rustdoc artifact `for<'__concord> T: Clone` | SDK23 consumer | MACRO defect |
| Q | Pagination × streaming rows across pages | SDK23 author + consumer | EXTERNAL |

## Subtraction and cost, by lane

| Lane (author) | What | Production lines (−/+) | Tests added | Existing assertions |
|---|---|---|---|---|
| `b12-limit` (coordinator) | A, B, C | core/macro additions | `b12_declared_limit.rs`, `b12_header_fields.rs`, UI | unchanged |
| `b12-x` (gpt-6-astra) | D, E; FRED/USGS adopt A | FRED −48/+16, USGS −21/+26 | 15 + 1 doctest | unchanged |
| `b12-y` (gpt-6.1-sol) | A in 10 SDKs, C in 3 | per SDK −1…−7 / +18…+59 (mostly tests) | 11 limit tests, 2 field tests, 2 Discord limiter tests | unchanged |
| `b12-pairs` (coordinator) | F3-4 `query(..)`, H docs | core ~110, macro ~100 | 5 + 4 UI + 1 doctest | unchanged |
| `p2-ratelimit` (gpt-6.1-terra) | G pilot crate | new crate 436 | crate tests | — |

### Independent review

`b12-review-sol` (Codex gpt-6.1-sol xhigh, read-only, `8dc53558..d3808b29`, 4 min) reported 7
findings; each was checked before acting:

| # | Finding | Check | Outcome |
|---|---|---|---|
| 1 | `concord-ratelimit` refusal mode can race with `remove` and poison the mutex | **reproduced** (`refusal_admission_survives_removal_before_poll` panicked, then `PoisonError` in a lease drop) | fixed: setup and first check under one lock; poisoning recovered |
| 2 | One FIFO for admission and leases can deadlock | **reproduced** (`lease_holder_can_acquire_ahead_of_a_lease_waiter` timed out) | fixed: independent admission and lease queues |
| 3 | `limit` re-parsed a forwarded `$d:expr` without its grouping | **reproduced**: `16 / $d`, `$d = 1 + 3` gave 19 | fixed (`parse_initializer`) |
| 4 | README composition order can dispatch during a cooldown | **reproduced** (dispatch at 200 ms inside a cooldown to 1000 ms) | fixed: lease first, admission immediately before dispatch |
| 5 | generated `const __LIMIT: usize` captured caller names | **reproduced**: E0391 cycle, E0308 with a `usize` alias | fixed (bound passed as written) |
| 6 | key-encoding test used only safe keys | confirmed | test now writes `k&y=z` in a key |
| 7 | Notion README understated the declared bound | confirmed | corrected |

Sound by the reviewer's reading: limits reach every finite path; no redaction leak or
over-redaction in the query-pairs, Debug and Error paths; the header readers and their adoption;
the media checker and offset helper; the SDK limit migrations; Discord's overlap change. It did not
cover every cfg/raw-identifier/group combination or runtime interleavings beyond reading. The
`query(..)` position change and the adoption lane came after the reviewed range and are covered
by a follow-up review (`b12-review2-sol`).

`b12-review2-sol` (Codex gpt-6.1-sol xhigh, read-only, `d3808b29..78b1ff77`) found three more:

| # | Finding | Check | Outcome |
|---|---|---|---|
| 1 | `limit __DECODE;` / `limit __META;` resolved to the generated statics of `prepare()` | **reproduced** (two E0308) | fixed: the bound is a private associated constant of the endpoint type |
| 2 | "before block entries" overstated: an endpoint's `append` to an inherited key keeps that key's first position | confirmed | wording corrected; witness `v=1&v=2&p=9&new=n` |
| 3 | the ratelimit cancellation regression's 10 ms window expired before its check | confirmed | window 1000 ms, so a stray charge is detected |

Sound by its reading: `query(..)` ordering with group splices, absent values, pair-only and
block-only endpoints; the earlier fixes; Discord's exclusive cursors and wire order; ClickHouse's
validation and execution boundary; ratelimit locking, queues and poison recovery; the doc fixes.
The three fixes above (`292c0fbc`) were made by the coordinator and are covered by failing-first
or witness tests; they have not been re-reviewed and are in scope for the next between-waves
review.

## Not done, and why

- Live provider calls: NOT_RUN (no authorization); all evidence is loopback.
- Native Windows: not run locally; CI covers Windows on the pushed campaign branch.
- Open-library-style first use was not re-run by a fresh consumer after the changes; the W2
  consumers are the next check of generated docs (B).
