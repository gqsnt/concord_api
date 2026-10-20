# ADR 0006 — Prepare bodies once per direct/page request

- **Status:** Superseded for v3 by [ADR 0018](0018-single-prepared-body-authority.md); the once-per-direct-request-or-page preparation rule is carried forward unchanged.
- **v2.1 status (historical):** Selected

## Decision

Body representation prepares once per direct request or page, then reconstructs within that page’s visible attempts. `PageStableInput` does not imply aggregate prepared-body reuse.

## Property earned

The contract matches proven page stability, keeps arbitrary prepared-resource lifetime bounded, and avoids silently strengthening extension obligations.

## Ordering

Prepare before credential/rate admission so deterministic local errors do not spend remote quota. Create attempt-only resources as late as possible through the prepared-body contract.

## Mechanical boundary

The initial generated shape attempted to share a conditional `Clone` path
between ordinary and paginated body endpoints. Stable Rust still has to prove a
concrete generated where-clause when that implementation is emitted; for a
one-shot `RequestStream` the obligation is false even though direct execution
does not need reconstruction. The compiler counterexample is retained in
`non_clone_body_endpoint`, while `pagination_non_clone_input` proves that the
same non-`Clone` input is correctly rejected when page reconstruction is
required.

The selected private shape therefore emits no unconditional body-input
`Clone` bound for ordinary execution and keeps `Input: Clone` behind the
pagination-only `PageRequestRepresentation` capability. This revises only the
generated trait shape; exact body input and replay semantics are unchanged.

## Deferred

`AggregateReusablePreparedBody` requires an explicit stronger lifetime/replay contract and benchmark evidence that per-page preparation is material.
