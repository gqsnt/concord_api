# Concord

Concord is currently a **Normative Implementation Candidate**. This repository contains the
approved Concord 0.1 specification and the initial Rust workspace scaffold; it does not yet
implement the runtime capabilities described by the specification.

Start with the [documentation overview](docs/README.md), the
[normative specification index](docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md), and the
[implementation plan](docs/planning/CONCORD_IMPLEMENTATION_PLAN_0.1.md).

Current validation entry points are:

```text
just fmt-check
just check
just clippy
just test
just nextest
just deny
just release
```

Planned coverage in the conformance catalog describes intended verification. It is separate from
actual executed evidence and does not establish product conformance.
