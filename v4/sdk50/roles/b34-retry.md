# Between waves 3 and 4: an async, fallible before-attempt hook in concord-retry

Working directory: a git worktree of the Concord workspace on branch `sdk50/b34-retry`, from the
integration head (which contains the Wave 3 SDKs). Build with `CARGO_BUILD_JOBS=4` and the
worktree's default `target/`, one cargo command at a time. You cannot commit; leave the work in
the working tree. Read nothing outside the worktree except the cargo registry. No external service.

## The evidence (four independent witnesses, Wave 3)

`Retry::receive(prepared, mode, hook, classify, now, sleep, jitter)` takes a **synchronous,
infallible** `hook(&mut reqwest::Request, attempt)`. Three SDKs had to abandon it:
- `examples/adyen-checkout-sdk/src/policy.rs`: must refuse the next attempt when the
  idempotency guarantee expires during backoff → `Prepared::into_parts` + a fallible factory on
  `Retry::run` (FRICTION F1).
- `examples/xero-accounting-sdk/src/policy.rs`: must await `TokenCache` acquisition and a
  concurrency lease before each attempt, and release the lease before backoff (FRICTION F3, F4).
- `examples/paypal-sdk/src/policy.rs`: must await token acquisition (~30 lines re-creating the
  template cloning, FRICTION F2); and `Report` keeps only the last outcome, so "an earlier attempt
  ended without a response" lives in classifier state (F3; MAINTENANCE candidate 3).

## Task

1. In `crates/concord-retry`, add the smallest API that lets these SDKs keep the adapter: an
   async before-attempt hook that may fail with the SDK's own error (the run stops before that
   attempt is dispatched, with the refusal returned alongside the last completed outcome), and
   report accounting that distinguishes dispatched attempts from refused ones and records whether
   any dispatched attempt ended without a response head. Keep `receive` and `run` unchanged
   (compatible). Unit tests, rustdoc, README.
2. Adopt it in Adyen, Xero (only `src/policy.rs`) and PayPal **only where it subtracts**.
   Measure production / test / docs lines removed/added per SDK. If an adoption grows the SDK,
   do not keep it — report why.
3. Behaviour rule: do not edit or remove any existing assertion in any test. If an existing test
   conflicts with the shared API, stop that adoption and report the conflict.

Run, for every touched package: `cargo test --locked -p <pkg>`, `cargo clippy --locked -p <pkg>
--all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p <pkg> --no-deps`,
and `cargo fmt --all -- --check`.

Report: the API (signatures), why each part is needed, per-SDK line measurements, tests, and any
adoption you dropped.
