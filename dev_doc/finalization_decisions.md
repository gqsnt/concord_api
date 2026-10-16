# V3 finalization decisions

These decisions resolve the current implementation choices. The
[release-gate status](release_gate_status.md) records actual qualification;
[owner decisions](owner_release_decisions.md) delimit later release actions.

| Question | Selected V3 decision | Evidence or remaining limit |
|---|---|---|
| Supported version | V3 only; one grammar, runtime and current state schema | ADR 0027; unsupported syntax and removed CLI/alias checks |
| Representation shape | Directional generic traits and core-owned acquisition drivers | Representation matrix, UI and downstream consumers |
| Parent representation defaults | Client/scope defaults, local exception and explicit fence | Effective contract/Rustdoc provenance; unfamiliar-human comprehension remains unmeasured |
| Value controls | Payload `set_x`, exact optional `replace_x`, conditional `set_x_if_some`, omission and retained reset | Current value-shape matrix; no convenience aliases |
| Query/header composition | One ordered symbolic program with word operations | Suppressed encoders do not run; sensitivity remains sticky |
| Request bodies | One `PreparedBody` with media, length, bound and replay facts | Native H1/H2 exact-length and multipart corrections |
| Streams | Managed carriers with finite byte, total lease and pending-read idle bounds | Acquisition and full consumption remain distinct outcomes |
| Observation | Read-only, bounded and sanitized events | Observer containment and native privacy regressions |
| Credentials | Explicit required secrets or declared providers; intent/version storage protocol | Provider callbacks and local expiry remain inside deadline/cancellation authority |
| Rate state | Explicit ID, ordered labels and encoding schema; atomic all-or-none admission | Current V3 oracles and fail-closed schema registration |
| Runtime limits | Preserve selected finite defaults; no workload-independent retuning | [Runtime documentation](../docs/runtime.md) and limit/config tests |
| Empty chunks | 1,024 total for request/buffered/status/provider acquisition; 1,024 consecutive for acquired streams; 32 per poll | The distinct lifecycle bounds are intentional; the general defaults table must not erase that distinction |
| Performance | Measure the corrected V3 against the previous V3 source on the same host/toolchain | Structural savings alone do not imply latency or workload acceptance |
| Human review | V3 task packets ready; no invented participants or independent-human result | Unknown for public freeze; not a machine-beta blocker |
| Version | `3.0.0-beta.1`, unpublished | Later merge/tag/publication decisions remain separate |

The former V2 worksheet is retained in Git history. It does not add an active
alias, converter, deferred V2 feature or separate execution path to V3.
