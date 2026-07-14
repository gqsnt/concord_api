# Security Model

This document is the safe-consumer guide for the current Concord release. It describes the public surfaces kept safe by default, the explicit extension points under caller control, and the dangerous raw-response escape hatch.

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

- user-authored codecs;
- custom codecs and entity markers where supported;
- client-level retry modes and rate-limit policies or helpers;
- auth providers and auth materialization helpers where supported;
- hooks and debug sinks that intentionally receive sanitized metadata views;
- pagination controllers and other explicit extension hooks;
- native streaming and multipart body inputs;
- safe managed-client configuration.

Advanced surfaces remain safe only within their contract. Concord cannot prevent leaks introduced by user-authored codecs or callbacks that inspect metadata.

### Dangerous escape hatches

`concord_core::dangerous` contains explicit escape hatches. They are not part of the normal user path.

The dangerous feature gates are:

- `dangerous-raw-response`
- `dangerous-dev-tools`

These enable, respectively:

- raw response access through `BuiltResponse` and `.execute_raw_response()`, which can return raw response headers and body bytes before endpoint decode;
  - the narrow `__development` lifecycle-observation seam used by deterministic tests.

These features are intended for controlled diagnostics, protocol testing, and local debugging. They should not be treated as the default application surface, and they should not be enabled in production unless that risk is intentionally accepted.

`concord_core::__development` is additionally available only with
`dangerous-dev-tools`. It is a hidden, unstable observation seam for Concord's
deterministic tests, not an alternate transport or a normal debug-build API.
Without the explicit feature it does not exist—even under `debug_assertions`.
Its narrow observations do not make credential cache, body engine, response
entity, or request execution error types public.

### Generated-code-only plumbing

`concord_core::__private` is generated-code-only plumbing. Descriptor and
authentication-binding integration uses this single current namespace. It is
not a stable reflection, transport, middleware, or authentication-executor
API; its public visibility exists only so macro expansions compile across
crate boundaries.

It exposes opaque descriptors and preparation adapters, not Core runtime-plan
structs. Generated code emits resolved facts, and Core constructs and executes
the runtime plan behind `GeneratedPreparedCall`. Normal application code
should not import this namespace. Because cross-crate macro expansion requires
public reachability, `#[doc(hidden)]` is an unsupported-integration boundary,
not a claim that downstream Rust code is technically unable to call a symbol.

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
- execution hook metadata;
- rate-limit contexts;
- public errors;
- generated endpoint rustdoc.

Body-size limit failures remain typed and body-free. Diagnostic surfaces may mention the failing endpoint, status, limit, or safe header metadata, but they do not receive raw body bytes.

Reusable bytes, streaming inputs, and multipart recipes are converted directly
to their native Reqwest capabilities. Buffered responses use bounded native
collection and streaming responses retain native lazy delivery. No universal
public body or response bridge is part of the final surface.

The dangerous surface is raw response execution, which can expose sensitive raw
response headers and body bytes through the returned built response. Its
`url()` remains Concord's logical pre-authentication request URL. The current
escape hatch does not expose `reqwest::Response` or its native materialized URL;
any future capability that does so must be separately feature-gated and treated
as able to reveal redirect/native URL state and authentication query values.

Neither feature is enabled by default.

## Auth Material Handling

Raw auth material is materialized only in the native request immediately before managed-client execution.

Core is the authentication lifecycle authority. Generated clients declare
typed credential identifiers and provider bindings, but core sequences cache
lookup, coalesced acquisition or refresh, generation-aware invalidation,
secret material planning, and final header/query insertion. Generated code
does not run provider or credential-cache loops. Provider HTTP remains a
bounded, separate operation from the protected endpoint transport call.

The runtime checks auth collisions before rate-limit acquisition, hooks, debug, and transport side effects. Protected auth responses stay out of normal diagnostics, and auth-specific handling remains separate from ordinary metadata surfaces.

Auth values are still caller-controlled if user code intentionally exposes them or passes them through custom extension points.

## Runtime Order

Concord's runtime order is fixed at a high level:

1. plan the request;
2. derive secret-free auth placements and validate public collisions;
3. acquire credentials and materialize the execution body;
4. acquire any rate-limit resources;
5. run sanitized hooks and debug output;
6. materialize authentication and immediately invoke transport;
7. observe rate-limit feedback;
8. optionally perform one authentication recovery through the same visible-execution sequence;
9. decode or return the terminal response.

The exact implementation is intentionally internal, but the order above is the contract to rely on.

## Retry And Rate-Limit Safety

Reqwest is the sole general retry executor. `RetryMode` is fixed when the
managed client is constructed: protocol recovery uses Reqwest defaults,
disabled mode installs `retry::never()`, and status mode permits only bounded
502/503/504 retries for GET, HEAD, and OPTIONS on verified fixed-origin APIs.
Hidden Reqwest resends reuse the materialized native request and do not rerun
Concord hooks, authentication preparation, or rate-limit acquisition.

Concord retains one bounded, visible authentication recovery when the logical
body can be rebuilt. Non-empty declared rate-limit plans fail closed when the
governor cannot enforce them. A final 429 may install capped cooldown for a
future call, but never causes Concord to resend the current call. Execution and
rate-limit metadata remain body-free and auth-secret-free.

## Generated Rustdoc Safety

Generated endpoint rustdoc describes the effective endpoint contract using names and metadata only.

It should not render raw secret values, raw body bytes, or caller-chosen codec internals. The docs are intentionally a safe summary, not a dump of runtime internals.

## Consumer Guidance

- Use `prelude` for normal generated-client code.
- Use `advanced` only when you intentionally need an extension point.
- Use `dangerous` only for local diagnostics or protocol testing with controlled handling.
- Do not enable dangerous features in production unless you have intentionally accepted the risk.
- Do not upload or store logs, screenshots, artifacts, or bundles that may contain raw response bytes from dangerous execution paths.
