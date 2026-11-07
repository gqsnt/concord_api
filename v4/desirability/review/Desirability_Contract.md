# Proposed Concord desirability contract

**Review draft — 7 November 2026.** This document grants no execution, push, merge, publication, or deletion authority. It does not modify the attached plan. Existing action-specific authorization remains separate. Independent reviewers remain read-only; unavailable review is not approval. All experiments are automatic, without human-participant requirements.

## 1. Objective and permitted conclusions

Desirability is a contextual reason to choose Concord and continue using it. This contract tests the engineering hypothesis that, for a specified developer role and task, Concord makes the provider contract easier to discover, use correctly, review, extend, and maintain than a realistic alternative, while preserving necessary semantics and proportionate learning, integration, and migration costs. These task outcomes are evidence for desirability; they do not directly measure preference.

“Sexy” is the product hypothesis that useful power is immediately legible: provider structure becomes an understandable declaration, an ordinary Rust call, and visible rules for exceptions. Measurements may support that hypothesis for tested work. Human attraction, preference, sustained adoption, and market superiority remain **unmeasured**.

Keep three evidence classes separate: deterministic behavior checks; model workflow outcomes; structural diagnostics. More evidence in one class does not establish another.

## 2. Unit of evaluation

Every experiment records:

- **Actor:** SDK author, application caller, maintainer, extension author, or reviewer.
- **Task:** concrete goal, intended result, relevant failure, permitted materials and assistance.
- **Identity:** source, docs, fixtures, toolchain, instrument, and model settings.
- **Comparator:** current Concord plus its best documented workaround. Broader adoption claims additionally need a fair alternative, such as a hand-written reqwest implementation of the same task.
- **Outcome:** an independently checked success condition, baseline failure, minimum useful improvement, unlike witnesses, contrary case, and simple control.
- **Cost:** new obligations across Concord, capabilities, SDK, callers, documentation, and migration.

The corpus consists of selected reference slices. Report coverage of named obligations and tasks; do not extrapolate its SDK count to “95–99% of use cases.”

## 3. Primary outcomes

| Outcome | Required observation |
|---|---|
| Discover the contract | Correct required inputs, destination/base/path, public result, error owner, and decisive exception; first answer separated from assisted recovery |
| Complete ordinary use | Client construction and call compile **and** satisfy independent wire/result assertions; extra annotations and lookups recorded |
| Diagnose and recover | Correct failure classification and permitted recovery using retained nonsecret facts; no inappropriate replay or false completion |
| Change one provider fact | Necessary semantic edit sites, missed propagation, and stage detecting an omission |
| Preserve trustworthy construction | Invalid tenant/destination/credential combinations are unavailable or rejected at the intended stage; valid replacements remain possible |
| Extend and maintain economically | Complete capability integration, explicit ownership, preserved event order, and acceptable obligations on ordinary callers |

Identify critical facts separately. A correct method or media type cannot compensate for a wrong destination, public result, or recovery decision.

## 4. Gates, tradeoffs, and diagnostics

**Hard gates:** truthful public contracts; preserved provider semantics and declared compatibility; intended ownership and resource behavior; enumerated sensitive-output guarantees; legitimate native exits; existing required verification on the exact candidate. Never weaken tests to obtain a pass. An intentional compatibility change requires explicit migration and applicable authorization, not a better score.

**Bounded tradeoffs:** learning burden, caller verbosity, dependency/build cost, documentation volume, and implementation complexity. Register a specific cost, beneficiary, bound, and reason before implementation. A modest authoring cost may purchase meaningful caller safety; display that exchange rather than averaging it away.

**Diagnostics:** wrapper counts, forwarders, generated share, lines, words, symbols, and test totals. They identify possible causes and costs; none has a universally preferred direction. Classify wrappers as mechanical adaptation, provider validation, deliberate public boundary, or unexplained before considering removal. Count authoritative semantic edits, not Git hunks.

## 5. Three observability boundaries

1. **Concord-owned views.** Enumerate supported `Debug`/`Display`, errors and metadata views. Check declared sensitivity propagation on specified success/failure paths. Do not imply coverage of every downstream representation.
2. **SDK-owned public views.** Inventory SDK clients, contexts, models, errors, result wrappers and reports. Any redaction helper is opt-in; its field and nested-value coverage must be explicit. Preserve useful nonsecret diagnostic information.
3. **Native, application, and external output.** Native reqwest values, explicit unredacted escapes, application logging, provider responses and third-party formatting have separate owners. State exclusions and handoff responsibilities; do not promise global prevention of logging secrets or add automatic logging as an incidental change.

For every checked surface record sensitive source, transformation, formatting/error path, expected disclosure, exercise control, and exclusions. The conclusion is “no tested canary leaked on these surfaces,” not universal proof.

### Canary exercise controls

Use constructor-valid synthetic values. Literal bearer values are verified in the recorded header; Basic credentials through their expected encoding; signing keys through an independently verified signature **and absence of raw key material**; response-only values through a reached decoding fixture. Refused connections use preparation/error-path controls, not successful delivery. Check transformed credentials where those representations are themselves sensitive. Do not require every secret to appear literally on the wire.

## 6. Models, variability, and unknowns

Name the execution observation separately from the sensitive-output boundary. A declaration probe observes the **prepared request**; an SDK-operation probe observes **policy behavior and the public result**; a local receiver observes **requests made through the configured transport**. A signing policy or transport default can change what leaves the process after ordinary `prepare()`. An implementation-derived prepared-request dump measures predictability, while independent provider-fixture assertions check intended correctness.

Use fresh sessions with equivalent materials and no source/test access for reader tasks. Remove answer-bearing examples. Record requested and observed model/effort, including “unknown”; never invent unavailable metadata. Multiple available providers improve triangulation but do not simulate human populations.

Preserve initial answers, assistance, recovery, and all failures. Validate calls semantically, not only by compilation. For known deterministic failures use fail-before/pass-after evidence with controls. For stochastic comparisons use paired tasks, balanced presentation, repeated sessions, and per-task/provider results. A two-run range is not a confidence interval. Proposed pilot thresholds are decision choices, not validated population estimates.

A repeatedly consulted bank is **validation**. Reserve separate final tasks for use after freezing the candidate; if their feedback drives a change, retain that result and obtain fresh confirmation. Unknown does not mean unaffected.

## 7. Pilot decisions and expansion

- **Win:** a practically meaningful task improvement, preserved gates, acceptable registered costs, and no contrary-case failure.
- **Loss:** a gate failure, semantic counterexample, meaningful task regression, or disproportionate replacement burden.
- **Inconclusive:** variability, ceiling effects, split provider results, or insufficient instrumentation prevent a decision. Do not claim success or close an architectural family on that basis.
- **Keep current design:** its best existing mechanism solves the task as well with lower total burden.

First validate an instrument using known positive and negative cases. Prototype on unlike witnesses and controls; expand surviving changes to every affected SDK and the maintained broad verification. The final report states the exact tested improvement, remaining obligations, costs, exclusions, and unmeasured claims. It conveys no additional release authority.
