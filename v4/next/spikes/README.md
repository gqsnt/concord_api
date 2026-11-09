# Spike index

Status reflects the final reviewed tree: partly marks an unfinished design or blocking correctness limit. Deltas are baseline → final spike on each record's measured SDK cohort; classifier glue and hand-counted source are distinct. Missing aggregate hand counts are marked not measured.

| Slug | Idea | Status works/partly | Glue Δ (classifier) | Net source lines Δ (hand) | Concepts added | Review outcome | One-line main limit |
|---|---|---|---|---|---|---|---|
| [D1-derive-debug](D1-derive-debug.md) | Derive explicit redacted Debug | works | −286 | −294 | Debug derive; sensitivity markers; optional D4 public marker | REJECT → ACCEPT_WITH_FIXES | Fail-open new fields and Debug text/pretty-output compatibility costs. |
| [A2-derive-error](A2-derive-error.md) | Derive mechanical error impls | partly | −68 (−53 A2-attributable) | −68 across error modules (−53 A2-attributable) | Error derive; display/from helpers | ACCEPT_WITH_FIXES → ACCEPT_WITH_FIXES; blocking finding remains | Final validator rejects valid Unicode fields; source-chain errors cannot adopt. |
| [A1-error-document](A1-error-document.md) | Declare a typed provider failure document | works | −51 | −15 | error statement; Failure/Document; D on 7 generics; 13 methods | ACCEPT_WITH_FIXES → ACCEPT_WITH_FIXES | Per-SDK conversion/D propagation remains; diagnostics/docs fail charter rules. |
| [I1-wire-display](I1-wire-display.md) | Display from scalar serde spelling | works | 0 | −243 | WireDisplay derive; optional crate-path helper | ACCEPT_WITH_FIXES → ACCEPT_WITH_FIXES | Requires Serialize and changes formatter flags; no classifier glue removed. |
| [E1-operation-effect](E1-operation-effect.md) | Read-effect metadata on operations | partly | −1 | −30 | read attribute; is_read() | ACCEPT_WITH_FIXES → ACCEPT_WITH_FIXES | operations still switches traits; Datadog restoration lacks a targeted regression test. |
| [J1-concord-logs](J1-concord-logs.md) | Static call event plus Summary | works | 0 | not measured (S3 Debug block −12) | tracing feature; Summary; four methods; event schema | ACCEPT_WITH_FIXES → ACCEPT_WITH_FIXES | Feature-off footprint lacks a guard; J3 redaction/summary changes remain unconditional. |
| [H1-request-line](H1-request-line.md) | Compact request facts and shared rules | partly | 0 | not measured (doc edits: FRED −9; Nager −10/+3) | Shared Requests section and notation; no DSL/public type | REJECT → REJECT; blocking finding remains | Sticky sensitivity still leaks a header literal in mixed-chain generated docs. |
