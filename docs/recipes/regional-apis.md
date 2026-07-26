# Regional APIs

## Goal

Build one client whose endpoint families select different regional hosts while
sharing authentication and runtime state. Callers should choose a region as a
typed scope value instead of rebuilding the client or concatenating URLs.

## Complete code

The compiled contract uses final host shaping, inherited authentication, and
regional scopes:

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/regional_scopes.rs:regional_api_recipe}}
```

One built client can construct requests for several host families:

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/regional_scopes.rs:regional_api_usage}}
```

The example is construction-only because its `.test` hosts are reserved. The
same module is imported by the example crate's construction test.

## Why these declarations

`RegionalGameApi` owns inputs and policy common to the service: the protected
API key, fixed base domain, application path, and client-level rate rule.
`platform` is a client value because it selects the initial client-wide host
shape.

Each `accounts`, `matches`, or `league` scope takes its required `region` as a
navigation argument. Its `host [...]` clause replaces inherited generated
labels with the final host family, while its path establishes the prefix
shared by child endpoints. A single call therefore reads in domain order:

```rust
client
    .matches("europe".to_owned())
    .by_id("match-42".to_owned())
```

The scopes remain inert values. Entering one does not create a transport,
fetch credentials, or contact a service. Every bound request uses the
immutable runtime, credential slot, hooks, and limiter shared by the original
client.

Authentication is declared once on the client and inherited privately.
Regional values shape only the public request. The endpoint-local rate rules
compose with the client rule without turning routing values into credentials.

## Adaptations

Replace `String` routing inputs with application enums that implement stable
`Display` when the set of routes is known. This prevents arbitrary operator
text from becoming a DNS label and makes invalid routing choices
unrepresentable.

Use nested scopes when a route family has another stable grouping, such as
region → account → resource. Keep a value at the smallest scope that owns it;
required values remain visible as navigation arguments.

If only a public path or query value varies, keep the fixed host and shape
that value at the scope or endpoint. Use `RuntimeEndpoint` only when the
operation shape itself comes from runtime data. A region chosen at runtime is
not, by itself, a reason to give up a generated scope.

When a regional listing paginates, bind only the controller-owned endpoint
values. Static region values remain fixed across pages and may safely
participate in non-pagination host shaping.

## Production notes

Use a real DNS-domain fixed base before enabling nonempty `host [...]`
shaping; host labels cannot be prepended to an IPv4 or IPv6 literal. Validate
the service's exact region-to-host mapping and do not treat the example rate
windows as another service's quota contract.

Keep API keys in `SecretString` and private `auth` placements. The example's
public `X-Developer-Key` value is explicitly non-secret demonstration data;
never copy protected material into a public header.

Build one client per stable deployment configuration and clone it. If quotas
must coordinate across independently built clients or processes, install the
appropriate shared or distributed limiter explicitly. Test each route family
with safe plans and deterministic local transport rather than live service
calls.

## Related

- [Clients, scopes, and endpoints](../guide/clients-scopes-endpoints.md)
- [Shape the public request](../guide/request-shaping.md)
- [Authentication](../features/authentication.md)
- [Rate limiting](../features/rate-limiting.md)
- [Runtime-defined endpoints](../features/runtime-endpoints.md)
