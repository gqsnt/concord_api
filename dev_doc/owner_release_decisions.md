# Concord v3 owner release decision packet

**Status:** REQUIRED OWNER DECISION FOR BETA INTEGRATION AND STABLE PUBLICATION

No repository file, task input or observed Git history supplies the decisions
below. The v3 implementation delivers an unpublished `3.0.0-beta.1` with
`publish = false` everywhere and makes none of these decisions. Gate 0 cannot
pass, and stable release or publication is forbidden, until an authorized
project owner completes this packet. The lifecycle is owned by
[`release_gate_status.md`](release_gate_status.md).

## Required decisions

| Decision | Owner answer |
|---|---|
| Integrate the `3.0.0-beta.1` source candidate into a release branch, given that native Windows, pinned-toolchain `1.97.1` and MSRV `1.86` checks were not run | **REQUIRED OWNER DECISION** |
| Which local branch is the integration target (this repository has no `main` branch; `release-audit` expects one as its merge base) | **REQUIRED OWNER DECISION** |
| License policy: exact SPDX expression or approved `license-file` policy | **REQUIRED OWNER DECISION** |
| Approved public package names for façade, core, macros, extras and, if published, `concord-inspect` | **REQUIRED OWNER DECISION** |
| Is this the first registry publication for the approved namespace? | **REQUIRED OWNER DECISION** |
| Did Concord v2.x have external Git or path consumers who need the v2.1 → v3 migration guide and tool? | **REQUIRED OWNER DECISION** |
| Is the intentional v2.1 → v3 source/API break acceptable under the major version `3.0.0`? | **REQUIRED OWNER DECISION** |
| Official repository URL | **REQUIRED OWNER DECISION** |
| Official homepage URL, or explicit decision that no separate homepage exists | **REQUIRED OWNER DECISION** |
| Public package README policy/path | **REQUIRED OWNER DECISION** |
| Human-comprehension and independent-review evidence required before public compatibility freeze, if any | **REQUIRED OWNER DECISION** |
| Is any push (for example a review branch solely for CI evidence) authorized? | **REQUIRED OWNER DECISION**; absent authorization means **NO PUSH** |

Record the deciding owner/role and date without adding secrets or personal
contact information. Package names must be rechecked immediately after approval
and are not secured until registry ownership or the intended publication step
establishes control.

## Observed facts supplied to the owner

Observed in this repository on 2026-09-26:

- The workspace version is `3.0.0-beta.1`; all eleven workspace package
  manifests set `publish = false`.
- No project license file exists and no manifest declares `license` or
  `license-file`.
- No Git remote is configured in this repository. Its local branches are
  `baseline-v2.1` (the unchanged v2.1 source export, associated with commit
  `3765907aa2e424997bfd8f2fcdcc95275e6142f5`), `v3-implementation` and
  per-engineer worktree branches; there is no `main` branch.
- The v3 line contains deliberate source/API breaks from v2.1 (see
  [`public_change_matrix.md`](public_change_matrix.md) and the migration guide
  [`docs/migration_v3.md`](../docs/migration_v3.md)).

Observed during the v2.1 lifecycle on 2026-09-04 (historical, not repeated for
v3):

- In the v2.1 repository, local `main` at `f7464c5` was Concord v2.0.0 and its
  configured remote was `https://github.com/gqsnt/concord_api`; a local remote
  configuration does not establish official public metadata.
- At 2026-09-04T10:45:42Z, crates.io package `concord` was an unrelated
  Discord TUI, newest version 2.5.15, owned by `chojs23`.
- At that same query time, official Cargo and crates.io API lookups found no
  records for the candidate family `concord-api`, `concord-api-core`,
  `concord-api-macros` and `concord-api-extras`. This means only "not found at
  query time", not available, approved, reserved or secured.

## Technical options, not decisions

- A distinct package family can retain the Rust import `concord` by giving the
  façade package a `[lib] name = "concord"` target and documenting a Cargo
  dependency alias.
- The unoccupied-at-query-time `concord-api*` family is one candidate only.
- Cargo accepts either an SPDX `license` expression or an authorized
  `license-file` policy. Engineering will implement exactly the owner-selected
  policy and will not infer one.

Registry query details are retained in
[`evidence/package_namespace_2026-09-04.md`](evidence/package_namespace_2026-09-04.md).
