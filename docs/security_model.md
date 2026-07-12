# Security Model

This document is the safe-consumer guide for Concord V1. It describes the public surfaces Concord intends to keep safe by default, the explicit extension points that stay under caller control, and the dangerous escape hatches that can expose raw body bytes or local capture files when deliberately enabled.

Concord is designed to keep the ordinary path narrow:

- use generated clients and request builders for normal API calls;
- use `.await` or `.execute().await` for the decoded endpoint value;
- use `.response().await` when buffered endpoints need decoded value plus response metadata;
- use pagination with explicit `.paginate(...).collect()` when a request is paginated;
- import `concord_core::prelude::*` for normal generated-client use.

## Surface Map

### Safe public surfaces

The normal safe surface is the generated client plus `concord_core::prelude`.

That surface includes:

- generated client constructors and facade methods;
- request builders and pending requests;
- decoded request terminals such as direct `.await` and `.execute().await`;
- `.response().await` for buffered endpoints that need status, headers, url, and metadata alongside the decoded value;
- pagination collection through `.paginate(...).collect()`;
- ordinary request/response codec markers and user-facing configuration types that are not marked dangerous.

These surfaces are intended for application code. They do not intentionally expose raw auth secrets or raw response body bytes.

### Advanced extension surfaces

`concord_core::advanced` is for stable extension and customization points. Typical examples are:

- custom transports;
- custom codecs and entity markers where supported;
- retry and rate-limit policies or helpers;
- auth providers and auth materialization helpers where supported;
- hooks and debug sinks that intentionally receive sanitized metadata views;
- pagination controllers and other explicit extension hooks;
- metadata-bearing response types such as `DecodedResponse` where needed.

Advanced surfaces remain safe only within their contract. They are caller-controlled extension points, so Concord does not promise to prevent every leak once user code inspects metadata, constructs custom codecs, or runs custom transport logic.

### Dangerous escape hatches

`concord_core::dangerous` contains explicit escape hatches. They are not part of the normal user path.

The dangerous feature gates are:

- `dangerous-raw-response`
- `dangerous-dev-tools`

These enable, respectively:

- raw response access through `BuiltResponse` and `.execute_raw_response()`, which can return raw response body bytes before endpoint decode;
- deprecated dev body capture through `DevBodyCaptureConfig` and `RuntimeConfig::dev_body_capture(...)`, which can write selected raw response bytes to local disk when explicitly configured.

These features are intended for controlled diagnostics, protocol testing, and local debugging. They should not be treated as the default application surface, and they should not be enabled in production unless that risk is intentionally accepted.

### Generated-code-only plumbing

`concord_core::__private` is generated-code-only plumbing.

It exists so macro-generated code has stable paths for request planning, response planning, endpoint internals, and other implementation details that are not intended as a public user API. Normal application code should not import it.

The compatibility alias `concord_core::internal` is not the preferred path. Generated code should use `__private`.

## Secret Handling

`SecretString` redacts both `Debug` and `Display`.

Intentional secret exposure is explicit at the call site through `expose_secret()`.

Concord-generated docs and public diagnostics are designed to avoid rendering raw secret literals. If user code calls `expose_secret()` or otherwise moves secret material into custom code, that material is under caller control.

Concord does not promise cryptographic secrecy, memory zeroization, or process isolation beyond what is already implemented by the runtime and standard Rust ownership rules.

## Body-Byte Handling

By default, Concord keeps raw body bytes out of the ordinary diagnostic path.

The following surfaces are metadata-only or body-free by design:

- debug sinks;
- runtime hooks;
- retry contexts;
- rate-limit contexts;
- public errors;
- generated endpoint rustdoc.

Body-size limit failures remain typed and body-free. Diagnostic surfaces may mention the failing endpoint, status, limit, or safe header metadata, but they do not receive raw body bytes.

The standard `DynBody` path preserves HTTP data and trailer frames while using
`Bytes` and the redacted `BodyError` type. Send-only streams and readers are
adapted with safe exclusive synchronous polling; no unsafe trait adaptation,
background forwarding task, or unbounded queue is involved. Its reusable
frame-aware limiter counts data bytes only, so trailer fields do not consume a
byte budget. Legacy transport conversion is temporary and remains a
byte-stream-only boundary until the transport migration.

The dangerous surfaces are the exception:

- raw response execution can expose raw response body bytes through the returned built response;
- dev body capture can write selected raw response bytes to local disk when explicitly configured.

Neither feature is enabled by default.

## Auth Material Handling

Raw auth material is materialized at the transport boundary.

The runtime checks auth collisions before rate-limit acquisition, hooks, debug, and transport side effects. Protected auth responses stay out of normal diagnostics, and auth-specific handling remains separate from ordinary metadata surfaces.

Auth values are still caller-controlled if user code intentionally exposes them or passes them through custom extension points.

## Runtime Order

Concord's runtime order is fixed at a high level:

1. plan the request;
2. check auth collisions and materialize auth requirements;
3. acquire any rate-limit resources;
4. run hooks, debug, and transport send logic;
5. classify the response or transport failure;
6. handle auth rejection;
7. observe retry and rate-limit behavior;
8. decode the endpoint response.

The exact implementation is intentionally internal, but the order above is the contract to rely on.

## Retry And Rate-Limit Safety

Retry and rate-limit behavior remain bounded.

- retries are bounded by the configured policy and runtime caps;
- non-empty declared rate-limit plans fail closed when the governor cannot provide enforcement;
- retry and rate-limit metadata stay body-free and auth-secret-free.

## Generated Rustdoc Safety

Generated endpoint rustdoc describes the effective endpoint contract using names and metadata only.

It should not render raw secret values, raw body bytes, or caller-chosen codec internals. The docs are intentionally a safe summary, not a dump of runtime internals.

## Consumer Guidance

- Use `prelude` for normal generated-client code.
- Use `advanced` only when you intentionally need an extension point.
- Use `dangerous` only for local diagnostics or protocol testing with controlled handling.
- Do not enable dangerous features in production unless you have intentionally accepted the risk.
- Do not upload or store logs, screenshots, artifacts, or bundles that may contain raw response bytes from dangerous or dev capture paths.
