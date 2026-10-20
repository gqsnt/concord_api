# Concord V3-only product boundary

## Selected product

The owner's latest instruction selects V3 as the only supported Concord
version. The implementation, grammar, runtime state and generated caller API
follow that decision. The former conversion component and its requirements are retired; a converter
is not part of the finished V3 product.

[ADR 0027](../../adrs/0027-v3-only-product.md) and the
[version policy](../../../docs/version_policy.md) are the active product
decisions. This instruction governs the active implementation and release scope.

## Removed surfaces

| Former surface | Current behavior |
|---|---|
| V2-to-V3 conversion command and implementation | Removed; the ordinary xtask dispatcher rejects migrate-v3 |
| Conversion-only tests, examples, fixtures and guides | Removed from active source |
| Older declaration spelling accepted as an alternative | Rejected by the V3 parser/compiler contracts |
| RequestBody compatibility alias | Removed from public exports; PreparedBody owns V3 preparation facts |
| ApiError::stage() compatibility accessor | Removed; failure_stage() is the V3 accessor |
| catalog-v3 registry alias | Removed; catalog is the canonical V3 target |
| concord.contract/1 and /2 input formats | Rejected with the contract CLI's format-error status |
| Older-binary shared-store import markers/protocol | Removed; registration and admission validate the current V3 schema |
| Former V2 performance declaration in maintained tools/perf | Removed; maintained probes use V3 |

The first removal inventory covered 96 paths and 42,569 removed lines before
fixture relocation and other changes. The 234 conversion-only tests removed
with their unsupported component do not count as a coverage improvement.
The dated correction report preserves that accounting.

## V3 capabilities retained

Native V3 extension roles remain: EncodeBody, DecodeBody, DisplayAtom,
PreparedBody, explicit representation selection, request controls, managed
streaming, pagination, runtime endpoints, inspection and current shared-store
conformance. API authors can still deprecate their own values or members.
RetryFailure::stage() remains a distinct V3 calculation; removing the
ApiError compatibility accessor does not remove it.

Authored provider paths such as /v2 identify the remote API's routes. They do
not select a Concord V2 runtime. A generated-code bridge version and the
concord.contract/3 format likewise identify current contracts, not support for
an older Concord product.

The V3 Catalog rate-key oracle preserves the previous V3 bytes. The Riot
recording changes only the declared User-Agent to ConcordRiotExample/3;
46 endpoints, 92 origin contexts, 28 policies and the eight contract-edit
controls retain their current wire/key/schema/identity checks.

## Historical material

Two explicitly export-ignored history files remain unchanged as Git blobs.
The maintained source-export excludes them. The source-and-qualification return includes
their exact original bytes and identifies them separately in its provenance
and source manifest. Retaining historical records does not restore an API,
converter or storage-import path.

The performance campaign compares the current product with an isolated
historical worktree to document the cost of the
current architecture. Historical syntax exists only in that measurement worktree.
The checked-out product and maintained probe sources are V3-only. External
dependencies and comparable wire inputs were aligned for the measurement;
the historical reference is not a supported deployment mode.

## Verification

The compiler/UI suites reject the removed public aliases and unsupported
declaration forms. The final native return additionally exercises the actual
xtask binary: canonical target listing, four deterministic V3 exports,
same/different checks, semantic text/JSON diffs, old-format rejection and alias
rejection. A structural difference intentionally exits 1. Contract usage or
format failures intentionally exit 2. The outer dispatcher rejects the removed
migration command with exit 1 and usage text. These nonzero outcomes retain
their numeric status in the receipts.

No compatibility shim is proposed as remaining V3 work.
