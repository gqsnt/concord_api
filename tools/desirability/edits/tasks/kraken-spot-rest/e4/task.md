# kraken-spot-rest / e4: add an endpoint like its sibling, with its policy classification

**Provider changelog.** This SDK now needs Kraken's extended balance read,
`POST /0/private/BalanceEx`: a private, signed read exactly like `POST /0/private/Balance` (the
same form options `otp` and `rebase_multiplier`, the same optional `account_id` query value, the
same `API-Key` / `API-Sign` / nonce rules). Its result maps each asset to an object
`{"balance": "<decimal>", "hold_trade": "<decimal>", ..}`. Nothing else changed.

Add it next to `balances`.

Public names the hidden check uses:
- `kraken.account().extended_balances(options: BalanceOptions)`, whose `send().await` returns
  `Result<Response<ExtendedBalances>, Error>`;
- `concord_sdk_kraken_spot_rest::ExtendedBalances` (a map from asset name to
  `ExtendedBalance`) and `ExtendedBalance` with public fields `balance: String` and
  `hold_trade: Option<String>`;
- the request is signed like `balances`: `API-Key`, `API-Sign` over `/0/private/BalanceEx`, body
  starting with `nonce=` followed by the encoded options, and `.set_account_id(id)` like
  `balances`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
