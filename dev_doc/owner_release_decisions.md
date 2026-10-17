# Concord V3 owner decisions

**Status:** V3 correction work and ordinary `concord-v3` source-branch updates
are authorized. Main integration and registry publication are separate actions.
The implementation remains `3.0.0-beta.1` with `publish = false` in every package.
[Release-gate status](release_gate_status.md) owns the current qualification.

## Decisions established for this work

| Decision | Recorded answer |
|---|---|
| Supported product version | **V3 only**, explicit project-owner instruction, 16 October 2026 |
| Conversion and older-version consumers | **No V2-to-V3 converter, aliases or old-binary state import**; [ADR 0027](adrs/0027-v3-only-product.md) supersedes those earlier obligations |
| Public source/API boundary | The owner selects the canonical V3 contract and does not require the former V2 surface |
| Reading project authority | Private specification and closure prompt may be read; their contents are not vendored into the public source |
| Corrective implementation | Direct fixes and native verification requested by the owner |
| Source repository | Owner-provided `https://github.com/gqsnt/concord_api`, branch `concord-v3` |
| Source-branch update | Ordinary fast-forward source push authorized; preserve other branches and history |
| Preserved main | `3765907aa2e424997bfd8f2fcdcc95275e6142f5`; no main merge is part of this correction |
| Dependency license policy | Owner instruction on 17 October 2026 authorizes explicit BSD-3-Clause exceptions for `alloc-no-stdlib`, `alloc-stdlib` and `brotli`; `brotli` also lists MIT for its conjunctive license. The project's own license and `publish = false` remain unchanged. |
| Supported validation | Windows and Linux, pinned Rust 1.97.1 and MSRV 1.86.0 |
| Runtime defaults | Retain the selected finite V3 defaults until workload evidence justifies a change |

## Decisions needed only for a later release action

| Decision | State |
|---|---|
| Merge a qualified beta into main or a separate release branch | **REQUIRED OWNER DECISION** at that action |
| Public registry package names and namespace ownership | **REQUIRED OWNER DECISION** for publication; recheck registry state then |
| The project's own public license and publication metadata | **REQUIRED OWNER DECISION** for publication; the dependency exceptions do not select a project license |
| Separate homepage and package README policy | **REQUIRED OWNER DECISION** if different from the repository documentation |
| Stable version, tag and registry publication | **REQUIRED OWNER DECISION**; the current beta stays unpublished |
| Human evidence for a public API freeze | Recommended: execute the prepared V3 task study and independent Rustdoc/diff review before freezing the public API |

The missing human observations do not prevent machine qualification of this
unpublished beta. Preparing a protocol or performing an implementer review does
not count as a human participant result. Historical namespace queries and
release decisions remain in dated evidence and Git; they do not establish
current registry availability or ownership.
