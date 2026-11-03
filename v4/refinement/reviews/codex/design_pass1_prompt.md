# Independent Concord V4 product/design challenge — PASS 1 (blind)

You are the independent reviewer, requested as local Codex GPT-6.1 Sol at xhigh reasoning effort. Report the effective model/effort you observe and any mismatch. Do not edit anything, push, publish or change global configuration.

## Isolation rules for this pass (important)

This pass must be independent of the existing implementation and of the coordinator's thinking.
- Read ONLY the three files in the current directory: `01_CHARTER.md`, `USER_TASKS.md`, `CONTRACT_CASES.md` (and this prompt).
- Do NOT read, list or search any other path on this machine (in particular nothing under /home/canarit/projects, no Git repositories, no crate registries for "concord"). Do not use the network.
- If anything about the existing Concord V4 design is already in your context, disclose it explicitly instead of pretending the pass is blind.

A second, separate pass will later give you the frozen V4 source to compare against. Write this pass without it.

## Task (verbatim from the owner's prompt, first pass)

Before reading the coordinator's preferred design or decision log, read only the supplied product charter and neutral user-task corpus. The product is a small typed declarative backbone around reqwest. It represents clients/scopes/endpoints, value/default/presence semantics, HTTP composition, scoped sensitivity and minimal representations. Auth orchestration, retries, rate limiting and pagination are excluded. Ordinary secret/header/query data and useful client-local typed values are legitimate.

From these requirements, identify the few decisions most likely to determine desirability. Propose a minimal coherent authoring/calling surface and the best serious alternative for its weakest point. Show complete declaration, generated signatures and calling examples, not just abstract advice. State likely surprises, unnecessary opinions, user costs hidden by a small implementation, and where native escape should be sufficient.

In particular challenge root construction, exact optional controls, default/reset lifecycle, inherited overrides, representation selection/defaults, wire formatting, Clone/ownership, non-wire storage and error boundaries. Do not add a generic execution framework.

## Output (your final message is saved verbatim as the pass-1 report)

1. Effective model/effort observed; files you read; contamination disclosure.
2. The 5–8 decisions that most determine desirability, each with the smallest counterexample that exposes it.
3. Your proposed authoring surface: a complete declaration for the regional catalog fixture in USER_TASKS.md (base `/v1`, region host label, tenant scope, product ID, optional category, default page size, repeated tags, sensitive header, JSON search/update, error document, streaming download), the generated Rust signatures (client constructor, scope navigation, endpoint inputs/setters, prepare/send/native exit, error type), and caller code for U03/U04/U06/U08/U09/U10/U14/U15.
4. For each of: root construction, optional controls (incl. exact None / omission), default evaluation and reset, inherited override vs exact replacement, representation syntax and defaults, wire formatting (Display vs adapters), Clone/ownership/Send, non-wire storage, error/sensitivity boundary — your choice, the best serious alternative, and the trade-off. Truth tables where semantics are subtle.
5. Likely surprises and hidden user costs in a "small implementation" design; places where native reqwest escape should suffice and no core feature is warranted.
6. Explicitly rejected ideas (with the counterexample) so they are not re-explored.

Mark everything as reasoning (you are not running code in this pass). No fabricated tests or measurements.
