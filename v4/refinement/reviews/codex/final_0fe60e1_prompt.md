Original source SHA: 0fe60e155673a2b7b794d4ce389b6609d38815cb
Independent snapshot HEAD: 321a032770cf8ac566475de9b0f5d953479ac479
Review mode: execute; requested model: gpt-6.1-sol; requested effort: xhigh.
Verify effective settings; report personally executed versus inspected-only evidence.
Never push, publish, modify global settings, bypass sandbox, or claim acceptance before triage.

## Supplied document: CODEX_FINAL_ACCEPTANCE.md

# Final independent Concord V4 acceptance review

You are the independent different-provider reviewer, local Codex Sol 6.1 XHigh. Verify the effective settings. Inspect the frozen candidate and origin manifest. Review the product from a user's point of view as well as from a code-correctness perspective.

The goal is a desirable, small typed API-domain layer over reqwest. It must not own auth orchestration, retries, rate limiting or pagination. Do not equate fewer lines or passing existing tests with a good API.

First inspect the public quick start, generated calling surface and neutral tasks without reading the coordinator's favorable narrative. Predict presence/default/override behavior and attempt several tasks. Then inspect internals, decisions, tests and evidence. Ask whether any common task needs an unnecessary native escape, obscure helper, wrong trait bound or unexpectedly large amount of glue.

Review the full runtime and the semantic/codegen boundaries. Recheck native validation preconditions, ordering claims, safe error text, reflected sensitive data, defaults and snapshots, custom formats, native parts/decoder, feature isolation, renamed dependencies and generated docs. Review the inline experiment's reproducibility and causal claims rather than assuming its summary is correct.

With permitted writable disposable storage, run a fresh locked build, important regression tests and independent probes. Never run a prebuilt binary as proof of this SHA unless you have established its provenance. Do not bypass the sandbox; distinguish environment limitations from library failures. Keep all production source unchanged after experiments or report the exact diff and invalidate claims that assume otherwise.

Read the acceptance matrix and verify that each claimed platform/test/measurement has source-bound evidence. Inspect the matrix assertions for tautologies, masked compile-fail cases, ignored features and tests that pass with the reported bug. Request targeted mutation evidence where needed.

Deliver prioritized findings with reproduction/evidence, accepted trade-offs and unmeasured areas, plus a clear verdict for this exact candidate. State which previous findings were independently confirmed resolved. Do not declare universal safety, optimality, exhaustive verification or readiness for a platform you did not execute or observe through a credible source-bound run.

No pushes, publishing, releases or global settings changes. Do not implement a new framework to solve a review finding.


## Supplied document: NOTE.md

# Coordinator note for the final acceptance review (identity and pointers only)

Candidate: `0fe60e155673a2b7b794d4ce389b6609d38815cb` (tree `c76389d9c24a6b5fa601c9ee08d4149c9042ba75`,
`Cargo.lock` sha256 `1880bdf8dfe1402b2e823ebc893b4f1a37025b5e026c73c2372ffccdfc1e8269`) on branch
`concord-v4-refine`; the snapshot's `.concord-review.json` records it. Baseline: the V4 alpha `8f7adc2`.

Suggested reading order, per the owner's prompt: first the user-facing material only —
`README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`, `docs/CALLER_GUIDE.md`,
`docs/RECIPES.md`, `docs/DIAGNOSTICS.md`, the crate docs (`cargo doc -p concord`), `examples/` —
and the neutral task corpus; attempt several tasks. Only then the internals and the coordinator's
records: `v4/SEMANTICS.md` (normative), `v4/refinement/{DECISIONS,STATUS,PERF,REVIEW_TRIAGE}.md`,
`v4/refinement/evidence/`, `docs/MIGRATION.md`, `docs/MAINTAINERS.md`.

Previous independent findings to confirm or refute: milestone review of `6604a53` — F-01 (native
multipart backslash escaping), F-02 (raw-keyword client name), F-03 (strict rustdoc without
default features), F-04 (migration text and contract table) — and the design pass's header-map
capacity panic. Their claimed fixes are listed in `v4/refinement/REVIEW_TRIAGE.md`; treat them as
claims.

Verification the coordinator ran on this exact candidate (a claim for you to check, not evidence
you may reuse): `scripts/verify.sh --msrv`, 11 steps PASS on Linux x86_64, rustc 1.97.1 and
1.85.1. Not run by anyone on this candidate: Windows, macOS.


