# Concord v2.1 owner release decision packet

**Status:** REQUIRED OWNER DECISION FOR STABLE PUBLICATION

No repository file, task input, or observed Git history supplies the decisions
below. Gate 0 cannot pass and stable release/publication is forbidden until an
authorized project owner completes this packet. Local integration of the
unpublished `2.1.0-beta.1` is allowed after its technical, documentation,
Windows, and Linux gates pass.

## Required decisions

| Decision | Owner answer |
|---|---|
| License policy: exact SPDX expression or approved `license-file` policy | **REQUIRED OWNER DECISION** |
| Approved public package names for façade, core, macros, and extras | **REQUIRED OWNER DECISION** |
| Is this the first registry publication for the approved namespace? | **REQUIRED OWNER DECISION** |
| Did Concord v2.0 have external Git or path consumers? | **REQUIRED OWNER DECISION** |
| Is the intentional v2.0-to-v2.1 source/API break acceptable under version `2.1.0`? | **REQUIRED OWNER DECISION** |
| Official repository URL | **REQUIRED OWNER DECISION** |
| Official homepage URL, or explicit decision that no separate homepage exists | **REQUIRED OWNER DECISION** |
| Public package README policy/path | **REQUIRED OWNER DECISION** |
| Is a temporary remote review-branch push authorized solely for CI evidence? | **REQUIRED OWNER DECISION**; absent authorization means **NO PUSH** |

Record the deciding owner/role and date without adding secrets or personal
contact information. Package names must be rechecked immediately after approval
and are not secured until registry ownership or the intended publication step
establishes control.

## Observed facts supplied to the owner

- Local `main` at `f7464c5` is Concord v2.0.0, not v1.0.0.
- All intended public package manifests currently set `publish = false`.
- No project license file or license decision is present.
- At 2026-09-04T10:45:42Z, crates.io package `concord` was an unrelated
  Discord TUI, newest version 2.5.15, owned by `chojs23`.
- At that same query time, official Cargo and crates.io API lookups found no
  records for the candidate family `concord-api`, `concord-api-core`,
  `concord-api-macros`, and `concord-api-extras`. This means only “not found at
  query time,” not available, approved, reserved, or secured.
- The configured Git remote is `https://github.com/gqsnt/concord_api`, but a
  local remote configuration does not establish official public metadata.
- The v2.1 branch contains broad intentional source/API breaks from v2.0. The
  version and first-publication meaning therefore require a product decision.

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
