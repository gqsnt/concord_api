# ADR 0010 — Optional initial static secret, required custom provider

- **Status:** Selected for prototype; real multi-auth gate pending

## Decision

Permit `secret name?;` to model optional initial protected material. A derived slot is unavailable when absent. Require custom provider objects; provider acquisition may report unavailable. Endpoint-backed slots start empty by source semantics without `?`.

## Property earned

A client can serve public endpoints before optional static token material is installed without silently making auth placement optional.

## Invariant

If an endpoint declares a placement and material is unavailable, execution fails before dispatch. It never drops auth.

## Revisit

If optional secrets confuse client construction or real APIs need mutually exclusive auth families, prototype named least-authority client profiles. Do not use optional punctuation as a substitute for a full capability model when that model is actually needed.

