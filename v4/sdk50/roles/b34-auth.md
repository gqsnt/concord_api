# Between waves 3 and 4: refresh-token rotation in concord-auth

Working directory: a git worktree of the Concord workspace on branch `sdk50/b34-auth`, from the
integration head (which contains the Wave 3 SDKs). Build with `CARGO_BUILD_JOBS=4` and the
worktree's default `target/`, one cargo command at a time. You cannot commit; leave the work in
the working tree. Read nothing outside the worktree except the cargo registry. No external service.

## The evidence (three providers)

`concord_auth::TokenCache` caches an access-token header with a lifetime and single-flight
refresh. Providers that rotate the refresh token need more, and each SDK built it by hand:
- `examples/hubspot-crm-sdk` (Wave 2): a replacement refresh token must not be overwritten by an
  older one under concurrency.
- `examples/xero-accounting-sdk/src/{auth,store}.rs` (FRICTION F1): a 343-line journal persists
  the rotated pair before use. Its fresh consumer (FIRST_USE F11) found that any refresh that
  does not end with a saved pair erases the old refresh token — a 503 or a refused connection
  included — so a network blip forces reauthorization, although Xero documents a grace retry with
  the old token.
- `examples/quickbooks-online-sdk/src/auth.rs` (FRICTION F3): no room for a rotated refresh
  token in what an acquisition returns (side channel); a failure removes the entry, so the SDK
  remembers a rejected grant itself; the authorization-code exchange's access token cannot be
  inserted (a one-use "seed"); a replacement arriving during a refresh needs an SDK counter.

## Task

1. In `crates/concord-auth`, add the smallest compatible API for rotation: inserting a token
   obtained elsewhere; an acquisition that publishes provider state (the rotated refresh token)
   atomically with the access token; and a way to tell "the provider issued no new pair" from
   "a pair was issued but not stored", so an SDK can keep the old refresh token when nothing was
   issued. Persistence stays the application's; a documented durable rotating-pair recipe
   (crash boundary, process ownership) belongs in the README. Unit tests, rustdoc, README.
2. Adopt it in QuickBooks (`src/auth.rs`), Xero (only `src/auth.rs` and `src/store.rs`) and
   HubSpot **only where it subtracts**; measure production / test / docs lines removed/added per
   SDK. If an adoption grows the SDK, do not keep it — report why.
3. Behaviour rule: do not edit or remove any existing assertion. If you change a behaviour that
   a test pins (for example Xero's fail-closed refresh), stop and report it instead.

Run, for every touched package: `cargo test --locked -p <pkg>`, `cargo clippy --locked -p <pkg>
--all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p <pkg> --no-deps`,
and `cargo fmt --all -- --check`.

Report: the API (signatures), why each part is needed, per-SDK line measurements, tests, and any
adoption you dropped.
