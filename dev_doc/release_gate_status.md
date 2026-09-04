# Concord v2.1 release-gate status

<!-- concord-release-status-owner -->

- **Canonical lifecycle status:** Concord v2.1 integration candidate / beta
- **Workspace version:** `2.1.0-beta.1`
- **Publication:** disabled in every intended public package manifest
- **Supported-platform boundary:** Windows and Linux are supported and mandatory. macOS is not currently a supported or release-gated platform.

**Beta integration decision:** PENDING — mergeable after the active language,
generated API, runtime/security, documentation, no-retrocompatibility,
Windows, Linux, exact-candidate, and merge-commit checks pass.

**Stable publication decision:** BLOCKED — owner package/license authority and
unobtained human-freeze evidence remain open. Stable `2.1.0`, tags, final public
package names, registry publication, and publication claims are not authorized.

This is the repository's only authoritative lifecycle and gate-status document.
README files, `FINAL.md`, the claim ledger, ADRs, and evidence records may
summarize it but do not define another status.

## Beta integration gates

| Integration gate | Current state | Evidence boundary |
|---|---|---|
| Active language and generated API | **Pending exact final-candidate verification** | The selected v2.1 parser, semantic graph, generated controls, descriptions, and UI negative space are implemented. The final convergence commit still requires the complete maintained suite. |
| Runtime and information-flow security | **Pending exact final-candidate verification** | Gates 5–9 previously passed mechanically on Windows and Linux at `0f72e865...`; no prior SHA is reused as final evidence. |
| Documentation and canonical examples | **Pending exact final-candidate verification** | README, quick start, overview, and Riot walkthrough are owned by compiled source anchors; the strengthened Rust `docs-check` remains the final authority. |
| No compatibility path | **Pending exact final-candidate verification** | Retired spellings remain only in focused migration diagnostics, compile-fail fixtures, migration tooling/docs, and explicit history. Successful aliases or parser modes are forbidden. |
| Windows | **Pending exact final-candidate execution** | A prior candidate passed the aggregate on Windows; the final convergence SHA must be rerun. |
| Linux | **Pending exact final-candidate execution** | A prior candidate passed Linux checks; the final convergence SHA must be rerun. |
| Exact candidate, clean tree, branch diff, and source export | **Pending** | Required before advancing `release/v2.1-readiness`. |
| Exact local merge commit | **Pending** | Required after the local `--no-ff` merge into `main`. |

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
prior Windows/Linux beta evidence. Gate 10's machine portion is part of the
beta integration table above; its independent-human portion remains explicitly
unknown for public freeze. Every affected check is rerun on the exact final
candidate and merge commit before beta integration is marked complete.

## Evidence boundary

`dev_doc/evidence/beta_candidate_0f72e865.md` is historical prior-candidate
evidence. New exact-candidate and merge evidence will be recorded without
claiming macOS, legal authority, package control, publication readiness, or
human comprehension.

No push, tag, publication, license selection, registry ownership, human result,
or unsupported platform execution may be inferred from this status.
