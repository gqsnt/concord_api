# Capability capsule: `concord-ratelimit` — shared admission, keyed budgets, cooldown, concurrency leases

You implement a new focused crate in the Concord Rust workspace (your working directory, a git
worktree on branch `sdk50/p2-ratelimit`). Read `AGENTS.md`/`CLAUDE.md` if present,
`docs/AUTHOR_GUIDE.md` §5.11–5.13, `crates/concord-retry/README.md`, `crates/concord-auth/README.md`.
You cannot commit; leave the change in the working tree. Network is enabled (loopback tests run);
contact no external service.

## Evidence

A wave-1 SDK for Discord (in another worktree; read it, do **not** edit it:
`/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/w1-sdk24-discord-rest/examples/discord-rest-sdk/src/limiter.rs`
and its `FRICTION.md` items 4–7) wrote ~260 lines of rate admission by hand: a route key made of the
operation and the major resource id (channel/guild), a map from route key to the provider's bucket
id (`X-RateLimit-Bucket`), per-bucket remaining/reset windows from `X-RateLimit-Remaining` /
`X-RateLimit-Reset-After`, a global gate set by a global 429, per-route cooldown from 429 bodies
(`retry_after`, fractional seconds) or headers, and refusal (`Gated`) when a request would be
sent into a known-empty window. Earlier corpus: `examples/native` has a semaphore admission
recipe (`items_admitted`), `examples/riot` stops on the first 429 and reports `Retry-After`,
GitHub/GitLab SDKs parse rate headers into metadata without admission.

## Ownership

Rate limiting owns: admission against explicit **scope keys** supplied by the SDK (the crate never
infers a bucket from a URL or an endpoint object), local request-rate budgets (token bucket or
window — say which and why), provider-observed windows and **cooldowns** (a 429 or a "remaining 0"
observation closes a key until a time; a global key gates everything), **concurrency leases**
(a permit that is released on drop/completion/cancellation, and that can be held until a response
body/stream finishes when the SDK chooses that model), fairness/ordering choice stated, bounded key
storage. It does **not** own: provider header names and meanings (the SDK maps headers to
observations), retries (`concord-retry` owns repetition; never hold a permit during a retry wait),
credentials. A local policy is not a claim about the provider's distributed quota; cross-process
coordination is out of scope (say so).

## What to build

1. `crates/concord-ratelimit` (`publish = false`, workspace lints/docs). Runtime-free core with
   injected clock and sleep (a `tokio` feature may supply defaults). Small API; justify each type.
   Suggested shape (change it if you find simpler): `Limiter<K>` with
   `acquire(&[keys]) -> Admission` (wait or refuse according to a mode the caller picks),
   `observe(key, Observation)` (remaining/reset, cooldown until, global), `lease(key)` for
   concurrency permits. Cancellation of a waiting acquire restores admission; a dispatched request
   keeps its charge; no permit is held across a retry backoff.
2. Tests (deterministic; injected time; a real executor where concurrency is the risk): two keys
   sharing a provider bucket, a global cooldown gating every key, a 429 cooldown with fractional
   seconds, remaining-0 window then reset, refusal vs waiting mode, cancellation of a waiter, lease
   release on drop and on stream completion, bounded storage, shared clones of one limiter
   competing fairly as stated, overflow-safe time arithmetic.
3. A Discord-shaped integration test inside the crate's tests (a fake server with route buckets,
   a shared bucket across two channels, a global 429) driving requests through a generated Concord
   endpoint and a small `SendWith` policy that uses the crate — this is the shape the Discord SDK
   will migrate to after its wave closes. Count how many of `limiter.rs`'s generic lines the crate
   would replace and list what stays Discord-specific.
4. Adoption now (no wave-1 SDK edits): replace `examples/native`'s semaphore admission recipe with a
   lease from the crate if its tests' assertions stay unchanged; give `examples/riot` a cooldown
   from its `Retry-After` observation if that keeps its behavior and tests.
5. Composition: one documented example with `concord-retry` (retry wait outside any permit; each
   attempt re-acquires) and `concord-auth` (token-service traffic admitted under its own key).
6. Gate: `scripts/verify.sh` steps (tests/clippy/doc per feature), `check-graph.py`, `check-ci.py`,
   `package-check.sh`, `consumers.sh` crate lists, packaged consumer `checks/consumers/ratelimit-only`.

Bounded verification (`CARGO_BUILD_JOBS=4`, one cargo job at a time). Report the public API with a
one-line justification per item, decisions on ambiguous points, test counts per package, the
`limiter.rs` replacement estimate, and anything left undone.
