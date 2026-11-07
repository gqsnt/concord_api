# Maintenance record: trade history (SDK47 Kraken Spot REST)

Role: independent maintainer (not the author, not the first consumer). Date: 2026-11-07.
Live provider calls: **NOT_RUN**. All behaviour below was observed against loopback fixtures.
Baseline: author commit `322da9f2` plus consumer commit `53a4c70b`, on frozen Concord
`c3fd4f1e`. No shared crate, check, script or other example was edited.

## Prior exposure (read before starting)

- The role brief, then `git log`. The bodies of the two previous commits summarise the author's
  result and the first consumer's findings (48 concurrent sends, strict-provider rejections, the
  path-prefixed base refusal, "one missing order field fails the whole read"). That summary is the
  only first-consumer content I saw before the change was implemented and tested.
- This package: `src/*`, `tests/protocol.rs`, `tests/workflows.rs`, `README.md`, `PROVENANCE.md`,
  fixtures and the Python oracle. From `tests/first_use.rs` only the test names and its helper
  functions (lines 1–108), when writing my consumer test.
- Concord: `docs/CHEATSHEET.md`, pagination sections of `docs/RECIPES.md` and `docs/CALLER_GUIDE.md`,
  `docs/AUTHOR_GUIDE.md` §2, §2.1, §5, §5.6 and §9, `crates/concord-pagination` (README, manifest,
  `lib.rs`, `offset.rs`, the public parts of `pages.rs` and `types.rs`), `concord-codecs/src/form.rs`,
  `concord-core/src/response.rs` (`Operation`), `examples/testserver/src/lib.rs`.
- `FRICTION.md` and `FIRST_USE.md`: read only after the implementation and its tests passed.
- By mistake my first downloads went into an existing `target/tmp/docs/` holding another role's
  rustdoc text extracts. I saw only their file names, opened none of them, and moved my two pages to
  `target/tmp/maint/`.

Official pages, read 2026-11-07 (fetched once, read-only; both redirected):
- https://docs.kraken.com/api/docs/rest-api/get-trade-history →
  https://docs.kraken.com/api-reference/account-data/get-trades-history
- https://docs.kraken.com/api/docs/guides/spot-rest-auth →
  https://docs.kraken.com/exchange/guides/rest/authentication (the author's authentication page)

## What changed for callers

| New | Behaviour |
|---|---|
| `api.account().trades_history(options)` (generated) | Signed `POST /0/private/TradesHistory`, first page only, `set_account_id` like the other private reads. `Response<TradesHistory>`: `trades: BTreeMap<trade id, Trade>`, `count: Option<u64>` |
| `api.trade_history(options, account_id, from)` | `TradePages = concord_pagination::Pages<(String, Trade), u64, TradesPageMeta, Error>`. Walks `ofs` until `count`. Same filters and wallet query on every page. Each page goes through the ordinary `send policy`, so it is signed with a fresh nonce from the shared `Nonces`, after the previous page has arrived. The cap defaults to `TRADES_PAGE_CAP` (20); `max_pages` replaces it |
| `TradesHistoryOptions`, `TradeBound::{Time, Trade}`, `TradeType`, `AssetClass` | The reference's filters: `otp`, `type`, `start`/`end` (timestamp or trade id), `consolidate_taker`, `ledgers`, `rebase_multiplier`, `aclass`, `pair`. `ofs` is `pub(crate)`: only the traversal sets it. Debug withholds every value |
| `Error::Page(PageFailure { meta, offset, problem })`, `PageProblem::{MissingCount, Offset(OffsetError)}` | A page without `count`, with more than 50 trades, ending beyond `count`, or empty before `count` stops the walk. It does not loop, skip, or probe again |
| Re-exports `concord_pagination`, `Incomplete`, `Page`, `PagesError`, `Resume` | Callers can name the traversal's results without adding a dependency |

End states: `collect_complete()` returns `Ok(pages)` only once `count` is reached.
`Err(Incomplete { collected, error })` keeps every completed page. `error` is
`PagesError::Bounded { resume: At(next ofs) }` when the cap stops the walk, or
`PagesError::Provider { error: sdk::Error, failed: <that page> }` when a page fails. Nothing is
retried. To resume, pass `error.resume()` as `from`, with the same options.

Unchanged for existing callers: market discovery, ticker, `select_market`, balances, open orders,
`account_snapshot`, `Policy`, `Nonces`, signing and the transport. All 34 pre-existing tests pass
unmodified, including the exact-byte and API-Sign vectors for Balance and OpenOrders. `Error` was
already `#[non_exhaustive]`, so the new variant does not break callers outside the crate.

## Grounding in the provider documentation (read 2026-11-07)

- Route, signing and permission: `POST /0/private/TradesHistory`, `API-Key`/`API-Sign`, permission
  "Query closed orders & trades". The signature covers the URI path and SHA256(nonce + POST data),
  and the nonce must increase per key (authentication guide). The pre-existing `Policy` reproduces
  this. The reference's request examples use JSON bodies; the SDK keeps the authentication guide's
  form encoding. Whether Kraken accepts form bodies on this route is NOT_RUN.
