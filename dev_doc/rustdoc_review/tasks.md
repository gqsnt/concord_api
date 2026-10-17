# Independent Rustdoc reviewer tasks

**Status:** v3 tasks; no human results are implied by preparing this packet.

Start with the public documentation index, generated example Rustdoc, and the
candidate identified in the handoff. The exact parent edit for tasks 6–7 is in
[`fixtures.md`](fixtures.md). Save the task 6 prediction before opening the
after snapshot or semantic diff. A missing snapshot or diff is a preparation
gap to record, not an invitation to invent its output.

Using public/generated documentation and supplied normal entry points only:

1. Find the common client-construction path, including required values and
   explicit runtime/limiter configuration where applicable.
2. For one required value, optional value, and defaulted value, identify the
   generated controls and distinguish payload `set_x`, exact `replace_x`,
   conditional `set_x_if_some`, `omit_x`, and retained `reset_x` availability.
3. Find one endpoint's effective response representation and its client,
   named-scope, or endpoint provenance.
4. Explain why request and response representation inheritance are independent
   directions.
5. Explain an `explicit` representation fence and what a new descendant
   endpoint must declare.
6. For the exact parent edit in `fixtures.md`, predict every affected endpoint
   by ID or source path. Distinguish changes to selected representations,
   provenance, and other behavior. Save the prediction before seeing the diff.
7. Read the resulting `concord.contract/3` snapshots and semantic diff. Classify
   caller, wire, security, runtime, and any other reported changes. Compare the
   reported endpoint set and each change's associations with the saved
   prediction, and record any correction.
8. Choose between `collect_prefix(u32)`, `collect_all(NonZeroU32)` and
   `pages(u32)`, then explain bound exhaustion and the traversal summary.
9. Distinguish successful `ByteStream`/`ItemStream<T>` acquisition from later
   stream completion, failure, cancellation, and early drop.
10. Identify which URL/header metadata is explicit caller authority through
    `RequestPlan`/`ApiResponse` and which exact values are absent from automatic
    observers, debug output, and limiter request context.

For each task record the initial prediction, correctness after verification,
measured elapsed seconds (or explicitly "not measured"), path/pages/types
opened, search terms, corrections, confidence from 1–5 before feedback, and
whether macro/core implementation source was consulted. Do not reconstruct
timing retrospectively. Identify the reviewer as human, agent, or implementer;
agent timings and findings must not be entered as human results.

Classify a finding as:

- **Critical:** a semantic, security, compatibility, completeness, or lifecycle
  misunderstanding;
- **Major:** a requested fact cannot be found normally or requires
  implementation reading;
- **Minor:** navigation or naming friction without semantic error.

Gate 10's optional public-freeze human review passes only when all ten facts are findable through
public docs/descriptions, no Critical misunderstanding remains, provenance and
parent-change consequences require no macro-source reading, and every
release-relevant Major finding has been fixed and reviewed again.