## Supplied document: ENV.md

# Execution environment for this run

- Sandbox: workspace-write confined to this snapshot, network access enabled for this run only so
  loopback tests can bind sockets (the Linux workspace sandbox forbids all sockets otherwise). Do not
  contact external services; everything needed is local.
- `CARGO_HOME` is a private registry prefetched with `cargo fetch --locked`, located OUTSIDE the
  snapshot (granted as an extra writable root) so trybuild's path normalization behaves as in an
  ordinary checkout. `CARGO_TARGET_DIR` is `target/review-build`. Build fresh with `--locked`
  (add `--offline` if cargo tries the network). Toolchains: 1.97.1 (rust-toolchain.toml), 1.85
  (`cargo +1.85`), nightly.
- Put probes under `target/probes/`; restore or report any production-source change; the original
  source integrity is checked after the run. Stop any server you start before finishing.


## Supplied document: 01_CHARTER.md

# 01 — Product charter and authority

## Mission

Polish the existing Concord V4 into a desirable Rust API-client authoring and calling library. Continue the current code. The earlier clean-slate V3→V4 instructions are historical and must not trigger deletion or another rewrite.

Priority order: current user instructions; this charter's product boundaries; executable evidence and the newly chosen contract; current V4 implementation/specification as a revisable baseline; V3 as historical knowledge. Conflicts must be recorded, not silently papered over.

## What remains inside

Client/scope/endpoint structure, typed ordinary values and their legitimate storage, deterministic construction rules, path/subdomain/query/header composition, sensitivity-aware Concord diagnostics, minimal body/response conversion, and native reqwest interoperability. A little well-justified convenience is allowed when it eliminates repeated application code without acquiring a workflow responsibility.

## What remains outside

Authentication orchestration, credential discovery/fetch/refresh, retry scheduling, rate admission and pagination traversal. Do not reintroduce them through DSL annotations, hooks, core-owned registries, service containers, sidecar schedulers, default-feature crates, or a generic managed executor.

A caller-provided token is ordinary possibly sensitive data. A cursor or page-size query parameter is ordinary endpoint data. Client-local storage is not automatically an auth store. A caller may supply an `Arc<T>` or an application object; Concord must not invent lifecycle or refresh policies for it. Study genuine storage needs without turning Concord into an application-state container.

Application-only examples may show those four workflows to prove composability. They must not become supported Concord subsystems or a project to build four companion crates.

No new OpenAPI importer, multi-client federation, cache framework, request graph, plugin ecosystem, website application, LSP, or documentation-export engine in this campaign. Native request/response use is permitted even when a convenience surface is deliberately absent.

## Product quality priorities

1. Correctness and honest semantics are constraints, not scores that style can outweigh.
2. Predictable author/caller behavior is more important than minimal token count in the DSL.
3. Prefer a small number of orthogonal concepts and one canonical way to do a task.
4. Preserve native compatibility; do not wrap every reqwest option in Concord vocabulary.
5. A small implementation that shifts complexity onto every user is not automatically better.
6. An additional helper/type is acceptable when a real task and tests show a net reduction in complexity.
7. Prefer ordinary Rust when it is sufficient; do not force awkward Rust solely to keep a macro primitive count low.

## Compatibility and permissions

The inspected workspace is an unpublished V4 alpha. Local source compatibility is not sacred: justified changes to its syntax or generated API are authorized. Record a migration note and update all maintained callers. Do not retain a second legacy parser just to preserve the alpha. Inspect local downstream uses before choosing a break.

Local branches/worktrees, commits, tests, docs, benchmark artifacts and scoped specialist edits are authorized. Preserve V3, existing branches, user changes, agent configuration and unrelated files. No force reset, blanket clean, recursive deletion of user paths, publication, push, release, global model setting changes, package/account changes, or paid cloud operations are authorized by this handoff.

Normal design choices do not require another approval round. Pause only for destructive actions, essential unavailable access, or a genuine conflict with the product boundary. Continue independent local work where a platform gate is blocked; report the gate accurately.

## Practical definition of desirability

