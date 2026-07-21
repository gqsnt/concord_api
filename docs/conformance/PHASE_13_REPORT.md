# Phase 13 Conformance Report

## Verdict

`PHASE_13_COMPLETE_WITH_LIMITATIONS`

Phase 13 — Lexically Scoped Rate Limits is complete against the prospective immutable source
identified below. The sole limitation is Phase 14 product-completion, conformance-closure, and
release-final work. The normative package remains `Normative Implementation Candidate`; Phase 14
is active and `NOT_STARTED`, and only Phase 14 may restore `Normative Release Final`.

## Prospective correction boundary

- Correction starting HEAD and explicit baseline:
  `44799306fbf1bd039b73c5e57903aed30e5e7a0d`.
- Executable correction commits:
  `985f6e56b6e2ec6a8980062cc1e57b52d31267c9` and
  `91378c1a0ac748ab029d6c396871920b63366716`.
- Final immutable tested source: `91378c1a0ac748ab029d6c396871920b63366716`.
- Evidence/report/state commit: `51941379ed153204c7d5f5da824c96b0d4078153`.
- Evidence anchor: `962bb0ad4ff173befcb6b21b806eac086859551f`.
- Metadata descendant: the compatible documentation-only descendant that records the actual
  anchor after it exists.
- Branch: `master`; head policy: `anchor-must-be-an-ancestor`.

The authoritative correction range is exactly:

```text
44799306fbf1bd039b73c5e57903aed30e5e7a0d
→ 91378c1a0ac748ab029d6c396871920b63366716
```

Its only changed path is:

```text
scripts/audit-release-registry.ps1
```

The blocked historical classification remains immutable superseded history:

- blocked source: `d0891ac8de5e44c5da7d8d1202c3a9e4cfa4b304`;
- blocked evidence: `847679cff263ce642227bd0a042456a20495416b`;
- blocked anchor: `c305edafcbb8131ed86c62a38d25104f5e7c80ac`;
- blocked metadata descendant: `44799306fbf1bd039b73c5e57903aed30e5e7a0d`.

The initial prospective documentation sequence `11c47d75… → 76efa827… → 4f60b3f9…` is also
superseded: its Final-mode report audit rejected a line break inside one required literal status
phrase. That failure changed no executable source, test, evidence row, or validation result.

That older source classification was invalid because the historical correction range leading to
`d0891ac…` interleaved executable audit corrections with Phase 13 evidence, report, planning, and
controller-state changes. No history rewrite was needed: this prospective sequence starts from the
current inherited state and establishes a new executable-only range.

## Source-boundary audit

The release audit now accepts an explicit `-CorrectionBaseline`, verifies exact commit existence and
ancestry, and audits `CorrectionBaseline..SourceCommit` as the authoritative source range. For this
correction the baseline is fixed to `44799306…`, and the only permitted source-range path is the
tracked release-audit script.

Source mode accepts inherited Phase 13 report, evidence, planning, and controller files as
superseded state. It validates the prospective executable range and current executable source while
reporting final evidence completeness as not assessed. Evidence and Final modes restrict the
post-source range to the approved evidence, coverage, validation, report, planning, README, and
controller paths and reject executable changes.

The self-test suite passes in PowerShell 7.6.3 and Windows PowerShell 5.1 with 26 deterministic
rejection/acceptance classes and 979 parsed claims. It covers the script-only source range; rejection
of evidence CSV, report, controller, and README source paths; missing and non-ancestral baselines;
documentation-only descendants; rejection of post-source executable changes; and inherited Phase 13
evidence handling.

Focused inventory also exposed and corrected one inherited command-registration defect: the governor
key-limit revalidation test was registered under `execute::tests` although it is owned by
`rate_limit::governor::tests`. The corrected inventory selected both required tests and the focused
execution passed 2/2. No product or test semantics changed.

## Authority and implemented contract

The Phase 13 authority remains `docs/normative/11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md` and
`ADR-046 — Generated rate limits use lexical ownership and global keys`. The generated contract transition exactly `13 → 14` remains unchanged.

The approved grammar remains:

```text
rate_limit_decl =
    "rate_limit" "{"
        cost_clause?
        rate_window+
    "}" ;

rate_limit_observer_decl =
    "rate_limit_observer" rust_path ";" ;

cost_clause = "cost" positive_integer ";" ;
rate_window = positive_integer "/" duration_literal ";" ;
```

Client, outer-scope, inner-scope, and endpoint rules compose in that exact lexical order. Generated
declaration identities are deterministic and collision-safe, every normal generated key is
`RateLimitKey::global()` with zero parts, and request values do not enter generated identities,
keys, plans, or rate-limit-only `Display` bounds. Templates, attachments, `by`, replace, and off
remain absent. Advanced prepared/custom explicit keys remain bounded, inspectable by the selected
custom limiter, and redacted from automatic surfaces.

