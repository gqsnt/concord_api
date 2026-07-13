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

The standalone `perf` crate contains native targets for:

- managed client construction across all retry modes;
- visible execution overhead;
- bounded authentication recovery and credential caching;
- pagination;
- native streaming upload/response;
- rate-limit governor and cooldown processing;
- redaction/hooks and allocation reporting.

```text
cargo check --manifest-path perf/Cargo.toml --all-targets
cargo bench --manifest-path perf/Cargo.toml --bench retry_modes
cargo bench --manifest-path perf/Cargo.toml --bench auth_runtime
cargo bench --manifest-path perf/Cargo.toml --bench streaming_upload
cargo bench --manifest-path perf/Cargo.toml --bench streaming_response
cargo bench --manifest-path perf/Cargo.toml --bench rate_limit_governor
```

Machine-local historical reports may mention the removed Concord attempt loop;
they are measurements of earlier architecture, not current API guidance.
