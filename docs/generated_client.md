# Generated client surface

The generated Rust API mirrors the declaration tree and keeps fields private.

## Client construction

`Client::builder()` is canonical. Required client values use exact `set_x(T)`
methods; optional values use `set_x(Option<T>)`, `with_x(T)`, and `clear_x()`;
defaulted values add `reset_x()`. Secrets accept exactly `SecretString`. Custom
providers and an explicit limiter are required when the contract needs them.

`.retry(policy)`, `.configure_http(...)`, and `.configure_runtime(...)` keep
transport and operational configuration cohesive. `build()` deterministically
reports missing input categories and never panics for ordinary configuration.

`Client::new()` exists only when no required client value, secret, custom
provider, limiter, or other runtime choice is missing.

## Scopes and endpoints

A required non-defaulted scope value appears on its domain method:
`client.regional(region)`. Optional/defaulted scope controls live on the returned
scope, so ownership stays visible before selecting a child.

An endpoint method takes required non-defaulted ordinary values and body inputs
in declaration order. Optional/defaulted endpoint values become controls on the
request snapshot. Required endpoint values have no public setter; construct a
new endpoint intent to change them.

Generated controls consume `self` and return an immutable new snapshot. `clear`
and `reset` are distinct whenever a retained optional default is present.

## Request and terminals

Requests are `#[must_use]`, body-free in `Debug`, and inert until polled. Every
endpoint supports await for its declared output and `.response().await` for
`ApiResponse<Output>`. Only declared capabilities add `.accepted_bytes()`,
`.paginate(limit)`, or credential storage terminals.

Every request also supports `.timeout(duration)`, `.disable_timeout()`, and
`.inherit_timeout()`. Request-level controls do not duplicate ancestor value
controls.

## Unbound tree

`Client::unbound()` starts an isomorphic tree with scope/endpoint values and body
input but no client values, credentials, transport, limiter, gates, or runtime.
An intent is storable, passable, describable, and inert.

`client.plan(&intent)` creates a credential-free plan without consuming it.
`client.request(intent)` returns the same named request shape as direct
construction. Direct and unbound-bound paths share identity, values, attachments,
capabilities, and executor behavior.

## Reflection

`Client::contract()` returns the immutable value-free contract tree. Request and
intent `.description()` return the same endpoint view. Docs, deprecation,
presence/default/observation, type spelling, attachment origins, body/response
contracts, auth categories, timeout, repeatability, rate schema, pagination, and
terminals derive from the same resolved IR as generated code.

The facade provides narrow modules for authentication, body/codec, endpoint,
error, HTTP, observation, pagination, rate limiting, response, retry, and
runtime configuration. Ordinary generated use does not require a broad prelude.
