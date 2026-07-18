# Concord engineering-quality contract

## Minimal coherent design

1. Implement only behavior required by approved acceptance scenarios.
2. Prefer evolving or simplifying an existing production path over adding a peer path.
3. Do not add a public item, feature, dependency, state machine, registry, executor, abstraction, or
   generic parameter for hypothetical future use.
4. Do not create a temporary public API that a later phase must remove.
5. When a new mechanism replaces an existing equivalent, remove the obsolete production mechanism
   in the same phase after replacement proof exists.
6. Keep unrelated cleanup, renaming, formatting, and refactoring out of the phase patch.

## One architecture

A “single” architecture means one production owner of the behavior, not one function or one type.
Private decomposition is acceptable when helpers cannot independently execute, reorder, bypass, or
weaken the production lifecycle.

Audit, as applicable, for one owner of:

- generated semantic normalization;
- pending request and prepared public head;
- operation identity and deadline;
- request body and native-body accounting;
- native request execution;
- response-head observation and status classification;
- bounded buffering or streaming;
- decoding and result metadata;
- auth, limiter, pagination, or later state machines when their approved phase arrives.

No public transport replacement or unmanaged raw Reqwest path is permitted.

## Public API and type design

- Every public item must serve a documented workflow and reside in the correct namespace.
- Prefer compile-time capabilities, ownership, sealing, and exact types over runtime brands or
  `Any`-based dispatch.
- Avoid unnecessary `Clone`, `Send`, `Sync`, `'static`, boxing, and dynamic-dispatch bounds.
- Keep implementation adapters private. `__private` is only a narrow generated-code bridge.
- Stable public items require complete Rustdoc, feature notes, error behavior, and downstream
  compilation proof.
- Generated public Rustdoc must not reveal generated-private capability types or core private paths.
- Public or generated changes require an API diff or equivalent semantic review.

## Errors, limits, and sensitive data

- Errors remain typed and preserve safe causal context.
- Do not swallow failures, return success-shaped defaults, broaden retries, or silently fall back to
  a weaker behavior.
- Reduce native errors before storage or exposure.
- Never expose request/response body bytes, credentials, sensitive headers, native URLs, provider
  material, rate-limit keys, cursors, or proxy configuration through automatic diagnostics,
  recursive sources, hooks, debug events, or documentation.
- Custom extension-authored messages are application-owned boundaries and must be documented.
- Enforce finite limits before user decoding or native execution where required.
- Withhold the first excess chunk/frame rather than passing it to user code.
- Preserve primary-error precedence when secondary capture, cleanup, or completion fails.

## Async and concurrency

- No lock across `await`.
- No user callback, provider call, or limiter sleep under a core lock.
- Prefer ownership and guards over `Mutex<Option<T>>` cleanup patterns.
- Do not spawn detached work unless an approved requirement says it must outlive the caller.
- Treat cancellation as an ownership path; do not fabricate completion after panic or drop when the
  contract permits absence.
- Add deterministic interleaving/race evidence for concurrency changes and model-check reduced
  credential transitions when the repository policy requires it.

## Rust-specific discipline

- `unsafe` remains forbidden in core and macro crates without an accepted ADR and dedicated safety
  evidence.
- Preserve Rust 1.85/MSRV and edition constraints unless an approved compatibility change says
  otherwise.
- Avoid needless allocation, cloning, string formatting, boxing, and dynamic dispatch on hot or
  lifecycle-critical paths.
- Use checked arithmetic for aggregate lengths, counters, and deadlines.
- Keep feature-specific code behind coherent module-level `cfg` boundaries where practical.
- A dependency proposal must justify function, MSRV, features, compile/binary impact, security,
  source, and why existing dependencies or the standard library are insufficient.

## Readability and maintainability

- Use repository terminology consistently.
- Names should state domain responsibility, not implementation trivia.
- A file or module should have one primary responsibility.
- Split multi-subsystem orchestration before adding another concern; do not create vague `utils`,
  `common`, or `context` modules.
- Comments explain invariants, security, compatibility, cancellation, or non-obvious decisions. Do
  not narrate syntax or leave planning prose in production code.
- Remove dead code, abandoned alternatives, temporary logging, debug assertions used as behavior,
  and stale TODOs introduced by the phase.

## Test quality

- Test externally observable behavior and architecture-critical invariants.
- Prefer semantic compiler tests over generated-token snapshots.
- Use standalone manifests only when manifest topology is the contract.
- Use channels, barriers, gates, and paused time rather than arbitrary sleeps.
- Every new data-bearing error path requires a redaction sentinel.
- Every finite boundary needs below/equal/first-excess coverage where applicable.
- Never remove, ignore, weaken, retry, or rewrite a test merely to match the implementation.
- A test or verifier change that reduces coverage is a blocker unless the approved requirement
  explicitly changes the contract.

## Change-surface audit

Before each source commit and at phase end, record and justify:

- production files added, removed, and modified;
- production lines added and removed;
- public/generated items added, changed, or removed;
- dependencies and features changed;
- state machines or execution paths introduced;
- unsafe blocks introduced or modified;
- tests deleted, ignored, retried, or materially weakened;
- CI, task-runner, release, lint, deny, or verifier changes;
- files outside the expected mission surface;
- concepts introduced only for later work.

A new public abstraction, dependency, feature, state machine, unsafe block, verifier change, or
unexpected source area requires explicit mission authority and final-report justification.
