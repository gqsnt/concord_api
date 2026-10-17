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
| Dependency license policy | Owner instruction on 17 October 2026 authorizes explicit BSD-3-Clause exceptions for `alloc-no-stdlib`, `alloc-stdlib` and `brotli`; `brotli` also lists MIT for its conjunctive license. The dependency policy is separate from the project grant selected below; `publish = false` remains in force. |
| Project open-source license | **MIT OR Apache-2.0**, selected under the owner's 17 October 2026 instruction to choose an open-source license; both texts and package metadata are implemented |
| Requested exercise and documentation review | The owner requests execution by agents on 17 October 2026. The [follow-up](reviews/2026_10_17_followup/README.md) records actual answers, compiler/runtime checks and independent agent rereview. Human participant metrics remain unmeasured and are not required to complete this requested technical review. |
| Repository metadata | `https://github.com/gqsnt/concord_api`, inherited consistently by workspace packages |
| Supported validation | Windows and Linux, pinned Rust 1.97.1 and MSRV 1.86.0 |
| Runtime defaults | Retain the selected finite V3 defaults until workload evidence justifies a change |

## Decisions needed only for a later release action

| Decision | State |
|---|---|
| Merge a qualified beta into main or a separate release branch | **REQUIRED OWNER DECISION** at that action |
| Public registry package names and namespace ownership | **REQUIRED OWNER DECISION** for publication; recheck registry state then |
| Separate homepage and package README policy | **REQUIRED OWNER DECISION** if different from the repository documentation |
| Stable version, tag and registry publication | **REQUIRED OWNER DECISION**; the current beta stays unpublished |
| External developer feedback | Optional additional empirical evidence; the requested agent exercise and Rustdoc review are recorded separately, with no human participants claimed |

The owner-directed agent evaluation completes the requested technical exercises.
Its results do not establish how unfamiliar humans perform: the retained human
protocols and C63/C64 observations remain separately unmeasured. Historical namespace queries and
release decisions remain in dated evidence and Git; they do not establish
current registry availability or ownership.
