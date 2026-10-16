# V3 version policy

Concord V3 is the only supported product version in this workspace. The public
language, generated Rust API, runtime, inspection tools, examples and validation
fixtures target V3 directly.

## Supported surface

Use the canonical language in [the DSL specification](spec.md), the generated
controls in [Requests and values](requests_and_values.md), and the
`concord.contract/3` inspection format. There is one managed execution path and
one canonical grammar.

Old syntax is rejected. A diagnostic may explain the valid V3 spelling, but
does not execute an older language or silently translate its meaning. There
are no V2 compatibility aliases, older-runtime selection switches, automatic
V2-to-V3 converter, or legacy-state import protocol.

Applications declare their clients against V3 and use V3 services and state
schemas. Shared limiter state must have the current explicit schema; Concord
does not certify state produced by older Concord binaries.

## Extension contracts

`PreparedBody`, `EncodeBody`, `DecodeBody`, `RequestRepresentation` and
`ResponseRepresentation` are native V3 extension contracts. Their documented
trait adapters implement the selected V3 body and response model. An API
author can deprecate one of their own values or endpoints within V3; this does
not select an older Concord version.

## Decision authority

The owner's 16 October 2026 instruction selects this scope and supersedes
earlier requirements to deliver a V2-to-V3 migration tool or cross-version
state continuity. [ADR 0027](../dev_doc/adrs/0027-v3-only-product.md) records
the affected engineering obligations. Earlier Git commits and review records
retain their original evidence scope.