- Pagination facts: `ofs` is a result offset. `limit` defaults to 50 (range 1–100). `count` is the
  number of matching trades, and is `0` when `without_count` or `with_cursor` is true. That is why
  neither option is offered: the walk depends on `count` and on the 50-trade default.
- `start` is exclusive and `end` inclusive; each accepts a UNIX timestamp or a trade transaction
  identifier. The reference types both as `integer`, yet describes a txid, which is a string such as
  `THVRQM-33VKH-UCI7BS`. Hence the `TradeBound` union.
- Default order is "most recent trades" first. Inferred, not verified: a trade that arrives during a
  walk shifts later offsets, so an older trade can appear twice (merge by id), and a fixed `end`
  keeps the window stable.
- **`ofs` is marked deprecated** on the reference, in favour of `with_cursor` + `cursor`. The ticket
  asked for `ofs`, so it is implemented and the deprecation is stated in the README. Cursor mode is
  the documented successor; it would change the key to an opaque string and the end to an absent
  `cursor.next`.
- The reference's own response example omits `count`. Therefore `count` is optional on a single
  page, and the walk refuses a page that lacks it (`PageProblem::MissingCount`, tested with that
  example verbatim).
- The trade schema marks no field required, and the example omits several documented fields. I made
  nine fields required (order, pair, time, side, order type, price, cost, fee, volume); all others
  are optional. Position-closure fields are not modelled and are ignored.

## Every file and location edited

| File | Location | Change |
|---|---|---|
| `Cargo.toml` (package) | `[dependencies]` | `concord-pagination` path dependency |
| `Cargo.lock` (root) | this package's entry | `+ "concord-pagination"` (the only lock change) |
| `src/lib.rs` | crate docs, re-exports, `mod trades`, `scope account` in `api!` | new endpoint `trades_history`, re-exports |
| `src/policy.rs` | `Policy::prepare_signed` path allowlist | `+ "/0/private/TradesHistory"` |
| `src/error.rs` | new `PageProblem`, `PageFailure`; `Error` enum, `Error::status`, `Display` | `Error::Page` and its two match arms |
| `src/models.rs` | after `OpenOrder` | options, bound/type/class enums, `TradesHistory`, `Trade` |
| `src/trades.rs` (new) | — | `Kraken::trade_history`, page projection, constants, `TradesPageMeta` |
| `fixtures/trades-history.json`, `fixtures/trades-signatures.json` (new) | — | official example; independent vectors |
| `fixtures/signatures.py` | `main()` | checks the four new vectors (nine in total) |
| `tests/trades.rs`, `tests/trades_consumer.rs` (new) | — | 11 regression/protocol tests; 1 consumer test |
| `README.md` | call table, new "Trade history" section and doctest, "five declared paths", failure table, scope limits | docs |
| `PROVENANCE.md` | appended maintenance section | sources, fixtures, dependency, validation |

Facts that had to be written in several places:
1. **The route.** It is `path "TradesHistory"` in the declaration and `"/0/private/TradesHistory"` in
   the policy allowlist, two spellings of one fact. The README table and the "five declared paths"
   sentence restate it a third and fourth time.
2. **The wallet query.** `#[sensitive] query account_id?: String` now appears on three endpoints, and
   the traversal takes it as a separate argument. It is one fact written four times.
3. **Shared option fields.** `otp` and `rebase_multiplier`, with identical serde attributes, plus a
   hand-written redacted `Debug`, are repeated in `BalanceOptions`, `OpenOrdersOptions` and
   `TradesHistoryOptions`.
4. **The page size.** "50" is the `TRADES_PAGE_SIZE` constant, the endpoint doc and the README.
5. **The new error stage.** `Error::Page` needed arms in `status()` and `Display`.

## Did the compiler or the tests catch an omitted edit?

Each mutation below was applied to a copy and the original restored (scripts and logs in
`target/tmp/maint/mutations/`):

| Omitted / wrong edit | Compiler | Tests |
|---|---|---|
| m1: route missing from the policy allowlist | **silent** (compiles) | 10 of 11 `trades` tests, the consumer test and the README doctest fail with `Local(SigningInput)`. All 34 pre-existing tests pass |
| m2: no `Error::Page` arm in `status()` | **E0004** non-exhaustive match | — |
| m7: endpoint not declared | **E0599** `no method named trades_history` (in `trades.rs`) | — |
| m3: traversal does not set `ofs` | silent | 7 `trades` tests + consumer fail; the walk ends in `BeyondCount`, not a silent loop |
| m4: `count` ignored (short-page rule) | silent | At first only 1 test failed. The mutation showed it would add one unneeded signed request when `count` falls on a full page, so I added `count_reached_on_a_full_page_ends_without_an_extra_signed_request`; now 2 fail |
| m5: default cap not applied | silent | 1 test (`the_default_cap_bounds_signed_requests_on_a_long_history`) |
| m6: wallet query not forwarded to later pages | silent | 1 test |

