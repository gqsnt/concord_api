# Development Test Ownership

The `dangerous-dev-tools` boundary does not gate Concord's whole core
integration suite. Tests that previously reached broad runtime types through
`concord_core::__development` now have these owners:

- **Public behavior (default and all-feature suites):** native Reqwest
  execution, pagination, rate limiting, redaction, response limits, retry
  modes, runtime configuration, and the public portions of runtime ordering.
  These tests use public APIs for application extension points and the
  generated integration contract only where they model generated endpoints.
- **Internal/generated-contract invariants (default and all-feature suites):**
  request-entity preparation and the shared generated-plan harness. These use
  `concord_core::__private` as generated code does; no runtime engine is
  re-exported through the development seam.
- **Explicit development-seam behavior (all-feature suite):** enabled response
  capture, exact authentication lifecycle observations, and opaque credential
  generation comparisons. Only these assertions reference
  `concord_core::__development`, and every reference is locally guarded by
  `dangerous-dev-tools`.

The default suite still executes the public authentication-recovery and
runtime-order scenarios. The explicit feature adds classification, response
release, and exact-generation invalidation observations to those same
scenarios rather than replacing their public assertions. Response-release
observation uses the callback captured from the provider binding prepared for
that execution; instrumentation never resolves the application binding again.
Every recognized challenge passes through one body-free response-release
transition before credential mutation. The transition filters captured
targets by the credential, usage, and step identities retained in the
validated rejection plan, so a discarded or unrelated prepared binding sees
no classification or release event.
Generation observations expose equality-only identities and never format or
return the underlying counter.