The standard task should be shorter and more understandable than a good handwritten reqwest wrapper. The unusual task should have a documented, typed exit. A wrong declaration should fail near the cause. A maintainer should be able to trace one endpoint from syntax to request without learning an orchestration system.

Use qualitative evidence and executable tasks. Do not manufacture an aggregate desirability score, infer user preference from line counts, or call agent simulations a human usability study.


## Supplied document: 14_ACCEPTANCE.md

# 14 — Acceptance and delivery

## Implementation is required

The expected outcome is a refined working V4, not an architecture report. All mandatory decisions must have a disposition, all adopted changes must be implemented, and public documentation and callers must match the final contract.

## Acceptance matrix

| Area | Required evidence |
|---|---|
| Product scope | No Concord-owned auth/retry/rate/pagination orchestration, no disguised generic runtime; source/dependency review plus external integration examples |
| Authoring | Representative declarations are compact and legible; difficult semantics have truth tables and executable counterexamples |
| Calling | Constructor/controls chosen through actual tasks; required/optional/default meanings consistent; ordinary callers need no internal types |
| Values | Evaluation timing, snapshots, ownership/trait requirements, empty/absent/nested optional cases covered; deep immutability not overstated |
| Composition | Native URL/header boundaries validated; exact ordering contract tested; no lost/duplicated/accidentally encoded suppressed contributions |
| Core | Native exits and typed decoding work; non-success response ownership preserved; custom formats and selected native streaming path exercised |
| Safety | Sensitive output regressions discriminate, ordinary errors remain useful, native boundary limits explicit; no unresolved high issue |
| Compiler | Hygiene, renaming, visibility, collisions and scale checked; no hidden parallel semantic engine or unnecessary global state |
| Performance | Retained baseline/candidate workloads, consumers, raw samples and configuration; accepted regressions named; no universal claim from a narrow test |
| Docs | Maintained examples compile, key examples run, generated docs useful, fresh-use tasks attempted without internals |
| Verification | Fresh locked candidate verification, feature isolation, downstreams, MSRV and supported native platforms; exact source provenance |
| Review | Independent Codex reports complete and triaged; corrections re-reviewed appropriately; test/evidence audit separate from implementer claims |
| Release hygiene | Package metadata/license/features/migration checked; no push or publication performed |

## Status vocabulary

Use PASS, FAIL, BLOCKED, NOT_RUN, or NOT_APPLICABLE for a gate, with reason and evidence. Use KEEP, CHANGE, DEFER, or REJECT for a proposal. A known defect cannot be deferred as a feature. A platform tested on an older commit is historical evidence, not PASS for the candidate.

No numerical completion percentage is needed. No claim of exhaustive proof, mathematical optimality, universal zero overhead, or human usability validation from agent simulations.

## Definition of done

There is one selected public surface, one implemented semantic contract, compiled/runnable examples, a source-bound verification receipt, a completed independent review, and a clear migration/limitations report. Blocker/high findings are closed or the candidate is not accepted. Medium/low findings are fixed or explicitly justified as non-blocking, not merely renamed.

## Final report structure

State the final branch/worktree/commit/tree and lockfile digest. Summarize the user-visible changes with before/after authoring and calling examples. List kept/rejected design ideas and why, scoped to consequential decisions. Show test and platform results with commands and source references. Report benchmark trade-offs, not just wins. List review findings and disposition. State remaining limitations and whether each is a product boundary, accepted trade-off, unmeasured area or defect.

Include exact reproduction commands and the next user's entry point. The opening paragraph should answer whether the refinement is implemented and accepted, for which platforms and at which revision.

## Artifacts to keep in the repository

Prefer a handful of maintained files: current `SEMANTICS.md`, current API docs, `v4/refinement/STATUS.md`, `DECISIONS.md`, `REVIEW_TRIAGE.md`, `FINAL.md`, and an evidence index with small raw logs/results. Use existing repository conventions where appropriate; do not create parallel current spec/status trees that disagree.

Keep large generated build outputs out of Git. Retain the scripts and seeds required to recreate experiments. Scrub credentials and unnecessary private paths from deliverables, but do not edit archived reviewer conclusions to make them more favorable.

## Package validation is not publication

A package/list/dry-run check may reveal missing files, wrong licenses, path dependency/version mistakes or docs feature gaps. It does not authorize publishing an alpha, merging into main, changing remote branches, creating a release or transferring credentials. Leave those owner actions untouched.


