# Planning, reflection, and observation

Concord distinguishes static contract facts, inert value-bound plans,
result-determining gates, fixed completion evidence, public errors, and trusted
diagnostics.

## Static descriptions

`Client::contract()` returns the client/scope/endpoint tree without runtime
values. Request and unbound intent `.description()` return the same endpoint
view. Descriptions expose docs/deprecation, value owners and opaque type spelling,
presence/default/observation, attachment operations and origins, body/response
contracts, auth categories, timeout, repeatability, rate schemas, pagination,
capabilities, and identity stability. They never expose credential material.

Trait-owned behavior is identified as a declared capability or opaque extension
rather than invented static precision.

## Credential-free plans

`request.plan()` borrows an inert request. `client.plan(&intent)` does the same
after binding an unbound intent to public client values. Planning uses the same
bounded public shaping operations as execution and exposes endpoint identity,
method, redaction-aware observed URL/headers, timeout, repeatability, body/
response categories, rate-policy identities, and terminals.

Planning never performs I/O, calls providers, places authentication, acquires a
limiter, invokes execution gates/observers, inspects/materializes body values,
or allocates operation/attempt identity. Body information is static metadata
only.

`CredentialFreeRequestPlan` makes one exact guarantee: credentials are absent.
Visible ordinary public data may still be personal or operationally sensitive,
and `observed_url()` may differ from the real public wire URL when redaction is
declared.

## Observation classes

Ordinary public values may be visible. `#[redact]` selects redacted public
observation while preserving real wire/internal identity. Secrets/credentials
are a private class structurally excluded from public representations. Structured
sanitized headers and fixed redacted markers prevent safe accessors from leaking
values that `Debug` merely hides.

## Operational evidence

Gates receive credential-free/redacted bounded views and may reject at one named
phase. Completion receives one already-fixed bounded event and cannot change the
caller result. Rate limiters receive opaque canonical keys under a documented
application trust boundary; key values are absent elsewhere.

Public errors expose stable categories, endpoint/method, operation/attempt/page
identity, stage, and dispatch knowledge where applicable. Ordinary formatting
excludes bodies, credentials, dependency details, and redacted values. A
separate trusted diagnostic sink can receive bounded source-chain information;
applications remain responsible for visible public data.

Semantic contract snapshots are deterministic tool-private data. `xtask` diff
classifies caller API, value/domain, wire, security/auth, runtime policy,
capability, identity/rate schema, and documentation-only changes without
promising a public JSON format.
