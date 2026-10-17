# V3 finalization decisions

**Updated:** 17 October 2026. These decisions resolve the current implementation choices. The
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
| Performance | Retain the corrected V3 implementation with measured costs and bounded attribution recorded | Paired runtime/scale and runtime/compiler attribution completed on the recorded inputs; the [performance report](reviews/2026_10_17_closure/PERFORMANCE_REPORT.md) preserves remaining historical costs and avoids per-feature timing claims |
| Generated implementation cost | Reuse each owner's resolved inspection program, omit private helpers when there are no value/body controls, and remove the redundant response future layer | The public V3 surface and provenance/sensitivity semantics remain covered by their maintained tests; structural and measured effects are reported separately |
| Machine qualification | Accept completed gates only for their exact source inputs | Linux and Windows complete implementation/analyzer gates passed on `579e8ea`; the final delivery's CLI/UI/docs and source-export receipt remain separate checks |
| Evidence delivery | Preserve original native failures, successful runs and source identities | The final-return artifact owns `FINAL_RETURN_RECEIPT.json`; private authority material is kept outside the public archive |
| Dependency license policy | Explicit BSD-3-Clause exceptions for `alloc-no-stdlib`, `alloc-stdlib` and `brotli`; the `brotli` exception also lists MIT | Owner authorization on 17 October 2026; [deny.toml](../deny.toml) retains the global allowlist and the updated candidate must pass all ten CI jobs |
| Human review | V3 task packets ready; no invented participants or independent-human result | Unknown for public freeze; not a machine-beta blocker |
| Version | `3.0.0-beta.1`, unpublished | Later merge/tag/publication decisions remain separate |

The former V2 worksheet is retained in Git history. It does not add an active
alias, converter, deferred V2 feature or separate execution path to V3.

## Completion and release boundary

Engineering qualification of an unpublished beta is a machine-evidence decision.
It does not choose a public license, secure registry names, merge into main,
perform a human study, freeze the public API or authorize stable publication.
C63/C64 in the [claim ledger](claim_evidence_ledger.md) remain **Unknown**.
Historical license-policy failures remain preserved. The owner's 17 October
instruction brings the three dependency exceptions into this correction's scope;
their validation is bound to the updated candidate's receipt. The project's own
license and unpublished status remain unchanged. The final source update is
authorized; later release actions remain with the owner as recorded in
[the release packet](owner_release_decisions.md).
