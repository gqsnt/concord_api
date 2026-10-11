# Documentation inventory and ownership

**Status:** Active inventory for `3.0.0-beta.1`. User documentation (`README.md`
and `docs/`) is converted to v3 separately from this maintainer set; where a
`docs/` page still describes v2.1, the v3 implementation specification and the
v3 ADRs win.

## User documentation

| Document | Audience | Purpose | Normative? | Primary code/test owner |
|---|---|---|---:|---|
| `README.md` | evaluators/users | product entry | no | examples/docs checks |
| `docs/README.md` | all | navigation | no | docs inventory check |
| `docs/overview.md` | evaluators | product model | explanatory | design invariants |
| `docs/quick_start.md` | first-time author/caller | tutorial | no | getting-started example |
| `docs/mental_model.md` | users/maintainers | lifecycle vocabulary | explanatory | runtime/invariant tests |
| `docs/design_invariants.md` | all implementors | non-negotiable properties | **yes** | policy checks + system tests |
| `docs/spec.md` | authors/compiler maintainers | exact DSL semantics | **yes** | parser/semantic/UI tests |
| `docs/modeling_apis.md` | API authors | modeling guidance | no | large examples |
| `docs/generated_client.md` | callers | generated usage | normative for public surface | generated/UI/public API tests |
| `docs/requests_and_values.md` | authors | values, atoms, shaping | normative subsystem | shaping tests |
| `docs/request_bodies.md` | extension authors | body contract | normative subsystem | body tests |
| `docs/responses.md` | extension authors/callers | response, status, absence, streams | normative subsystem | response/record/stream tests |
| `docs/execution_policy.md` | maintainers/operators | exact stage order | **yes** | deterministic trace tests |
| `docs/runtime.md` | builders/maintainers | runtime configuration | normative subsystem | runtime/build tests |
| `docs/auth.md` | security/app authors | private credential lifecycle | normative subsystem | auth tests |
| `docs/retry.md` | app authors/operators | effects, retry, time | normative subsystem | retry/deadline tests |
| `docs/rate_limits.md` | provider/app authors | quota and enforcement | normative subsystem | rate tests |
| `docs/pagination.md` | authors/callers | traversal behavior | normative subsystem | pagination tests |
| `docs/planning_and_observation.md` | tooling/operators | inspection, evidence, errors | normative subsystem | planning/redaction/observer/error tests |
| `docs/runtime_endpoints.md` | advanced callers | runtime endpoints | normative subsystem | runtime endpoint tests |
| `docs/extensions.md` | extension authors | public semantic traits and extras | normative subsystem | public API/extension tests |
| `docs/riot_walkthrough.md` | authors/maintainers | large real-shaped contract | evidence/example | Riot fixture and recordings |
| `docs/migration_v3.md` | migrating users | v2.1 → v3 | migration policy | `migrate-v3` tool tests |
| `docs/migration_v2_1.md` | migrating users | v2.0 → v2.1 (historical) | no; historical | — |
| `docs/evidence_and_release_gates.md` | maintainers | proof and freeze gates | **yes** | CI/xtask |

## Maintainer documentation (`dev_doc/`) and root status

| Document | Audience | Purpose | Normative? | State |
|---|---|---|---:|---|
| `FINAL.md` | all | v3 status summary; defines no status of its own | no | active |
| `dev_doc/release_gate_status.md` | owners/release maintainers | sole lifecycle and gate record | **yes for release state** | active |
| `dev_doc/owner_release_decisions.md` | project owner | open legal/package/integration decisions | authoritative only when owner-completed | active |
| `dev_doc/decision_record.md` | maintainers | selected v3 implementation target | **yes** | active |
| `dev_doc/adrs/*` | maintainers | decisions, supersessions, revisit triggers | **yes per status** | active index; v2.1 ADRs kept as history |
| `dev_doc/claim_evidence_ledger.md` | maintainers | claim states with evidence | yes for claim states | active |
| `dev_doc/internals.md` | maintainers | responsibility and pipeline map | implementation normative | active |
| `dev_doc/repository_change_map.md` | maintainers | v2.1 → v3 source map (spec §21.2) | no | active |
| `dev_doc/public_change_matrix.md` | reviewers/migrators | v2.1 → v3 public changes | no | active |
| `dev_doc/documentation_contract.md` | maintainers | documentation truth rules | yes for docs | active |
| `dev_doc/doc_inventory.md` | maintainers | this inventory | no | active |
| `dev_doc/baseline_v2_1.md` | maintainers | pre-v2.1 baseline | no | historical |
| `dev_doc/implementation_v2_1.md` | maintainers | v2.1 phase plan | no | historical |
| `dev_doc/release_verification_2_1.md` | maintainers | v2.1 candidate verification | no | historical |
| `dev_doc/finalization_decisions.md` | maintainers | v2.1 finalization worksheet | no | historical |
| `dev_doc/documentation_refactor_review.md` | maintainers | v2.1 documentation refactor review | no | historical |
| `dev_doc/evidence/v3_documentation_slice_observations.md` | maintainers | observed subset of v3 command results behind the claim ledger | no | active, scoped to its commits |
| `dev_doc/evidence/*` (all other files) | maintainers | v2.1 exact-SHA evidence and observations | no | historical |
| `dev_doc/dx_study/*` | facilitators | v2.1 unfamiliar-human study packet | no | v2.1 packet; needs revision before use on v3 |
| `dev_doc/rustdoc_review/*` | reviewers | v2.1 independent Rustdoc review packet | no | v2.1 packet; needs revision before use on v3 |

The implementation report (`IMPLEMENTATION_REPORT.md`), acceptance results and
migration report for the v3 delivery are produced separately and are not
stored in this repository.

Rules:

- every normative claim has a test or release-gate owner;
- examples in docs are source-anchored to compile-checked examples where
  practical;
- one document owns each detailed fact; others link rather than silently
  restate different behavior;
- superseded documents are labelled historical near their heading, not left
  looking active beside v3;
- exactly one file carries the `concord-release-status-owner` marker;
  `cargo run --locked -p concord-xtask -- docs-check --locked` enforces it.