## Supplied document: USER_TASKS.md

# User-task corpus

Use these as neutral task requirements. Preserve the same HTTP behavior and representative values across design candidates. Fixtures run locally; no provider credentials or live network calls are needed. Each task should retain declaration, generated signature, caller code, expected behavior and any intentional unsupported route.

The first 20 are mandatory common/boundary tasks. The remaining tasks are design stressors: each needs an explicit disposition and a working route where it belongs in scope, not necessarily new core syntax. Several tasks can share a fixture.

| ID | Persona and task | Evidence to collect |
|---|---|---|
| U01 | Author a client with one text health endpoint and call it from a separate crate | Smallest complete declaration/caller; no hidden setup |
| U02 | Define JSON read/create endpoints, DTOs and an empty-output delete | Representation consistency, body types, empty semantics |
| U03 | Configure roots with 0, 1, 3 and 8 required inputs | Constructor clarity, missing-input errors, API growth |
| U04 | Use two same-typed identity fields and catch an accidental swap | Distinguish intrinsic positional risk from avoidable constructor ambiguity |
| U05 | Reuse a tenant/region scope for several endpoint calls | Ownership, reuse cost, readable navigation |
| U06 | Set an optional query, leave it absent and set it to an empty string | Predicted versus actual wire cases |
| U07 | Use a required optional payload and a nested optional placement | No type-name heuristics; meaningful nullable payload spelling |
| U08 | Override a default, clone the selection and prepare it twice | Evaluation timing and value retention |
| U09 | Create a child, then alter its parent via a consuming setter | Existing child's snapshot does not accidentally change |
| U10 | Inherit a locale, optionally override it, remove it and reintroduce it | Truth table, syntax predictability, fallback semantics |
| U11 | Repeat a query key from a collection, including an empty collection | Repetition order and absence distinctions |
| U12 | Place a caller-supplied secret header and reuse its value in a URL component | Wire positive control and protected diagnostic negative control |
| U13 | Point a client at a local/staging base while preserving appended path behavior | No authority injection or surprising path reset |
| U14 | Recover an unread non-2xx response and decode an API error document | Useful ownership and status classification |
| U15 | Inspect a prepared request, modify its native timeout/header and reuse its decoder | A real native exit rather than a nominal getter |
| U16 | Implement a small custom body and response representation | Minimal extension role; clear failure contract |
| U17 | Add a wrong ancestor reference, duplicate name and incompatible value | Actionable diagnostics near the cause |
| U18 | Consume the generated API through a renamed dependency and macro_rules wrapper | Downstream hygiene, naming, visibility |
| U19 | Install/use no-default and JSON-only configurations in separate consumers | No dev-dependency or feature-unification accident |
| U20 | Learn the library using only quick start and generated rustdoc | Missing docs, misleading controls, undocumented workarounds |
| U21 | Give one domain type different path/query/header spellings | Newtype versus adapter cost; no blanket serialization guess |
| U22 | Use a foreign timestamp/ID type with a human-facing Display | Orphan/coherence constraints; explicit wire spelling route |
| U23 | Declare many JSON endpoints with a binary and empty exception | Representation-default experiment and exception readability |
| U24 | Store caller-local typed context not directly sent on the wire | Wrapper versus minimal storage; no artificial placement or lifecycle framework |
| U25 | Pass a large clone-expensive value and a move-only value | Trait bounds and hidden clone costs; deliberate limitation if needed |
| U26 | Use an interior-mutable caller object in a selected scope | Honest structural snapshot contract; no claim of deep freeze |
| U27 | Configure conditional endpoints/visibility/deprecation in ordinary Rust modules | Attribute semantics, generated reachability, rustdoc |
| U28 | Receive a raw stream, stop early, and use an application-owned timeout | Native streaming proof, no Concord stream scheduler |
| U29 | Send a native/custom multipart or form body | Documented native/custom route; do not require a new subsystem |
| U30 | Execute an application-owned finite cursor sequence and bounded safe retry | No core policy changes; clear replay/safety assumptions |
| U31 | Apply caller-owned admission before execution and update supplied authorization | No refresh/admission hooks smuggled into Concord |
| U32 | Model an uncommon query sequence with interleaved repeated keys | Core support or a small explicit native rewrite; precise ordering scope |
| U33 | Use valid escaped/IDNA bases and invalid host/header boundary inputs | Static/runtime validation consistency and failure stage |
| U34 | Declare 1,000 endpoints and consume a few, many, and from multiple crates | Reproducible compile/codegen/fanout trade-offs |
| U35 | Modify one diagnostic or wire-format example as a new maintainer | Code ownership and traceability, not a giant architecture tutorial |
| U36 | Upgrade an existing V4-alpha caller after refinement | Accurate migration, no hidden dual grammar |

