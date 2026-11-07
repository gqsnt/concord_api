# Capability capsule: `concord-auth` — sensitive credentials, scoped token cache, coordinated refresh

You implement a new focused crate in the Concord Rust workspace (your working directory, a git
worktree on branch `sdk50/p2-auth`). Read `AGENTS.md`/`CLAUDE.md` if present,
`docs/AUTHOR_GUIDE.md` §5.11–5.13 and §7, `crates/concord-retry` (its README: the attempt hook
where a credential is applied fresh per attempt), and the existing consumers below. You cannot
commit; leave the change in the working tree. Network access is enabled (loopback tests run);
contact no external service.

## Ownership

Auth owns: validated **sensitive** credential header values (Basic: correct Base64 of
`user:password` with the empty-password/colon rules made explicit; Bearer/other schemes from a
token), a **scoped** token cache (key chosen by the SDK: destination/audience, tenant, principal,
scopes/access — the crate never guesses a scope), checked expiry (provider lifetimes are
untrusted numbers: no `Instant` overflow; an unrepresentable lifetime is a typed refusal),
**coalesced refresh per credential generation** (concurrent callers for the same key wait for one
acquisition; a stale 401 can only invalidate the generation it used, never a newer token),
cancellation safety (a cancelled refresher or waiter leaves no stuck "refreshing" state and no
stranded waiter), and no blocking mutex held across I/O. It does **not** own: provider token
endpoints and their documents (the SDK supplies an async acquisition closure), signing
algorithms (SDK), credential persistence/rotation storage (application), retries (`concord-retry`
owns repetition; refresh permission is not replay permission), rate admission.

## Existing consumers (current source)

- `examples/distribution-registry-sdk`: `src/auth.rs` (`BasicCredentials`, a handwritten
  `base64`, `Challenge::bearer`, `TokenAuth`, `Access`), `src/client.rs`
  (`Registry::{execute, cached_token, fetch_token}`: an `Arc<Mutex<HashMap<Access, CachedToken>>>`
  cache, check and fetch not coalesced — concurrent requests fetch duplicate tokens; one challenge
  replay; the checked-expiry fix from M0).
- `examples/twilio-sdk/src/auth.rs` (`BasicAuth::new`), `examples/clickhouse-sdk/src/client.rs`
  (`Credentials::basic`): Basic encoding.
- Bearer construction in `examples/openai-sdk` (`OpenAi::new`), `examples/kubernetes-sdk`
  (`BearerToken::header_value`).

## What to build

1. `crates/concord-auth` (`publish = false`, workspace version/lints/docs). Small public API;
   justify each type. Prefer plain async closures for acquisition. If you need a Base64 encoder,
   use a mature crate already in `Cargo.lock` if any (`cargo tree` / lockfile), else a small
   audited implementation with exhaustive tests; justify in the crate README.
2. Credentials: `basic(user, password) -> Result<HeaderValue, _>` (sensitive) and
   `bearer(token)`-style constructors refusing control characters, with `Debug`/`Display` that
   never print secrets.
3. `TokenCache<K>` (or similar): `get_or_refresh(key, acquire)` returning a token snapshot with its
   generation; `invalidate(key, generation)` that only drops that generation; lifetime from the
   acquisition result (`Duration` or seconds) with checked arithmetic and an optional refresh
   margin; bounded or explicitly managed growth. Use an owned shared future or an equivalent
   cancellation-safe state machine (e.g. `tokio::sync` primitives behind a `tokio` feature, or a
   runtime-free design — choose and justify). Injectable clock for tests.
4. Integration: show the ordinary path — a `SendWith` policy that applies the current token per
   request and, on a 401 for the generation it used, invalidates that generation and retries the
   request **once** only when the request is replayable (template `try_clone`) — and composing with
   `concord-retry`'s attempt hook (fresh credential per attempt, after the wait), without a second
   retry loop. Count token-service requests separately from application attempts.
5. Adopt in the registry SDK: replace its token cache and Base64 with the crate where it keeps
   every existing registry assertion unchanged (80 tests); keep realm trust, scope selection and
   challenge parsing in the SDK. Adopt Basic in Twilio and ClickHouse if their tests stay
   unchanged. Do not edit any other SDK.
6. Tests (deterministic; injected clock; real executor where concurrency is the risk): cold call,
   cached call, expiry and refresh margin, overflow lifetime, N concurrent callers → one
   acquisition, a stale 401 does not evict the newer generation, a cancelled refresher/waiter then
   progress, acquisition failure propagates to all waiters and the next call retries acquisition,
   redaction sentinels in every `Debug`/`Display`.
7. Gate: add the crate to `scripts/verify.sh` (tests/clippy/doc per feature), `check-graph.py`,
   `check-ci.py`, `package-check.sh`, `consumers.sh` crate lists, and a packaged consumer
   `checks/consumers/auth-only` without the macro crate.

Bounded verification (`CARGO_BUILD_JOBS=4`, one cargo job at a time). Report the public API with a
one-line justification per item, design decisions on ambiguous points, test counts per package,
and anything left undone.
