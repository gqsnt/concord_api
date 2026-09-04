# Documentation inventory and ownership

| Document | Audience | Purpose | Normative? | Primary code/test owner |
|---|---|---|---:|---|
| `README.md` | evaluators/users | product entry | no | examples/docs checks |
| `docs/README.md` | all | navigation | no | docs inventory test |
| `docs/overview.md` | evaluators | product model | explanatory | design invariants |
| `docs/quick_start.md` | first-time author/caller | tutorial | no | getting-started example |
| `docs/mental_model.md` | users/maintainers | lifecycle vocabulary | explanatory | runtime/invariant tests |
| `docs/design_invariants.md` | all implementors | non-negotiable properties | **yes** | policy checks + system tests |
| `docs/spec.md` | authors/compiler maintainers | exact DSL semantics | **yes** | parser/semantic/UI tests |
| `docs/modeling_apis.md` | API authors | modeling guidance | no | large examples/user tasks |
| `docs/generated_client.md` | callers | generated usage | normative for public surface | generated/UI/public API tests |
| `docs/requests_and_values.md` | authors | values/atoms/shaping | normative subsystem | shaping/property tests |
| `docs/request_bodies.md` | extension/authors | body contract | normative subsystem | body tests |
| `docs/responses.md` | extension/callers | response/status/streams | normative subsystem | response/record/stream tests |
| `docs/execution_policy.md` | maintainers/operators | exact stage order | **yes** | deterministic trace tests |
| `docs/runtime.md` | builders/maintainers | runtime/configuration | normative subsystem | runtime/build tests |
| `docs/auth.md` | security/app authors | private lifecycle | normative subsystem | auth systematic/security tests |
| `docs/retry.md` | app authors/operators | effects/retry/time | normative subsystem | retry/deadline tests |
| `docs/rate_limits.md` | provider/app authors | quota/enforcement | normative subsystem | rate tests |
| `docs/pagination.md` | authors/callers | aggregate behavior | normative subsystem | pagination proof suite |
| `docs/planning_and_observation.md` | tooling/operators | inspection/evidence/errors | normative subsystem | planning/redaction/observer/error tests |
| `docs/runtime_endpoints.md` | advanced callers | dynamic pressure valve | normative subsystem | runtime endpoint tests |
| `docs/extensions.md` | extension authors | public semantic traits/extras | normative subsystem | public API/extension tests |
| `docs/riot_walkthrough.md` | authors/maintainers | large real-shaped contract | evidence/example | Riot fixture/bench |
| `docs/migration_v2_1.md` | migrating users | v2.0.0 / earlier v1 forms → v2.1 | migration policy | migration/UI searches |
| `docs/evidence_and_release_gates.md` | maintainers | proof and freeze gates | **yes** | CI/xtask/perf/user protocol |
| `dev_doc/internals.md` | maintainers | responsibility/IR map | implementation normative | module/architecture review |
| `dev_doc/release_gate_status.md` | owners/release maintainers | sole lifecycle and gate decision | **yes for release state** | `docs-check` + exact evidence |
| `dev_doc/owner_release_decisions.md` | project owner | unresolved legal/package/product authority | authoritative only when owner-completed | Gate 0 |
| `dev_doc/implementation_v2_1.md` | maintainers | historical phase order | no; historical | release status |
| `dev_doc/repository_change_map.md` | maintainers | historical planned path impact | no; historical | repository state |
| `dev_doc/public_change_matrix.md` | reviewers/migrators | breaking surface inventory | no | public API diff |
| `dev_doc/decision_record.md` | maintainers | selected decision | **yes** | ADR review |
| `dev_doc/adrs/*` | maintainers | contentious decisions/revisit triggers | **yes per status** | named gates |
| `dev_doc/history/*` | maintainers | retained audit/rationale input | no; historical | explicit header + package exclusion |

Rules:

- every normative claim has a test or release-gate owner;
- examples in docs should be source-anchored to compile-checked examples where practical;
- one document owns each detailed fact; others link rather than silently restate different behavior;
- obsolete v1 documents are rewritten or removed, not left beside v2.1;
- every doc states target/freeze status while release gates remain open.
- exactly one file carries the `concord-release-status-owner` marker;
  `cargo run --locked -p concord-xtask -- docs-check --locked` enforces it.
