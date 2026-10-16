# Concord V3 status summary

Concord V3 is the only supported product version. This tree carries
`3.0.0-beta.1`, with `publish = false` in every package.
The language, generated client, managed runtime, extensions and inspection
operate directly on the V3 contract.

The [16 October correction report](dev_doc/reviews/2026_10_16_corrections.md)
records the direct follow-up to the [source review](dev_doc/reviews/2026_10_16_review.md).
The reviewed provider-deadline, credential-installation, exact-body-length,
multipart and generated-name failures have code corrections and targeted
native regressions. The conversion finding is closed by retiring the V2-to-V3
component under the owner's explicit V3-only decision.

Final integrated qualification and performance results are owned by the
[release-gate status](dev_doc/release_gate_status.md). Historical R7/R8 and
`ff1be9c` results qualify only their recorded inputs. Human studies and future
publication decisions remain separate from implementing and testing this beta.

## Contract and source map

- **Authority:** the authorized Edition 1 implementation specification,
  reconciled with the owner's 16 October V3-only instruction; the private
  specification and prompt are not vendored here.
- **Current version boundary:** [V3 policy](docs/version_policy.md) and
  [ADR 0027](dev_doc/adrs/0027-v3-only-product.md).
- **Normative repository documents:** [design invariants](docs/design_invariants.md),
  [DSL specification](docs/spec.md) and [execution policy](docs/execution_policy.md).
- **Maintainer maps:** [internals](dev_doc/internals.md),
  [repository responsibilities](dev_doc/repository_change_map.md),
  [public contract inventory](dev_doc/public_change_matrix.md) and
  [claim/evidence ledger](dev_doc/claim_evidence_ledger.md).

## Selected design

A single closed declaration produces typed client/scope/endpoint surfaces.
Bare values belong to their current owner and ancestors are explicitly named.
Payload assignment, exact optional replacement, conditional assignment,
omission and retained reset have distinct controls. Public head programs
select surviving occurrences before encoding, and sensitivity remains sticky.

Representations are directional. `PreparedBody` owns actual body media, length
and replay facts. Every operation enters the managed executor, whose journal
retains dispatch knowledge. Provider and local credential work stay under the
selected deadline and cancellation authority. Rate identities, schemas and
numbers are distinct, with atomic admission. Pagination and streams report
bounded progress and completion explicitly. Runtime endpoints use the same
owner projection and executor.

The maintained implementation gate is `just verify-implementation`.
`just verify` additionally includes external dependency-policy/workflow and
release checks; their results must be reported with their actual scope.