So the declaration ↔ caller link and the error enum are compiler-checked. The declaration ↔ policy
link (m1), and everything about the walk, are checked only by tests that call the new route. The
pre-existing suite cannot see m1.

Also tested: replacing the three `account_id` declarations with a Concord parameter group
(`params Wallet(#[sensitive] query account_id?: String);` plus `..Wallet`). It compiles, and all 48
tests and clippy pass with **no test edit**: the endpoints keep their own `set_account_id`, so callers
do not change. Not adopted. It adds a public `Wallet` trait to save two lines, and that choice is
the owner's.

## Remaining duplication (generic, not Kraken-specific)

- The allowlist restates routes the declaration already knows (fact 1). `Operation` exposes only
  name and method, so a policy cannot ask "is this a declared private operation, at exactly its
  declared path?"
- Offset-walk glue in `src/trades.rs` (about 40 lines): clone the client, rebuild the endpoint with
  the page key, send, check `count`, run `next_offset`, map `OffsetError` to the SDK error, build a
  `Page`. Only the field names (`count`, `trades`, `ofs`) and the page size are Kraken's.
- The page key goes into the body. The RECIPES pattern ("set the token on a clone of a request
  template") works for query and header values through setters, but a body is one opaque value
  without a getter. So the traversal keeps its own copy of the options and rebuilds the endpoint for
  each page. To keep `ofs` SDK-owned, it is a `pub(crate)` field, which means the generated
  single-page call can only read the first page. Callers who want a specific offset use
  `trade_history(.., Resume::At(k), ..).max_pages(1)`.
- Fact 3: three option records each carry a hand-written `Debug` and the same `otp` and
  `rebase_multiplier` fields.

## Provider-specific responsibilities (stay in the SDK)

The exact-byte signature and the allowlist. The `ofs` semantics, the default 50-trade page and the
meaning of `count`, which is why `without_count`, `limit` and cursor mode are excluded. Exclusive
`start`, inclusive `end`, and the timestamp-or-txid union. Newest-first ordering and the
duplicate/shift risk. A missing `count`. Lenient trade decoding.

Things the SDK does not decide, and documents as the caller's job: reusing one `otp` on every page
(whether a one-time code can sign several pages is not documented, so this is NOT_RUN); the quota
cost of each page (no limit is claimed, and the cap of 20 is an SDK default, not a provider number);
ordering against other calls on the same key. Within one walk, nonces rise with offsets, because a
page is signed only after the previous page arrives (tested with 8 concurrent walks plus 8 balance
reads: 32 distinct nonces). Across walks no arrival order is promised. A request allocated earlier
but delivered during a walk is refused by a strict provider, and the walk continues (tested).

## Candidate Concord improvements (not made; frozen baseline)

1. **Declared route on the endpoint identity.** Expose the composed static path (or path template)
   through `Endpoint::operation()` or `Operation`. A send policy could then authorize "declared
   operations of scope `account`, at exactly their declared path" without restating strings. That
   turns m1 from a silent omission into nothing to omit.
2. **An offset constructor in `concord-pagination`.** For example, `Pages::offsets(from, limit,
   fetch)`, where `fetch(offset)` returns items, an optional total and metadata, and the crate
   applies `next_offset` and reports `OffsetError` through a conversion into the factory error. This
   removes the generic half of `src/trades.rs`; field names stay in the SDK. `next_offset` alone
   already removed the error-prone arithmetic. The mutations show what it protects: termination at
   `count`, and no extra probe on a full final page.
3. **Generated docs for send-policy outputs.** The new endpoint's rustdoc again names
   `Envelope<TradesHistory>`, not the `Result<Response<TradesHistory>, Error>` callers receive (the
   first consumer's F2 applies unchanged).

No change to the client → scope → endpoint hierarchy is suggested: the new single-page read is one
more endpoint in `scope account`, and the traversal is an SDK method over it.

## Compared with the first consumer's findings (read afterwards)

- F7 of `FRICTION.md` asked that a future pagination extension use the shared engine rather than a
  hand-written traversal. This change does: `Pages` and `next_offset`.
- F12 (helpers drop partial results) does not apply to the walk: `Incomplete` keeps completed pages.
- F14 (one missing field fails a whole read) applies to `OpenOrder` and is unchanged. `Trade`
  requires only nine core fields. I had the one-line commit summary of F14 before designing, so this
  is not an independent rediscovery.
- F9 (a path-prefixed base refuses every call) now covers the new route too, unchanged.
- F6 (wallet as a separate mechanism) continues in the traversal's `account_id` argument.
- F10 (Display omits the endpoint) also holds for `Error::Page`; the operation is in
  `PageFailure.meta`.