## Fixture contract example (not proposed DSL)

A regional catalog has base `/v1`, a caller-supplied region host label, tenant scope, a required product ID, an optional category, a default page size, a repeated tag query and an ordinary sensitive header. A search returns a JSON envelope containing items and an optional next cursor. An update accepts JSON and may return an unread JSON error at non-2xx. A download endpoint returns a native stream through the chosen exit.

The author must express the domain. The caller must make the ordinary calls without reading source. The integration author must implement the finite multi-request loop outside Concord. Do not give the fresh participant the current DSL rendition before its initial prediction/design attempt.

## Fresh-review variations

The independent reviewer may add a new optional default, reuse one input under two wire spellings, move a parameter between scope and endpoint, rename the dependency, or add another same-typed root input. Retain the variation and observations. These are falsification tasks, not a benchmark score or a claim about human users.


## Supplied document: CONTRACT_CASES.md

# Contract and regression matrix

Map each applicable case to a real test/assertion. Cases describe requirements or decisions to resolve; they are not all promises that the baseline already implements. Use PASS / FAIL / BLOCKED / NOT_RUN / NOT_APPLICABLE, with source identity. A deliberate unsupported case needs a documented route or explicit product boundary.

## Values and ownership

| ID | Case | Discriminating witness |
|---|---|---|
| V01 | Missing required input | Separate downstream compile failure or selected builder's explicit build error |
| V02 | Required empty string versus missing | Empty reaches the wire; missing follows declared requirement |
| V03 | Default evaluation order and frequency | Independent counters; overridden defaults handled exactly as specified |
| V04 | Clone/prepare/send do not re-evaluate retained choices | Same selected value across all operations, not just matching final URLs once |
| V05 | Absent versus present empty | Key absent versus `key=` |
| V06 | Required Option payload versus optional placement | Distinct type/slot behavior without type spelling inference |
| V07 | Optional initialized value cleared explicitly | Correct absence/override semantics and inference for None |
| V08 | Existing child survives later parent setter | Snapshot value unchanged while new child observes change |
| V09 | Payload interior mutability | Documented caller-owned behavior, no false deep-immutable claim |
| V10 | Non-Clone, large Clone and borrowed inputs | Accepted ownership policy tested; no body-dependent accidental restriction |
| V11 | Same-type required inputs | Predicted ergonomic risk documented or reduced by selected construction |
| V12 | No hidden conversion or numeric widening | Separate compile cases; literal inference preserved |
| V13 | Independent clients and shared native client | No unintended Concord state sharing; intended native sharing retained |
| V14 | Caller-local non-wire data | Practical typed wrapper/storage route; no fabricated wire use |

## Composition and encoding

| ID | Case | Discriminating witness |
|---|---|---|
| C01 | Path hierarchy and base prefix | Exact URL from several scopes |
| C02 | Slash/percent/Unicode/question/hash in dynamic values | Encoded exactly once as a single value, not URL structure |
| C03 | Dot/empty segments and trailing slash | Explicit selected rules, error stage and unchanged base cases |
| C04 | Host labels, length and IP bases | Native-compatible boundary tests; no panic |
| C05 | Static versus runtime bases | Identical meaning for shared accepted inputs; deliberate differences documented |
| C06 | Native header-name preconditions | Maximum/beyond-maximum literal generated reproducibly; no runtime panic |
| C07 | HeaderMap capacity failures | Fallible result or established precondition; no ordinary-data panic |
| C08 | Introduction, append, override and exact replacement | Truth-table cases for present/absent/multiple ancestor occurrences |
| C09 | Present-empty repeated override | Clears or preserves exactly as chosen; differs from absent optional |
| C10 | Remove/reintroduce and repeated occurrences | Stable logical order and liveness meaning |
| C11 | Suppressed formatter never runs | Counter or deliberate failing formatter on removed branch |
| C12 | Query ordering versus header ordering | Query order exact; same-name header values checked; no global wire-order assumption |
| C13 | Native default headers | Omitted Concord contribution does not pretend to remove native defaults |
| C14 | One domain type, multiple wire spellings | Explicit per-use representation route or documented newtype alternative |
| C15 | Invalid header value/content type | Value-free build error, no constructor panic; static literals diagnosed early where promised |
| C16 | Long/deep declarations | Bounded test resource use, deterministic diagnostics, measured algorithmic growth |

