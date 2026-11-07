# SDK author capsule — SDK32 PayPal

You are the **author** of a bounded SDK for PayPal built on the Concord Rust library
(your working directory is a git worktree of the Concord workspace at the frozen baseline).
Read `AGENTS.md`/`CLAUDE.md`, `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md` and the rustdoc of the crates you use. You may read
existing SDKs under `examples/*-sdk` as examples: **record every example file you consult and
what you took from it** (this makes the result example-assisted, which is fine, but must be
labelled).

## Provider slice: SDK32 · PayPal (complex; payments and accounting)

Connected workflows (both required):
1. Use client credentials to create an order and obtain its current state.
2. Capture an approved order and retrieve the resulting capture.

Contract notes: Client-credentials token acquisition, expiry and caching; PayPal-Request-Id supported per endpoint; 429 policy is not a published fixed quota.
Failure that must be exercised: Do not assume a refresh token, or retry a capture unless that specific operation permits its request ID.
Excluded: Buyer approval UI, refunds, payouts and blanket idempotency of every endpoint.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developer.paypal.com/api/rest/authentication
- https://developer.paypal.com/api/rest/requests/
- https://developer.paypal.com/api/rest/reference/rate-limiting/
- https://developer.paypal.com/api/rest/integration/orders-api
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/paypal-sdk/` plus adding `"examples/paypal-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.

## Deliver (package `concord-sdk-paypal` in `examples/paypal-sdk/`)

- `src/`: the declaration (`concord::api!`), the SDK's public types, its error type and any send
  policy, adapters for pagination/auth/retry/rate limits as the slice needs. Use Concord's
  ordinary declaration and generated `send()` path first; shared capabilities where they help
  (`concord-pagination`, `concord-retry`, `concord-auth`, `concord-ratelimit`, `concord-codecs`
  features — read each crate's README before deciding; if one does not fit, say why in
  `FRICTION.md` rather than bending the SDK to it); a native escape only where
  the provider contract needs it, documented. Keep provider models and rules in the SDK.
- `tests/`: `protocol.rs` (wire-level request shapes, every documented error class, malformed /
  truncated / oversized / wrong-media bodies, redaction of secrets in `Debug`/`Display`),
  `workflows.rs` (both connected workflows end to end against fixtures, including the required
  failure). Fixtures under `fixtures/`, each derived from a cited documentation example or
  authored and labelled synthetic; never copy expectations from your own implementation.
- `README.md` (public docs: what the slice covers, how to call it, failure semantics, what is
  excluded), `PROVENANCE.md` (every doc page + retrieval date, fixture derivation table),
  `FRICTION.md` (each Concord cost you hit: observable evidence, what you had to write by hand,
  provider semantics vs generic mechanics, the smallest Concord change that would remove it;
  include the examples you consulted).
- `cargo test --locked -p concord-sdk-paypal` passes; `cargo clippy --locked -p concord-sdk-paypal --all-targets -- -D
  warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-paypal --no-deps` pass.

Do not shrink the slice to a single list endpoint; implement both workflows completely.

## Slice-specific notes

Use the sandbox base. Tokens come from client credentials (there is no refresh token): concurrent
requests must not fetch twice, and an expired token is fetched again. `concord-auth` has a scoped
token cache with single-flight refresh — read its README, evaluate it against this rule, and record
exactly what it could not express. `PayPal-Request-Id` makes only the endpoints that document it
safe to retry; a capture is never retried unless its request ID permits it. A timed-out capture is
uncertain: surface it.

## Report (final message)
Files, line counts (src vs tests vs fixtures), the Concord features you used and for what, every
friction item (short), examples consulted, commands and results.
