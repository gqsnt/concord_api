# Independent maintenance capsule — SDK47 Kraken Spot REST

You maintain the SDK in `examples/kraken-spot-rest-sdk` (package `concord-sdk-kraken-spot-rest`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK47 · Kraken Spot REST (complex; signed requests)

Connected workflows (both required):
1. Fetch public market data and select a trading pair.
2. Authenticate and fetch account balances and open orders with signed private requests.

Contract notes: Private calls require API-Key, API-Sign and strictly increasing nonce; sign the exact encoded payload and URI path.
Failure that must be exercised: Concurrent requests sharing a nonce source must not repeat nonces; do not promise arrival order from allocation order.
Excluded: Order placement, withdrawals, futures, WebSocket and real-money trading.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://docs.kraken.com/exchange/guides/rest/authentication
- https://docs.kraken.com/api/docs/rest-api/get-account-balance/
- https://docs.kraken.com/api/docs/rest-api/get-open-orders/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/kraken-spot-rest-sdk/` plus adding `"examples/kraken-spot-rest-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.
- Read nothing outside your worktree except the official documentation pages above and the
  Rust toolchain / cargo registry sources: in particular, not the campaign files in its parent
  directories (mission, contracts, records, other roles' briefs or worktrees).

## Ticket

Callers need their trade history: `POST /0/private/TradesHistory` (signed, like the existing
private reads) returns trades keyed by trade id with a total `count`, filtered by `start`/`end`
(timestamps or trade ids) and paginated with the `ofs` offset (50 results per call). Read the
trade-history reference and the authentication guide and cite them with the date:
- https://docs.kraken.com/api/docs/rest-api/get-trade-history
- https://docs.kraken.com/api/docs/guides/spot-rest-auth

Add a trade-history traversal that walks `ofs` until `count` is reached (with a page cap and a
resume point; the same filters on every page), signs every page with a fresh nonce from the
shared nonce source, and keeps the trades of completed pages when a later page fails. Keep market
discovery, the ticker and the balance / open-order reads unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/kraken-spot-rest-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
