# Performance

Concord keeps one managed Reqwest client per API client. Retry mode is chosen
at construction; no client is built per endpoint or request and there is no
pool keyed by endpoint retry settings.

## Execution hot path

A visible execution performs collision preflight, credential preparation,
rate-limit acquisition, sanitized hooks, secret materialization, one call to
`reqwest::Client::execute`, response observation, and terminal processing.
Reqwest-internal resends do not repeat Concord work.

`RetryMode::Disabled` provides exact visible-to-wire accounting.
`RetryMode::Status` can add at most one or two hidden resends for a cloneable
safe-method request. Streams, advanced bodies, and multipart remain
Reqwest-uncloneable. Protocol recovery has a Reqwest-owned bound that Concord
does not expose as stable performance metadata.

Authentication recovery reconstructs the logical request once and performs a
second visible execution. Request-local reusable auth preparation may be kept
only when its typed auth policy permits; rejection requiring refresh clears it
before the recovery.

## Bodies

Buffered JSON/text and reusable bytes materialize cloneable Reqwest bodies.
Request factories run for visible reconstruction, never for hidden Reqwest
resends. Streams remain streaming and multipart remains native; neither is
flattened to obtain cloneability. Exact-length and request limits are enforced
on every visible materialization.

Buffered responses collect once under the configured decompressed limit.
Streaming responses retain native lazy delivery and terminal error semantics.

## Hooks and rate limits

`DebugLevel::None` is the default. Custom debug sinks, hooks, and rate limiters
are on the visible-execution hot path. Hidden resends do not increment their
counters. Stored Retry-After cooldown is acquired by a later visible call and
does not delay terminal processing of the current 429.

## Benchmarks

The standalone `perf` crate contains public-surface targets for:

- managed client construction across all retry modes;
- visible execution overhead;
- generated facade overhead;
- reusable streaming and direct multipart recipes;
- native streaming upload and response execution;
- one bounded authentication recovery;
- response-limit failures and future-call Retry-After cooldown;
- deterministic buffered visible execution.

```text
cargo check --manifest-path perf/Cargo.toml --all-targets
cargo bench --manifest-path perf/Cargo.toml --bench managed_client
cargo bench --manifest-path perf/Cargo.toml --bench generated_client
cargo bench --manifest-path perf/Cargo.toml --bench request_bodies
cargo bench --manifest-path perf/Cargo.toml --bench visible_execution
cargo bench --manifest-path perf/Cargo.toml --bench native_paths
```

The maintained package contains no historical transport-polymorphism or
Concord retry-loop benchmarks.
