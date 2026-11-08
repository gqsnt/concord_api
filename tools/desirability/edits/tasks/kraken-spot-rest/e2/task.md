# kraken-spot-rest / e2: rename a shared header

**Provider changelog.** Kraken renamed the signature header of private requests: every private
(account) request now sends its signature as `API-Signature` instead of `API-Sign`. The value is
computed exactly as before; `API-Key`, the nonce and the body are unchanged; public requests
still send neither. Nothing else changed.

Public names the hidden check uses:
- `kraken.account().balances(options).send()` (and every other account operation) sends
  `API-Signature: <signature>` and no `API-Sign` header;
- `Policy::prepare_signed(endpoint)?.request_unredacted()` carries the signature under
  `API-Signature`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
