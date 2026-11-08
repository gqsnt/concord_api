# kraken-spot-rest / e1: version path prefix

**Provider changelog.** Kraken moved its Spot REST API from the `/0/` prefix to `/1/`: every
public endpoint is now `/1/public/<Method>` and every private endpoint `/1/private/<Method>`.
Signing is unchanged: `API-Sign` is still computed over the request's URI path (now
`/1/private/<Method>`), the nonce and the form body. Documents and statuses are unchanged.
Nothing else changed.

Public names the hidden check uses (unchanged names, new behaviour):
- `kraken.public().pairs()` / `ticker(pair)` request `GET /1/public/AssetPairs` / `Ticker`;
- `kraken.account().balances(options)` sends a signed `POST /1/private/Balance` whose
  `API-Sign` verifies against `/1/private/Balance` (`Credentials::signature` is the reference).

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