The observer continues to configure only the generated default governor. Governor-disabled generated
and prepared nonempty fallback plans fail closed unless a custom limiter or explicit no-op is
deliberately selected. Atomic reservation, hook and credential ordering, cancellation without quota
consumption, no refund, authentication recovery, hidden retry containment, response observation,
cooldown, finite-state behavior, pagination plan reuse, and security/redaction behavior are unchanged.

## Requirement lineage

Retained with explicit version change:

`PROD-GUAR-010`, `PROD-STATE-001`, `DSL-RATE-001`, `DSL-RATE-002`, `API-RATE-004`,
`GEN-BUILD-004`, `GEN-DOC-001`, `GEN-DOC-002`, `SEC-RATE-001`, `SEC-RATE-002`,
`BOUND-RATE-005`, `EXEC-RATE-001`, `EXEC-RATE-002`, `EXEC-RATE-003`, `FEAT-GOV-001`,
`FEAT-GOV-002`, `CONF-COMPILE-001`, `CONF-COMPILE-002`, `CONF-COMPILE-003`,
`CONF-DOWNSTREAM-002`, `CONF-SEC-001`, `CONF-MSRV-001`, `CONF-DOC-001`.

Superseded mappings:

`DSL-RATE-003 → DSL-RATE-011`, `DSL-RATE-004 → DSL-RATE-009`,
`DSL-RATE-005 → DSL-RATE-010`, `DSL-RATE-006 → DSL-RATE-010`, and
`DSL-RATE-007 → DSL-RATE-011`.

New Phase 13 requirements:

`PROD-MODEL-009`, `PROD-GUAR-015`, `PROD-NONGOAL-003`, `DSL-RATE-008` through
`DSL-RATE-012`, `API-RATE-005`, `GEN-RATE-001` through `GEN-RATE-006`, `SEC-RATE-003`,
`EXEC-RATE-004`, `EXEC-RATE-005`, `COMPAT-MACRO-006`, and `CONF-RATE-001` through
`CONF-RATE-004`.

## Validation

All 39 focused Phase 13 commands passed against `91378c1a…`. Every filtered Nextest command was
preceded by a matching inventory with a nonzero expected selection. Proof includes lexical parsing
and migration diagnostics, identity equality and collision resistance, global zero-part keys,
governor sharing, pagination plan/recovery/cancellation, governor-disabled generated/prepared
behavior, advanced explicit keys, runtime snapshot, first-excess limits, atomicity, cooldown and
finite state, retry and observation, security, Rustdoc, examples, feature topology, downstream
workflows, and Rust 1.85.

The complete maintained matrix passed:

- `just fmt-check`, `just check`, `just check-all`, `just check-no-default`;
- `just clippy`, `just clippy-all`;
- `just test` — 583/583;
- `just test-all` — 608/608;
- `just test-no-default` — 1/1;
- `just doctest`, `just doctest-all`;
- `just doc`, `just doc-all`;
- `just deny`, `just diff-check`, and `just release`.

The release aggregate independently reran the feature, fixture, downstream, security, Rustdoc,
examples, benchmark-smoke, MSRV, dependency, and source-audit surfaces. Its source mode reported:
prospective source boundary passed; final evidence completeness not assessed.

## Evidence and preservation

All 46 Phase 13 evidence rows are direct, passing, waiver-free, and source-matched to
`91378c1a0ac748ab029d6c396871920b63366716`. The completed claim audit has one section per row and
agrees exactly with the evidence CSV and source-matched validation registry. The registry has 58
Phase 13 validation rows, including PowerShell 7 and Windows PowerShell self-tests and the dedicated
prospective source-boundary command.

The catalog has 1,726 total rows, including 46 direct and 0 partial Phase 13 rows. Coverage remains
597 rows: 581 active requirements and 16 superseded historical requirements. No Phase 13 row is
partial or waived.

The exact Phase 0–12 evidence prefix remains 1,344,331 bytes and 1,680 rows, with SHA-256
`8300c71829e3ed5f4049d74832bea64a3ed3c46de7b731555de4567a05334b05`. The binary prefix and every
historical Phase 0–12 report remain unchanged.

## Closure boundary

No remote operation, branch creation, amendment, rebase, history rewrite, tag, publish, deploy,
credential access, controller-policy change, Phase 14 product behavior, or Phase 15 work occurred.
Phase 14 remains active and `NOT_STARTED`. Normative status remains
`Normative Implementation Candidate`, and release-final promotion remains reserved exclusively for
Phase 14.
