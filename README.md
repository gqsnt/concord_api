# Concord

Concord is currently a **Normative Implementation Candidate**. This repository contains the
approved Concord 0.1 specification and the Rust workspace. Phase 8 is the current delivered
implementation phase. It extends the one normalized generated request path with explicit bounded
sequential pagination, stable page/controller/safe-patch contracts, built-in and custom pagination
DSL, conditionally generated directly awaitable terminals, request-local canonical cycle
protection, finite page/item/identity ceilings, ordinary per-page execution, and aggregate
completion.

This is Phase 8, not a release candidate. Prepared endpoints and final extension certification,
positive dangerous raw successful responses, and release completion remain deferred to Phases
9-10. The built-in limiter is bounded best-effort process-local pacing; strict or
distributed enforcement remains an application-supplied custom limiter. The raw-response Cargo
flag is independently selectable, but its positive API remains explicitly owned by Phase 10.

Start with the [documentation overview](docs/README.md) and the
[normative specification index](docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md). For current
delivery state, see the
[implementation program](docs/planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md) and the
[Phase 8 report](docs/conformance/PHASE_8_REPORT.md). The implementation program is the active
planning authority for the remaining phase sequence.

Current validation entry points are:

```text
just fmt
just fmt-check
just check
just check-all
just check-no-default
just clippy
just clippy-all
just test
just test-all
just test-no-default
just doctest
just doctest-all
just doc
just doc-all
just deny
just diff-check
just release
```

Nextest is the canonical runner for Rust unit and integration tests. `cargo test` is retained only
for doctests, which Nextest does not execute. The default, all-feature, and no-default recipes are
intentional checks of different compiled surfaces. `just release` also runs independent
no-default configurations for every dangerous feature; all-feature testing is not treated as
feature-isolation evidence. Standalone Cargo fixture packages are reserved
for behavior that depends on package metadata or dependency topology; source-only macro and core
public-surface cases use ordinary integration tests or Trybuild.

Planned coverage in the conformance catalog describes intended verification. It is separate from
actual executed evidence and does not establish product conformance.
