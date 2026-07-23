# Security and redaction

Authentication material and native transport objects stay inside the managed
runtime. Public URL/header shaping accepts only restricted public expressions;
secrets and credential references cannot enter those paths.

Safe surfaces include:

- endpoint descriptions and request plans;
- hooks and completion events;
- errors and build diagnostics;
- response-head observation;
- rate-limit plans and keys;
- `Debug` output for public runtime values.

These surfaces contain endpoint identity, public method/URL/header projections,
body category and length hints, timeout selection, status category, and bounded
timing where appropriate. They do not contain credentials, protected
authentication headers/query values, provider payloads, native requests or
responses, response bodies, or body-factory output.

Planning is deliberately not an exact request preview. It builds the real
public head, then stops before hooks, credentials, limiter acquisition, body
encoding/factories, stream polling, transport, and completion.

Errors use stable semantic categories and reduced safe context. Secret-bearing
builders and provider errors implement redaction-aware formatting. Sentinel
tests cover URL, headers, plans, descriptions, hooks, errors, debug,
completion, limiter metadata, provider failures, and status bodies.

Dangerous capabilities are separately feature-gated and remain bounded. Their
reduced guarantees are documented in
[Dangerous capabilities](../reference/dangerous.md).
