# Design invariants

This file defines invariants for maintainers and extension authors.

## One executor

Generated direct requests, explicit endpoints, runtime-defined endpoints,
provider endpoints, retries, pagination pages, and response-byte terminals use
one managed executor.

No public API exposes a second native HTTP path.

## Syntax-neutral runtime

The macro resolves DSL syntax into semantic data and generated Rust. The
runtime receives typed request plans and has no parser dependency.

Concrete codecs, providers, pagination controllers, limiters, observers, and
hooks participate through traits rather than macro name recognition.

## Public before private

Public request shaping, safe planning, hooks, and limiter admission occur
before private authentication placement.

Secrets and credentials cannot enter public expressions or rate-limit keys.
The native authenticated request is not publicly inspectable.

## Finite behavior

Every retained collection, body, stream, provider response, pagination state,
rate-limit state, retry sequence, and diagnostic capture has a finite bound.

Known excess fails before retention. Unknown excess withholds the first excess
unit.

## Immutable snapshots

Client, scope, endpoint, and request values are immutable snapshots. Generated
controls consume one snapshot and return another.

Client clones share one runtime snapshot. Independent builds share state only
through explicitly shared extension objects.

## Capability-shaped API

Generated methods correspond to declared and trait-proven capabilities.
Unsupported terminals are absent or fail a sealed trait bound.

## Safe observation

Descriptions, plans, hooks, limiters, retry classifiers, completion, errors,
and automatic formatting expose purpose-specific semantic data.

Actual response values and headers are returned only through explicit
application accessors.

## Deterministic identity

Endpoint and rate-limit IDs are explicit or deterministically derived.
Pagination identity is canonical and credential-free.

Ordered authentication placements, rate-limit policies, dimensions, and
windows retain source order.

## Ownership

The macro owns syntax and compile-time structural diagnostics.
Core owns semantic runtime contracts and execution.
Extras own optional first-party implementations.
The façade owns application imports, feature composition, and generated-path
handoff.

## Change rule

v1 is frozen. A change may fix an implementation defect or documentation
error without adding public syntax, generated capabilities, or runtime escape
paths.
