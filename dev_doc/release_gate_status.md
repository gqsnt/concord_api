# Concord v2.1 release-gate status

<!-- concord-release-status-owner -->

- **Canonical lifecycle status:** Concord v2.1 locally integrated unpublished beta (integration candidate / beta lifecycle)
- **Workspace version:** `2.1.0-beta.1`
- **Publication:** disabled in every intended public package manifest
- **Supported-platform boundary:** Windows and Linux are supported and mandatory. macOS is not currently a supported or release-gated platform.

**Beta integration decision:** COMPLETE LOCALLY, SUBJECT TO THIS MERGE COMMIT'S
TERMINAL VERIFICATION — verified release candidate `bc4efb03...` was
fast-forwarded to `release/v2.1-readiness` and merged with `--no-ff` into local
`main`. The active language, generated API, runtime/security, documentation,
no-retrocompatibility, Windows, Linux, exact-candidate, and source-export gates
passed on that exact release parent. This merge is valid only when the complete
Windows/Linux matrix also exits 0 on the exact merge commit.

**Stable publication decision:** BLOCKED — owner package/license authority and
unobtained human-freeze evidence remain open. Stable `2.1.0`, tags, final public
package names, registry publication, and publication claims are not authorized.

This is the repository's only authoritative lifecycle and gate-status document.
README files, `FINAL.md`, the claim ledger, ADRs, and evidence records may
summarize it but do not define another status.

## Beta integration gates

| Integration gate | Current state | Evidence boundary |
|---|---|---|
| Active language and generated API | **Passed on exact release parent `bc4efb03...`** | Default/all-feature nextest, 190 macro tests, and no-default/all-feature/core UI configurations passed on Windows and Linux. |
| Runtime and information-flow security | **Passed on exact release parent `bc4efb03...`** | Both complete maintained aggregates and explicit locked workspace tests passed on Windows and Linux. |
| Documentation and canonical examples | **Passed on exact release parent `bc4efb03...`** | README, quick start, overview, and Riot walkthrough are owned by compiled source anchors; docs-check validated six machine-owned canonical anchors on both platforms. |
| No compatibility path | **Passed on exact release parent `bc4efb03...`** | The 107-file active scan and focused compile-fail fixtures passed. Retired spellings remain only in migration diagnostics/tooling/docs, negative fixtures, and explicit history. |
| Windows | **Passed on exact release parent `bc4efb03...`** | `just verify` and the explicit locked terminal commands exited 0 on Windows AMD64. |
| Linux | **Passed on exact release parent `bc4efb03...`** | The same commands exited 0 as native x86_64 Linux binaries on Ubuntu 26.04.1 under WSL2. |
| Exact candidate, clean tree, branch diff, and source export | **Passed on `bc4efb03...`** | Release audit reported the exact SHA and empty status; source export extracted and audited 888 entries on both platforms. |
| Exact local merge commit | **Terminal repeat gate** | This merge commit receives the same complete Windows/Linux aggregate, explicit locked checks, source export, release audit, parent-topology check, and clean status. The self-referential SHA/result is recorded in the operator handoff. |

The absence of a macOS run is retained as a historical fact in older evidence.
It is not missing evidence for this support boundary and does not block beta
integration or stable publication.

## Stable-publication gates

| Stable gate | State | Required evidence or authority |
|---|---|---|
| Gate 0 — package identity, legal authority, and release namespace | **Blocked — owner authority required** | Owner-selected license, approved and controlled package names, official metadata, and final-name package rehearsal. Registry `concord` was observed to identify an unrelated project; unoccupied-at-query candidate names are not reservations. |
| Gate 3 — unfamiliar-human comprehension | **Unknown/Blocked for public freeze** | Zero qualifying external participants have completed the maintained study. No comprehension claim is made. The owner must decide whether and how this evidence gates public compatibility freeze. |
| Independent Rustdoc/semantic-diff review | **Unknown/Blocked for public freeze** | Zero qualifying independent reviews have completed the packet. Implementer or AI review is not substituted. The owner must decide whether this evidence is mandatory for public freeze. |
| Stable lifecycle and publication mechanics | **Blocked** | Gate 0 and the owner-selected human-freeze policy must close before stable `2.1.0`, final names/metadata, tag, publication rehearsal, or registry action. |

Mechanical Gates 1, 2, and 4–9 have selected implementations supported by the
exact Windows/Linux release-parent evidence. Gate 10's machine portion is part
of the beta integration table above; its independent-human portion remains
explicitly unknown for public freeze. The terminal merge-commit matrix is a
validity condition of the local integration, not a stable-publication claim.

## Evidence boundary

`dev_doc/evidence/final_integration_candidate_6967c378.md` records the current
implementation-candidate matrix and the evidence-seal rule.
`dev_doc/evidence/local_beta_merge.md` records the merge parents, method, and
self-referential exact-merge verification boundary.
`dev_doc/evidence/beta_candidate_0f72e865.md` remains historical
prior-candidate evidence. Exact terminal and merge evidence is recorded without
claiming macOS, legal authority, package control, publication readiness, or
human comprehension.

No push, tag, publication, license selection, registry ownership, human result,
or unsupported platform execution may be inferred from this status.
