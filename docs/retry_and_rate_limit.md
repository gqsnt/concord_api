# Retry And Rate Limit

General HTTP retry is a managed-client construction choice. Reqwest is the
only general retry executor; Concord owns no status/transport attempt loop,
retry classifier, delay, admission registry, or endpoint retry policy.

## Retry modes

Generated clients expose `new_with_retry_mode(...)` and
`new_with_safe_reqwest_builder_and_retry_mode(...)`. The public modes are:

- `RetryMode::ProtocolRecovery` (default): installs no custom Reqwest retry
  policy and preserves Reqwest 0.13.4's built-in safe protocol recovery.
  Concord does not promise its internal budget or physical-send count.
- `RetryMode::Disabled`: installs `reqwest::retry::never()`. Every visible call
  to `reqwest::Client::execute` maps to exactly one wire request.
- `RetryMode::Status(StatusRetryConfig)`: replaces default protocol recovery
  with one Reqwest custom policy scoped to a descriptor-verified fixed host.

`StatusRetryConfig::new(max_retries, statuses)` accepts only:

- `max_retries` in `1..=2`;
- a non-empty status set containing only `502`, `503`, and `504`.

The classifier is internally limited to `GET`, `HEAD`, and `OPTIONS`. It never
status-retries `401`, `403`, `429`, or an unsafe method. A configured `503`
retry is immediate; Reqwest does not honor that response's `Retry-After` for
the hidden resend.

Status mode is rejected before request, provider, or body side effects unless
the versioned generated descriptor says the whole API is fixed single-origin.
Dynamic hosts, multiple origins, hostless origins, cross-origin-capable
pagination, and hand-written contexts without equivalent verified metadata are
ineligible. Redirects remain disabled.

```rust
use concord_core::prelude::{RetryMode, StatusRetryConfig};
use http::StatusCode;

let mode = RetryMode::Status(StatusRetryConfig::new(
    2,
    [StatusCode::BAD_GATEWAY, StatusCode::SERVICE_UNAVAILABLE],
)?);
```

Retry syntax is not part of the API DSL. Removed `retry`, `max_attempts`,
method/status/transport lists, idempotency declarations, and `retry_after`
retry switches produce a compile error directing callers to client-level
`RetryMode`.

## Visible executions and physical sends

A visible execution is one Concord call to `reqwest::Client::execute`.
Pre-send hooks and rate-limit acquisition run once per visible execution;
post-response hooks observe the final result returned by Reqwest. Reqwest's
internal resends are visible on the wire but do not rerun Concord hooks,
authentication preparation, or rate-limit acquisition.

Concord retains at most one explicit authentication recovery. That recovery
reconstructs a fresh native request and is a second visible execution. Each
visible execution independently receives the selected Reqwest policy.

Physical-send bounds are:

| Mode/body | Without auth recovery | With auth recovery |
| --- | ---: | ---: |
| `Disabled` | 1 | at most 2 |
| `Status`, Reqwest-cloneable body, `max_retries = R` | at most `1 + R` | at most `2 × (1 + R)` |
| Reqwest-uncloneable but Concord-rebuildable body | 1 | at most 2 |
| Concord-non-rebuildable body | 1 | 1 |
| `ProtocolRecovery` | Reqwest-owned bound | Reqwest-owned bound per visible execution |

Pagination applies the same rules independently to each page. Hidden-send
counters and physical-attempt indices are not public API.

Concord rebuildability is used only for authentication recovery. Reusable
bytes are both rebuildable and Reqwest-cloneable. Factory streams, advanced
bodies, and multipart may be rebuildable for authentication recovery while
their materialized Reqwest bodies remain uncloneable. Direct streams and
advanced one-shot bodies are neither buffered nor replayed. Multipart is never
flattened or made Reqwest-cloneable; an all-reusable multipart can perform one
authentication recovery, which creates a fresh boundary.

## Retry-After and cooldown

`Retry-After` never causes Concord to resend the current call. For a final
`429 Too Many Requests`, the default rate-limit observer parses delta-seconds
or an HTTP date, caps a positive value with `max_rate_limit_cooldown`, stores a
cooldown for future calls, and returns the final 429 response/error. Past dates
have no positive delay; malformed or unsafe values are ignored.

The default governor acquires any stored cooldown before a later visible
execution. Its finite cooldown-entry cap and pruning behavior remain
independent safety controls.

## Rate-limit DSL

Rate limiting remains an API-specific policy and may be declared and attached
through profiles:

```rust,ignore
policies {
    rate_limit app {
        bucket application by [host] {
            100 / 1s
        }
    }
    observe rate_limit MyResponseObserver
}

profiles {
    profile read {
        rate_limit app
    }
}
```

Rate-limit acquisition follows credential preparation and precedes sanitized
pre-send hooks and secret materialization. Response observers receive
sanitized headers and may install future-call cooldowns; they cannot authorize
a resend of the current call.