## Bodies, response and native integration

| ID | Case | Discriminating witness |
|---|---|---|
| R01 | JSON/text/binary/no typed output | Independent expected bytes/outputs |
| R02 | Absent versus zero-length body | Actual transport facts, not an assumed Content-Length |
| R03 | Custom Encode/Decode | Extension implemented in a downstream crate |
| R04 | Unit response draining | Peak-memory positive control; mutated buffering must fail |
| R05 | Truncated response | Transport/read error rather than false success |
| R06 | Non-2xx unread response ownership | Recover status/headers and body after error |
| R07 | Native modification plus retained decoder | Modified request reaches server and decoded type remains correct |
| R08 | Raw streaming and early drop | Actual streaming fixture; no untested claim from an accessor's presence |
| R09 | Multipart/form or other native body | Ordinary native/custom path with a supported fixture |
| R10 | Concord no-I/O construction | Loopback admission count and no execution before send; user callbacks scoped separately |
| R11 | One execute versus exactly-once | Tests/wording distinguish Concord orchestration from native policy |
| R12 | External policy examples | Retry/cursor/admission/auth data outside library with zero core modifications |

## Diagnostics and sensitivity

| ID | Case | Discriminating witness |
|---|---|---|
| S01 | Explicit marker and safety-name policy | Wire contains value, protected Debug does not |
| S02 | Reuse across path/host/query/header | Every protected placement follows effective sensitivity |
| S03 | Override/removal/reintroduction | Sensitivity not accidentally cleared |
| S04 | TLS redirect reflects query | Raw chain positive control, normal error negative control |
| S05 | TLS redirect reflects header | Same, without sensitive original URL parts |
| S06 | Body decoder quotes input | Ordinary error never quotes it; explicit raw accessor boundary documented |
| S07 | Runtime base contains private data | Selected base policy stated and tested; no accidental redaction promise |
| S08 | Useful safe classification | Callers handle kind/status/stage without raw error text |
| S09 | Custom callback error/display behavior | Trusted-user-code boundary explicit; no blanket guarantee |
| S10 | Sensitivity graph scale | Independent test model plus branching and chain stress |

## Compiler, documentation and reproducibility

| ID | Case | Discriminating witness |
|---|---|---|
| T01 | Rename and macro forwarding | Real separate consumer crates |
| T02 | Names/raw identifiers/visibility/collisions | Separate diagnostic cases, both relevant spans where possible |
| T03 | cfg/docs/deprecation policy | Every accepted attribute's generated surface works coherently |
| T04 | UI compiler-version variance | Pinned snapshots; semantic failure verified separately on supported compilers |
| T05 | Feature isolation | Independent consumer/package builds, not only a unified workspace |
| T06 | Generated rustdoc and snippets | Actual rendered inspection plus compiled/runnable examples |
| T07 | Current toolchain and MSRV | Fresh source-bound commands with lockfile |
| T08 | Native Linux/Windows | Actual execution receipts or explicit blocked candidate gate |
| T09 | Packaging | Local package contents/dependencies/features checked, no publication |
| T10 | Benchmark reproducibility | Retained generator, consumer, config, raw runs and before/after identities |
| T11 | Claim-to-test audit | Independent reader identifies the exact assertion for each material PASS |
| T12 | Mutation discrimination | Baseline pass, mutant fail, restored pass in disposable snapshots |

## Small independent semantic oracle

Represent an owner chain and ordinary value states in a test-only straightforward structure. Apply the chosen composition rules independently, compute expected occurrences/sensitivity, and compare with compiled fixtures or resolved-model outputs. Do not call the production composition routine to obtain the oracle result. Include minimized seeds for failures. This oracle is not a new public runtime or serialized contract format.
