# v2.1 claim–evidence ledger

Consequential claims move state only when compilation, tests, measurements, or
human tasks provide evidence. The permitted states are Observed, Inferred,
Assumed, Desired, Candidate, Selected, Supported, Disconfirmed, Unknown, and
Deferred.

| ID | Claim | State | Evidence/gate | Falsifier | Decision dependency |
|---|---|---|---|---|---|
| C0 | The supplied committed baseline is `f7464c5` (subject `v2.0`) while its workspace package version is `1.0.0`; the dirty supplied tree contained the unreviewed v2.1 implementation | Observed | `git show f7464c5:Cargo.toml`, initial status/diff, checkpoint `50460e7`, 2026-09-04 | a different supplied revision or manifest | migration accounting |
| C1 | explicit `?` and `each` make type aliases unable to change presence/cardinality | Supported | value/atom UI and request-head property tests, 2026-09-04 | alias changes generated surface/wire behavior | value/placement model |
| C2 | inherited representations reduce repetition without unacceptable hidden dependency | Selected, provisional | Gate 2 mechanics passed; Gate 3 human study blocked, 2026-09-04 | users miss origin or moves cause unrecognized valid-but-wrong behavior | representation ownership |
| C3 | `RequestAtom` is safer than implicit `Display` at acceptable ergonomic cost | Selected, provisional | Gate 4 mechanical suite passed; human DX evidence remains unavailable | boilerplate/diagnostics materially worse with no defect reduction | atom model |
| C4 | word patch operations are more predictable than punctuation | Selected, provisional | Gate 3 | punctuation wins task accuracy/time | syntax spelling |
| C5 | retaining `with_x` improves common use enough to pay method cost | Unknown; not selected for v2.1 | Human evidence unavailable; mandatory fallback removed the alias, ADR 0012 | a future controlled task study shows material benefit without omission ambiguity | generated controls |
| C6 | generic representation pairs can express the finite built-in matrix under MSRV/coherence | Supported | Gate 1: workspace tests, downstream multi-pair marker, Rust 1.86 checks, 2026-09-04 | overlap, driver leakage, or remote diagnostics | representation traits |
| C7 | endpoint-owned status improves truth and finite reflection | Supported | UI/runtime/status suites and generated descriptions, 2026-09-04 | common APIs require unsafe dynamic callback | status model |
| C8 | safe/idempotent/non-idempotent effect classes improve retry/pagination correctness | Supported | effect, retry, and pagination compile/runtime suites, 2026-09-04 | distinction yields no behavior or cannot model provider semantics | effect model |
| C9 | controller-owned page interpretation supports real pagination with narrow authority | Supported | Gate 6 controller, generated, limit, identity, and runtime suites, 2026-09-04 | real controllers require broad native response/async iteration | pagination model |
| C10 | per-page body prepare is sufficient baseline | Supported | Gate 6 lifecycle tests and maintained runtime probe, 2026-09-04 | encoding dominates and aggregate reuse has safe bounded contract | body lifecycle |
| C11 | read-only observer is sufficient for general operation evidence | Selected, provisional | Gate 5 mechanics and Gate 9 sanitization passed; operator task review remains unavailable | legitimate unowned cross-cutting decision exists | observation model |
| C12 | rate feedback can be non-result-determining without losing necessary enforcement | Supported | Gate 7 whole-plan/future-cooldown suites, 2026-09-04 | current result must legitimately depend on durable feedback commit | limiter authority |
| C13 | optional static secret is useful and comprehensible | Selected, provisional | auth construction task | profiles/separate clients materially clearer | auth surface |
| C14 | stream carriers can hide drivers at acceptable cost | Supported | static/enum/boxed allocation, throughput, lifecycle, compile, and binary comparison; Gate 8, 2026-09-04 | maintained probe crosses a precommitted budget | stream internals |
| C15 | complete static/effective descriptions fit machine compile/binary budgets | Supported mechanically; human clarity unknown | clean baseline/candidate measurements, edit costs, 1/10/100/500/1,000 expansion/method/binary/description scale, analyzer; Gate 10 human review blocked | scale becomes superlinear or a reviewer cannot locate provenance/consequences | reflection architecture |
| C16 | the supplied pre-checkpoint tree passed the focused maintained baseline | Supported within scope | `just fmt`, `just check`, and `just test`, 2026-09-04 | a reproducible failure in those commands | regression comparison; no claim that baseline `just verify` was run |
| C17 | one-shot ordinary body endpoints must not inherit pagination's `Clone` obligation | Supported | `non_clone_body_endpoint` and `pagination_non_clone_input` UI counterexamples, 2026-09-04 | a sound stable-Rust shape reconstructs pages without an input replay capability | endpoint/body generated bounds |
| C18 | the migrated v2.1 candidate passes every maintained repository check | Disconfirmed | development and clean-worktree `just verify` both fail `cargo deny` because the four public packages are unlicensed; every other recipe passed independently, 2026-09-04 | an owner-selected license plus a complete green rerun | blocks stable release and merge |
| C19 | packaged public crates work without workspace path dependencies | Supported technically, not licensed for release | clean-worktree package/extract/downstream `cargo check --locked` and `cargo run --locked`, `dev_doc/evidence/package_verification.md`, 2026-09-04 | an extracted-only downstream failure | packaging mechanics; does not supply publication rights |
