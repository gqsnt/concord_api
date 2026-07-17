# Concord

Concord is currently a **Normative Implementation Candidate**. This repository contains the
approved Concord 0.1 specification and the Rust workspace. Phase 1 foundations are implemented;
the later endpoint runtime described by the specification is not yet present.

Start with the [documentation overview](docs/README.md), the
[normative specification index](docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md), and the
[implementation plan](docs/planning/CONCORD_IMPLEMENTATION_PLAN_0.1.md).

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
intentional checks of different compiled surfaces. Standalone Cargo fixture packages are reserved
for behavior that depends on package metadata or dependency topology; source-only macro cases use
ordinary integration tests or Trybuild.

Planned coverage in the conformance catalog describes intended verification. It is separate from
actual executed evidence and does not establish product conformance.
