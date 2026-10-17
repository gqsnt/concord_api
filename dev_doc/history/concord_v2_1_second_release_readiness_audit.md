# Historical input — not current source of truth

This audit is retained as the remediation input that preceded the current
canonical status. Its findings do not override the corrected active
specification, [`release_gate_status.md`](../release_gate_status.md), or the
current exact-tree evidence.

# Concord v2.1 — second release-readiness audit

**Audited workspace:** `concord_api2(1).zip`
**Audited branch:** `release/v2.1-readiness`
**Audited commit:** `adad5326de2e2569016ba6f1f01cfe76b2ddf0be`
**Baseline branch:** local `main` at `f7464c58f4a60b7332547e8f91039a269171a6ff`
**Audit mode:** static source, Git, documentation, release-process, and external package-identity review
**Mechanical limitation:** the audit environment contains no Rust/Cargo/Just/PowerShell toolchain, so the agent’s Rust test and benchmark executions were inspected as recorded evidence but were not independently reproduced.

---

# 1. Executive decision

## 1.1 Current verdict

**Do not merge `adad5326` into `main` as it stands.**

The agent made the correct immediate decision by preserving a clean release branch and declining to merge while mandatory gates remained open. The branch is a strong, serious implementation candidate, and the majority of the architectural remediations are real. It is not merely a documentation prototype.

However, the latest workspace reveals several additional blockers that were not identified in the agent’s final report:

1. the public Cargo package name `concord` is already owned by an unrelated crates.io project;
2. the active documentation selects optional static secrets, but the parser and semantic model do not implement them;
3. `with_*` was removed from generated APIs but remains reserved in collision analysis, rejecting otherwise legitimate endpoint/scope names;
4. the claim–evidence ledger states the baseline package version was `1.0.0`, while the actual baseline manifest is `2.0.0`;
5. the claimed `git diff --check` evidence checked only an empty working-tree diff; the actual `main...release` diff fails;
6. active repository documents disagree on whether the branch is a prototype or a release candidate;
7. the full aggregate verification was recorded at `9470e24`, while the audited HEAD is a later documentation commit;
8. documentation source-anchor and retired-language checks run in `just docs` but are not run by GitHub Actions;
9. the production executor still reads `SystemTime::now()` directly for HTTP-date `Retry-After`, partially contradicting the stated injected-time claim;
10. no actual Linux/macOS/Windows CI run has exercised this unpushed release branch;
11. the human-DX packet is carefully prepared, but its two-variant comparison is underpowered at its declared minimum of three participants.

The project is therefore in this state:

```text
architecture selected and largely implemented
+ substantial machine evidence reported
+ important security and runtime corrections landed
− source-of-truth contradictions remain
− publication identity is unresolved
− legal authority is unresolved
− human comprehension is untested
− exact final HEAD has not received the full aggregate rerun
= strong integration candidate, not merge-ready under the project’s current policy
```

## 1.2 Recommended next state

Continue on the existing `release/v2.1-readiness` branch, or create a short-lived `fix/v2.1-release-truth` branch from `adad5326`. Do not restart the architecture and do not create a v2.2 design branch.

Close a new **Gate 0 — package identity, legal authority, and release namespace**, then resolve the concentrated source-of-truth defects, run the exact final candidate on actual supported platforms, execute the human studies, and only then perform the local no-fast-forward merge.

## 1.3 Merge policy recommendation

The cleanest path is:

```text
keep release branch
→ fix P0 contradictions
→ owner selects license and package namespace
→ run actual platform CI
→ perform human DX and independent Rustdoc/diff review
→ apply findings
→ rerun exact final candidate
→ set final version
→ merge --no-ff into local main
→ rerun full verification on merged main
```

A beta integration merge is possible only if the project owner explicitly changes the repository policy so that `main` is a development branch rather than a stable-ready branch. That would require:

- using a beta/preview version, not `2.1.0`;
- restoring `publish = false` until package identity and licensing are solved;
- recording that human gates remain open;
- forbidding tag/publication.

That is an available policy choice, but it is not the current project policy and is not the recommended default.

---

# 2. Epistemic contract of this audit

The audit separates four kinds of statement.

## 2.1 Observed directly in the supplied workspace

Examples:

- exact Git refs and commit ancestry;
- files and public signatures present in source;
- parser behavior visible in code;
- documentation claims and contradictions;
- manifest metadata;
- branch-diff whitespace failures;
- CI job definitions;
- source-level use of `SystemTime::now()`;
- absence of a license file.

## 2.2 Reported by the implementation agent

Examples:

- test counts;
- benchmark ratios and memory measurements;
- exact Cargo/Clippy/Rustdoc command results;
- Windows-host performance figures;
- package extraction/downstream execution results.

These claims are plausible and supported by committed evidence documents, but this audit could not rerun them.

## 2.3 Externally verified

Examples:

- Cargo publication requirements and name allocation rules;
- current occupation of the `concord` crates.io name by an unrelated Discord TUI;
- Cargo’s ability to give a package and its library target different names;
- SemVer implications of public trait/API changes.

## 2.4 Inferred or recommended

Examples:

- whether a defect blocks merge, beta integration, or stable publication;
- the preferred package-name migration strategy;
- whether observer URL authority should be narrowed before first public release;
- whether one human can serve in more than one evidence role;
- the preferred commit/merge sequence.

These are judgments grounded in the selected v2.1 product model and Systems Thinking v3, not direct facts.

---

# 3. What the agent did correctly

# 3.1 Git and preservation discipline

The release process is unusually disciplined.

Observed Git state:

```text
main                              f7464c58  v2.0
backup/v2.1-prototype-unreviewed  50460e7d  checkpoint
release/v2.1-readiness            adad5326  audited HEAD
origin/main                       afcea24a
```

Properties:

- `main` remains unchanged;
- the original large dirty prototype was committed to a backup branch;
- the release branch is clean;
- no tag was created;
- no remote push or publication occurred;
- the release branch is a descendant of local `main`;
- a complete commit sequence records remediation and evidence work.

This is a strong preservation boundary. The agent avoided both destructive history rewriting and an unsafe merge motivated by task completion pressure.

The first checkpoint is huge—491 changed files and more than 33,000 changed lines—but that is understandable because it captures a pre-existing dirty prototype rather than pretending to be a reviewable semantic commit. The subsequent commits are substantially better separated by purpose.

Do not rewrite this history merely to make it aesthetically perfect. A synthetic rebase of hundreds of files would create fresh risk without improving the release claims. Preserve it and make the eventual merge commit explain the architecture and gate evidence clearly.

# 3.2 The branch is a real implementation

Static inspection confirms real code across:

- parser and semantic model;
- generated direct/unbound API;
- request/response representation pairs;
- atom and ordered-value encoding;
- endpoint status/effect;
- one executor and deadline;
- authentication slots/recovery;
- rate plan/admission/feedback;
- pagination controller/page model;
- stream carriers and private erasure;
- request planning, descriptions, semantic diff;
- runtime endpoints;
- extras and test support.

The branch changes 551 tracked files relative to `main`, with 24,325 insertions and 13,615 deletions. It contains 504 Rust files, 65 Markdown files, more than 1,000 test annotations, and 141 checked compiler-error snapshots. Raw counts do not establish correctness, but they disconfirm the hypothesis that the result is only a speculative document.

# 3.3 The highest-value semantic corrections are present

## Exact payload type and explicit Concord presence

The implementation separates:

```text
Rust payload type
Concord outer presence
initializer/default
placement cardinality
```

That preserves aliases and nested `Option` semantics. It is one of the strongest v2.1 decisions.

## Explicit scalar and sequence roles

`RequestAtom`, `AtomEncoder`, `OrderedValues`, and `each` exist as separate capabilities. This prevents `Display`, `Vec`, or arbitrary `IntoIterator` from becoming hidden protocol languages.

## Domain-truthful endpoints and representation pairs

Request and response representation traits are generic over the exact input/output type. The generated endpoint arrow remains the caller’s actual output type. Acquisition drivers remain core-controlled.

## Effect classes

The implementation distinguishes:

```text
safe
idempotent
non_idempotent
```

This is stronger than a binary replay flag and gives pagination a more truthful gate than “technically repeatable.”

## Explicit pagination completeness

`PageLimit` contains distinct variants for prefix and natural completion, and the generated terminal accepts the exact type. The earlier numeric coercion has been removed.

## Typed response feedback

Rate-limit response feedback contains status and parsed server delay rather than arbitrary headers, and it cannot replace the current response result. This is the correct authority direction.

## Response observer information-flow repair

General observation no longer receives arbitrary response-header values. `ApiResponse::headers()` deliberately remains an explicit application-level authority, while its `Debug` implementation hides URL, values, and response-header contents.

## Truthful synchronous observer semantics

The `OperationObserver` documentation now says what core can and cannot guarantee:

- synchronous and inline;
- read-only and non-fallible at the type boundary;
- panic contained;
- not preemptible if it blocks;
- intermediate observer time reduces remaining budget;
- final observer time may delay delivery after result fixation.

That is materially more honest than claiming an async deadline can preempt arbitrary synchronous Rust.

---

# 4. Accuracy review of the agent’s final response

| Agent statement | Audit judgment | Explanation |
|---|---|---|
| Release branch is clean and preserved | **Accurate** | `git status` is clean and HEAD/ref values match. |
| Main remains unchanged and no merge occurred | **Accurate** | Local `main` is still `f7464c58`; release is separate. |
| No push, tag, publish, or force operation occurred | **Supported by local history** | No local tag exists; remote branch is not present in supplied repository. |
| Endpoint/runtime `accept`, active `clear_*`, generated `with_x`, and numeric pagination coercion were removed | **Mostly accurate** | Public emission/removal is real; however `with_*` remains reserved, and optional-secret docs/parser diverge. |
| Arbitrary response-header values no longer reach observers/rate feedback | **Accurate for those interfaces** | Event and limiter types expose typed metadata only. Explicit `ApiResponse::headers()` remains deliberate caller authority. |
| Observer timing semantics are truthful | **Accurate** | Core docs explicitly acknowledge non-preemption and post-result delivery delay. |
| Riot/focused fixtures exercise representation inheritance | **Supported statically** | Source/tests and evidence documents exist. Human discoverability remains untested, correctly assigned to Gate 3. |
| Stream comparison passed budgets | **Agent-reported and plausibly evidenced** | Synthetic single-host evidence exists; cannot be independently rerun here. |
| Final exact-tree `just verify` passed every recipe until `cargo deny` | **Reasonable but not exact for HEAD** | Full clean evidence is recorded at `9470e24`; audited HEAD `adad532` is later. |
| `git diff --check` passed | **Misleading** | It checked the empty working-tree diff. `git diff --check main...HEAD` fails. |
| Supplied baseline version was `1.0.0` | **False** | `git show main:Cargo.toml` reports `2.0.0`. |
| One coherent public language remains | **Not fully true** | Docs select `secret x?`, while parser accepts only `secret x;`. |
| Gate 7 fully proves injected/core time behavior | **Overstated** | Production response handling still directly calls `SystemTime::now()`. |
| Gate 10 is blocked by reviewer and license only | **Incomplete** | Package namespace collision, metadata, real CI, branch-diff hygiene, and exact-HEAD verification remain. |
| Publication is mechanically ready except license | **False as a package claim** | `concord` is already an unrelated crates.io package; metadata is incomplete. |

The final report is therefore honest in spirit and substantially accurate, but it is not yet a complete release authority document.

---

# 5. Critical finding: package identity is a new Gate 0

# 5.1 `concord` cannot be assumed publishable

The façade manifest declares:

```toml
[package]
name = "concord"
version.workspace = true
```

The crates.io name `concord` is already used by an unrelated Discord terminal client. This is not a hypothetical namespace concern. Cargo allocates names first-come-first-served, and only crate owners can publish new versions.

Even if the project owner happened to be granted ownership, publishing an unrelated HTTP-client library as version `2.1.0` of an existing Discord application would violate package identity and user expectations. It should not be treated as a legitimate continuation.

# 5.2 Recommended package strategy

Select a new package namespace after checking it immediately on crates.io. One plausible family is:

```text
concord-api
concord-api-core
concord-api-macros
concord-api-extras
```

This is a candidate, not a reservation. Availability must be checked and then secured by the project owner before public release.

The user-facing Rust crate path can remain ergonomic:

```toml
[package]
name = "concord-api"

[lib]
name = "concord"
```

Cargo permits the library target name to differ from the package name. Downstream applications could therefore write:

```toml
concord = { package = "concord-api", version = "..." }
```

and continue using:

```rust
use concord::api;
```

Internal packages can likewise use dependency aliases where useful. This separates registry identity from source-level ergonomics.

# 5.3 Required namespace work

Before publication:

1. project owner decides the package family;
2. verify ownership/availability for every public package name;
3. reserve or publish according to a deliberate release process;
4. update all `name`, `package`, exact-version dependency, `-p`, patch, packaging, CI, docs.rs, README, and release-order references;
5. regenerate `Cargo.lock`;
6. package and consume extracted artifacts under their final names;
7. run actual dry-run/local-registry release rehearsal.

# 5.4 Restore publication safety meanwhile

The v2.0 baseline had `publish = false` in all four public manifests. v2.1 removed it before the package identity and license were solved.

Restore `publish = false` until Gate 0 is closed. Remove it only in a dedicated release-enablement commit after:

- package names are owned;
- license is selected;
- metadata is complete;
- release order is rehearsed;
- all gates are passed.

This is a balancing control against accidental publication. It does not solve the public repository’s licensing question, but it restores the previous explicit Cargo safety boundary.

---

# 6. Critical finding: the project has no license authority yet

No `LICENSE`, `LICENSE-*`, `COPYING`, or equivalent file exists. The public package manifests contain descriptions but no:

- `license` or `license-file`;
- `repository`;
- `homepage`;
- `readme`;
- keywords/categories.

Cargo’s publication guidance explicitly expects the first five fields before publishing. More importantly, choosing a license is not a technical inference the agent is authorized to make. The project owner must select it.

## 6.1 Why this blocks more than `cargo deny`

Without a license grant:

- the public GitHub repository is visible but not automatically open source;
- downstream users do not have a clear grant to copy, modify, or redistribute;
- crates cannot honestly declare license metadata;
- dependency-policy checks correctly fail;
- contributor expectations are undefined.

## 6.2 Required owner decision

The owner should select one deliberate policy, for example:

- MIT;
- Apache-2.0;
- dual MIT OR Apache-2.0;
- another explicit license compatible with project goals.

The audit does not choose one.

After the decision:

- add license file(s);
- add SPDX metadata to all public packages;
- add repository/homepage/readme metadata;
- define contribution licensing in `CONTRIBUTING.md` if contributions are expected;
- rerun `cargo deny` and package verification.

---

# 7. Source-of-truth defects that must be fixed before merge

# 7.1 Optional static secrets: active docs, absent implementation

Active documents declare:

<!-- concord-dsl -->
```rust
credentials {
    secret optional_api_key?;
    credential optional_key = api_key(secret.optional_api_key);
}
```

The active specification says:

```text
credential_decl := attrs secret ident [ ? ] ;
```

ADR 0010 and claim C13 select optional static secrets provisionally.

The parser, however, does exactly this:

```rust
body.parse::<keyword::secret>()?;
let name: Ident = body.parse()?;
body.parse::<Token![;]>()?;
secrets.push(SecretDecl { name });
```

There is no optional marker in `SecretDecl`, no branch parsing `?`, no positive compiler fixture, and no generated optional-secret construction surface.

This violates the repository’s own documentation contract: active docs are required to describe implemented branch behavior.

## Recommended resolution

**Defer/remove optional static secrets from v2.1 active language** unless the owner explicitly states that this feature is required for the first release.

Why removal is the minimum intervention:

- endpoint-backed credential slots already model initially unavailable material;
- custom providers can report unavailability through their semantic acquisition operation;
- optional private authority has different security/capability meaning from optional public data;
- implementing it correctly requires builder methods, derived-source availability rules, placement failure semantics, clone/security-context behavior, docs, UI tests, and auth recovery tests;
- Gate 3 has no human evidence for the feature.

Required edits if deferred:

- remove `?` from `docs/spec.md` credential grammar;
- remove optional-secret examples from `docs/auth.md` and migration docs;
- change ADR 0010 to `Deferred`/`Rejected for 2.1`;
- change C13 to `Deferred`;
- update repository change map and finalization table;
- add compile-fail diagnostic for `secret x?` explaining alternatives;
- retain a revisit trigger based on a real multi-auth API.

If the owner instead selects implementation, do not merely parse punctuation. Implement the complete private-state contract and corresponding evidence.

# 7.2 Removed `with_*` methods remain reserved

Generated emitters and tests correctly remove `with_x(T)`. Yet semantic collision validation still reserves:

```text
with_<value>
omit_<value>
reset_<value>
```

for optional values.

This means a valid-looking domain child such as:

<!-- concord-dsl -->
```rust
GET with_locale(...) -> ...
```

can be rejected even though no generated `with_locale` method exists.

That is a hidden compatibility tax. Removing a public capability should release its namespace unless another actual generated method needs it.

Required correction:

- remove `with_` insertion from setter and child-method collision models;
- remove the active spec statement reserving it;
- add compile-pass fixtures for scope/endpoint names such as `with_locale`;
- keep a compile-fail fixture proving the optional value itself has no generated `.with_locale(...)` control.

The two facts are compatible:

```text
child method named with_locale may exist
optional value does not generate a with_locale control
```

# 7.3 Stale `host` diagnostic

The rate-key grammar uses `origin`, but one parser error still says reusable dimensions accept `host`, `endpoint`, or a formal.

This is small but important because compiler diagnostics are part of the DSL. Fix the message and its snapshots/tests.

# 7.4 Claim C0 is factually false

The ledger states baseline `f7464c5` had workspace package version `1.0.0`. Direct Git inspection shows:

```text
f7464c5 subject: v2.0
workspace version: 2.0.0
```

A claim–evidence ledger loses authority quickly if an “Observed” row contradicts its cited command.

Correct C0 and any downstream migration/version reasoning derived from it.

# 7.5 Prototype versus release candidate contradiction

Active documents currently say all of the following:

```text
README: implementation prototype; not a release candidate
FINAL: implementation prototype; not a release candidate
release_gate_status: technically exercised release candidate
Cargo version: 2.1.0-rc.1
ADR 0012: selected for release candidate
```

Choose one lifecycle model.

Recommended state until human and package gates close:

```text
status: integration candidate / beta prototype
version: 2.1.0-beta.1 (or project-specific preview version)
publish: false
```

Use `rc` only when the team believes no intentional public semantic change remains unless a release-blocking defect is found. Gate 3 exists precisely because public DSL decisions may still change after user evidence, so `rc` currently overstates commitment.

If the project intentionally defines RC as “mechanically complete but human evidence pending,” document that nonstandard policy once and remove contradictory wording. The beta route is clearer.

# 7.6 Branch-diff hygiene claim is wrong

The recorded command `git diff --check` returned zero because the working tree was clean. It did not inspect committed changes against `main`.

The meaningful command is:

```bash
git diff --check "$(git merge-base main HEAD)" HEAD
```

That currently fails on trailing whitespace and extra EOF blank lines in the root remediation plan, several ADRs, the baseline document, and one UI source file.

Fix the files and add the merge-base form to the release check. A clean working tree and a clean branch diff are separate properties.

# 7.7 Exact final HEAD has not received the full aggregate verification

The release-status document correctly says the aggregate clean verification occurred at `9470e24`. The audited HEAD `adad532` is later and changes ten release/evidence documents.

Those changes are unlikely to break Rust code, but they can break:

- source anchors;
- links;
- retired-vocabulary checks;
- policy consistency;
- package metadata claims.

After all corrections, run the entire maintained suite on the exact candidate commit. Record the full SHA, environment, and outputs. Then run it again after the merge commit on `main`.

# 7.8 Documentation checker is not part of actual CI

`just docs` runs `scripts/check_docs_examples.ps1`, which validates:

- source anchors;
- DSL versus pseudocode fences;
- links;
- documentation inventory;
- retired public vocabulary.

The GitHub Actions documentation job runs doctests/Rustdoc but does not run this script. `just workflow` only validates YAML syntax through actionlint; it does not execute CI.

Consequences:

- local Windows checks can pass while Linux CI never exercises the source anchors;
- stale public vocabulary may enter through a pull request;
- the release claim depends on a PowerShell-only script absent from the CI job.

Recommended correction:

- port the checker into `concord-xtask docs-check`, or make the script reliably cross-platform under installed PowerShell;
- call it from both `just docs` and GitHub Actions;
- add semantic checks for optional-secret grammar, reserved generated names, and status wording—not only method-call regexes;
- make the CI job use the exact same canonical command.

# 7.9 Process artifacts create competing authority

Tracked repository root contains:

```text
PROMPT_FINISH_CONCORD_V2_1_AND_MERGE_LOCAL.md
CONCORD_V2_1_RELEASE_READINESS_REMEDIATION_PLAN.md
FINAL.md
```

The workspace ZIP additionally contains an ignored `concord_v2_1/` directory with stale implementation-kit material. It is not tracked, but it is distributed in the workspace archive and contains older decisions.

Several tracked planning documents also remain written in future tense or preserve superseded choices, including `implementation_v2_1.md` and `repository_change_map.md`.

Recommended cleanup:

- remove ignored `concord_v2_1/` from final distributed workspace;
- move the implementation prompt and original remediation plan to a historical archive outside active product docs, or mark them prominently `SUPERSEDED`;
- keep one current release-readiness document in `dev_doc/`;
- update or supersede planning documents that no longer describe actual implementation;
- let `docs/spec.md`, `docs/design_invariants.md`, `docs/execution_policy.md`, current ADRs, and the claim ledger be the active authority set.

This reduces reading radius and prevents an older prompt from silently outranking current code.

---

# 8. Core/runtime audit

# 8.1 One executor and operation model

The selected architecture remains strong. Static source confirms one managed execution path with distinct identities for:

- logical operation;
- pagination aggregate/page;
- visible attempt;
- transport-private physical recovery;
- auth recovery;
- policy retry.

The core tracks monotonic dispatch knowledge and uses one absolute operation deadline. This is architecturally coherent with the DSL’s effect and replay semantics.

# 8.2 Observer boundary

The `OperationObserver` now has appropriately narrow result authority. It returns unit, cannot mutate the request, and cannot change the fixed result. Panics are caught.

The timing contract is correctly documented as cooperative rather than preemptive.

## Residual operational issue: swallowed observer panic

`observe` currently catches and discards observer panic without any visible diagnostic:

```rust
let _ = catch_unwind(...);
```

This protects the operation result, which is correct. But an observer can fail permanently and silently, causing loss of telemetry.

Recommended P2 improvement:

- increment a bounded internal diagnostic counter;
- optionally emit one rate-limited trusted diagnostic;
- never fail the current request;
- avoid recursive observer notification.

This is not a merge blocker if documented, but should be considered before first public stable release because observability failure otherwise becomes invisible.

# 8.3 Observer URL authority

Observer and limiter request contexts include `observed_url`, described as credential-sanitized but possibly containing PII or operational identifiers.

This is an explicit authority, not an accidental leak. Nevertheless, it is broader than many observers/limiters require.

Before first stable release, decide whether general events should expose:

1. full sanitized bound URL; or
2. endpoint ID plus structural route/template and selected safe facts; or
3. full URL only under an explicit observation policy.

The current design is defensible because installing an observer/limiter is an application-owned trusted choice and `#[sensitive]` values are redacted. Still, least authority favors removing full URLs from `RateLimitRequest` unless a concrete limiter needs them. A limiter already has endpoint, method, logical host, page/attempt identity, and canonical policy plan.

Classify this as P1 before first public API freeze, not an emergency rewrite.

# 8.4 Server-delay time source contradiction

`parse_server_delay` accepts a `SystemTime` parameter and is directly testable. Production execution nevertheless calls:

```rust
parse_server_delay(response.headers(), SystemTime::now())
```

The terminal-status fallback also calls `SystemTime::now()`.

This means the claim “parsed once using injected/core time” is only partially true:

- parsing is centralized;
- the production wall clock is still ambient;
- deterministic tests can test the helper but not necessarily the complete production path under an injected wall clock.

Recommended fix:

- add a private/core time source to the immutable runtime/client context;
- capture wall-clock time once when the response head is observed;
- pass the parsed `ServerDelayHint` to retry, limiter feedback, and terminal status capture;
- keep the clock implementation private unless downstream customization has a proven role;
- use deterministic test control through core/test-support.

At minimum, downgrade the claim if implementation is not changed. Because Gate 7 currently claims injected time, this should be corrected before calling the gate fully passed.

# 8.5 Rate-limit authority

Strong aspects:

- complete immutable plan;
- canonical length-framed key encoding;
- finite key and window bounds;
- cost cannot exceed capacity;
- explicit limiter selection for non-empty policy plans;
- response observation cannot fail the current result;
- typed server delay only;
- future admission state belongs to limiter implementation.

## Residual multi-policy accounting question

Whole-plan admission is described as conjunctive. For process-local implementation, atomic admission can avoid partial debit. For external/distributed limiters, several independent keys/authorities may not support rollback.

The public contract should say whether:

- `acquire` is one atomic whole-plan request to the implementation; or
- core calls several policies and earlier reservations may be conservatively consumed if a later one rejects.

The current interface gives the entire `RateLimitPlan` in one call, which is the better design. Document that implementors own atomic/conservative accounting and that a granted `RateLimitPermit` is whole-plan admission. This is primarily documentation and extension-contract clarity.

# 8.6 Authentication

Authentication remains one of the strongest subsystems:

```text
secret/provider source
→ versioned slot
→ current material
→ private placement
→ exact challenge attribution
→ bounded source-specific recovery
```

Versioning, stale completion rejection, single-flight, endpoint-backed store, private target enforcement, and one recovery replay are appropriate.

The optional-secret contradiction must be resolved, but the core architecture should not be redesigned.

# 8.7 Response and explicit caller authority

`ApiResponse<T>` intentionally exposes actual accepted response headers and the credential-sanitized logical URL to application code. Its `Debug` omits:

- output value;
- URL;
- all header values.

This is a sound distinction:

```text
explicit caller accessor authority
≠ automatic logs/errors/observer authority
```

The docs should repeatedly state that public response metadata may still contain sensitive application data and that callers own later logging/storage.

# 8.8 Pagination metadata authority

`PaginationDescription` allows a controller to declare up to 16 response-header names and retain at most 64 KiB of corresponding values. `PageResponse` exposes only those values plus optional current logical URI, status, correlation, and decoded page.

This is narrow enough for `Link` and cursor headers compared with exposing the native response.

Residual concern:

- name-based sensitive-header rejection cannot know that an arbitrary custom header contains sensitive data;
- the controller receives exact values and therefore becomes an explicit sensitive-data owner.

Document this as an extension-author obligation. Consider an explicit sensitivity classification for declared header interests only if real use demonstrates the need; do not add a large metadata policy grammar preemptively.

---

# 9. DSL and generated-usage audit

# 9.1 Overall quality

The active v2.1 DSL is conceptually strong:

- literal client/scope/endpoint containment;
- opaque Rust payload types;
- explicit Concord optionality;
- retained defaults;
- explicit owner namespaces;
- canonical request atoms;
- explicit repeated emission;
- word-visible patch operations;
- directional representation inheritance and fences;
- endpoint-owned status and effect;
- private auth language;
- finite timeout;
- explicit rate and pagination capabilities.

The most important remaining DSL problems are not broad conceptual flaws. They are projection inconsistencies and untested cognition.

# 9.2 `set`/`omit`/`reset`

Removing `with_*` is a conservative release choice in the absence of human evidence. The exact surface is semantically regular:

```rust
set_x(Option<T>)
omit_x()
reset_x() // initialized only
```

This preserves nested optional meaning and configuration composition.

However, the namespace reservation bug must be fixed, and Gate 3 must test whether callers repeatedly struggle with wrapping present values in `Some(...)`.

# 9.3 Human study protocol needs correction

The study packet is thoughtful: tasks cover nested optionality, cardinality, patch operations, representation provenance, fences, semantic diff, pagination completeness, compiler errors, streams, and authority.

The minimum experimental design is flawed for the `with_x` comparison:

- minimum participants: 3;
- two variants: exact surface and exact+`with_x`;
- tasks 1–3 are the only comparative tasks;
- participants switch variants only after task 3.

At three participants, one variant receives one participant and the other receives two. A median 15% difference is not a meaningful comparative result, and individual Rust fluency can dominate the outcome.

Recommended redesign:

## Option A — final-surface comprehension only

Since the conservative fallback already removed `with_x`, use all three participants on the exact final surface. Gate 3 tests whether v2.1 is understandable, not whether `with_x` is globally optimal. Keep `with_x` deferred.

This is the smallest release gate.

## Option B — paired comparative study

If deciding `with_x` now is necessary:

- recruit at least 4–6 participants;
- give every participant equivalent paired tasks under both variants;
- counterbalance order;
- use different value names/scenarios to reduce memorization;
- compare within-participant correctness, time, method search, misconceptions, and confidence;
- perform final-surface tasks after a washout/counterbalanced sequence.

Do not claim statistical generality from such a small study. Use it as formative falsification: repeated high-confidence misunderstanding is a blocker; absence of observed misunderstanding is limited support.

# 9.4 Independent Rustdoc/diff review

The packet asks one non-primary reviewer to identify representation provenance, nested overrides, fences, and semantic-diff consequences. This is exactly the right review for inheritance penetrability.

One of the DX participants could also satisfy the reviewer role only if:

- they meet the reviewer background requirement;
- the Rustdoc/diff review is performed before training on the DSL tasks, or the order is counterbalanced;
- scoring is independent from the primary implementer;
- prior exposure is recorded.

Otherwise recruit a fourth person. Reusing a participant after twelve tasks would no longer test first-contact discoverability.

# 9.5 Representation inheritance

Mechanical evidence appears strong enough to keep the selected design:

- client default;
- scope override;
- directional independence;
- `explicit` fence;
- provenance;
- semantic diff;
- Riot usage.

The remaining question is cognitive, not architectural. Do not remove inheritance before the human review unless a direct contradiction appears.

# 9.6 Documentation examples

Increasing compiled source anchors from one to 29 is substantial progress. The split between executable and explicitly conceptual fences is healthy.

The remaining requirement is enforcement parity:

- checker must run in CI;
- active specs must match parser behavior;
- exact final HEAD must be rerun;
- human review must verify generated Rustdoc, not only Markdown examples.

---

# 10. Streams and performance evidence

# 10.1 Public model

The public distinction is correct:

```text
ByteStream
  lazy byte chunks; boundaries not semantic

ItemStream<T>
  lazy decoded semantic items

RequestStream<T>
  caller-supplied lazy request lifecycle
```

Private boxed erasure protects the public API from decoder/driver types and keeps the implementation changeable.

# 10.2 Current evidence

The committed report records:

- one construction allocation for boxed stream;
- zero polling allocations;
- near-parity synthetic throughput ratios;
- static, enum, and boxed code-size/compile comparisons;
- lifecycle tests for acquisition, polling, error, drop, and terminal evidence.

This is enough to select boxed erasure as the private implementation for the prototype and likely for the first release.

# 10.3 Evidence boundary

It is not proof of universal stream performance because:

- one Windows/x64 host was used;
- in-memory synthetic drivers do not measure network/TLS/decompression/decoder cost distributions;
- allocator/platform effects differ;
- real backpressure and executor scheduling may dominate.

Because the carrier types are public but the erasure is private, this residual uncertainty need not block release if:

- precommitted budgets passed;
- Linux/macOS CI at least builds/tests the lifecycle;
- no obvious allocation-per-item path exists;
- implementation remains replaceable without public breakage.

Gate 8 can remain “Passed for private implementation selection; portability observation ongoing.”

---

# 11. Compile scale, docs scale, and IDE evidence

The 1,000-endpoint fixture demonstrates roughly linear expansion rather than a sudden algorithmic blow-up, but the absolute scale is significant:

```text
537,495 expanded lines
20,019 generated methods
~6.4 MB debug binary
~1.28 MB static descriptions
~3.1 GB rust-analyzer reported peak phase memory
```

Candidate all-feature build and peak memory reportedly increased by low single-digit percentages over baseline, while cold check cost increased more.

Interpretation:

- no evidence of an immediate superlinear catastrophe;
- static descriptions and method count are real developer-system costs;
- one 1,000-endpoint monolith is an adversarial fixture, not necessarily normal use;
- batch `analysis-stats` is not evidence of interactive completion/hover latency;
- actual hosted CI memory is important because docs.rs and GitHub runners are constrained environments.

Gate 10 should require an actual Linux CI run. If docs.rs publication is intended, test package documentation in a constrained container or docs.rs metadata configuration, especially because the recorded Windows build peak exceeded 7 GB and public documentation can instantiate large feature matrices.

Do not redesign the semantic model solely from expanded-line count. First identify whether shared generated helpers or descriptor deduplication can reduce tokens without changing public types.

---

# 12. CI and portability audit

The workflow defines:

- check/lint;
- tests;
- documentation;
- policy/security;
- platform smoke on Ubuntu, Windows, and macOS;
- analyzer jobs.

This is good repository design.

But actionlint only validates YAML. Because the release branch was not pushed, those jobs did not actually run on this candidate. The agent’s Windows machine evidence does not substitute for:

- Linux filesystem/path/case behavior;
- macOS platform behavior;
- shell/PowerShell availability;
- hosted-runner memory;
- actual Cargo cache/fresh environment;
- package/docs behavior outside the developer machine.

Before stable merge, obtain one of:

1. push a temporary review branch and run GitHub Actions, if/when remote push is authorized; or
2. run equivalent clean Linux and macOS environments locally/through CI infrastructure and retain logs.

The current instruction prohibited remote push, so the agent correctly did not do it. Closing this gate requires new authorization or equivalent infrastructure; it cannot be inferred.

---

# 13. Version and SemVer decision

The local baseline is version `2.0.0` but had `publish = false`. This creates two separate questions.

## 13.1 Git/path consumers

If anyone consumes `main` or v2.0 by Git/path, v2.1 contains broad source-breaking changes:

- public trait changes;
- method removals;
- DSL grammar changes;
- type and terminal changes.

Under ordinary SemVer, these are major-version changes.

## 13.2 First registry publication

If the eventual package namespace has never been published, the first registry release may use `2.1.0`, but that number communicates a history that registry users cannot see. It is permitted, but should be deliberate and documented.

## Recommendation

Decide and record:

- whether v2.0 had external consumers;
- whether this is the first public registry release;
- whether compatibility is intentionally broken;
- whether package namespace changes reset practical ecosystem history;
- what “2.1” means.

A conservative option for a first public, highly experimental DSL library is a pre-1.0 package version. But renumbering is a product decision, not required if the owner deliberately wants 2.1. The important point is to stop treating version as an implementation detail.

---

# 14. Reassessed release gates

| Gate | Agent status | Audit status | Required next action |
|---|---|---|---|
| **0 — package identity/legal authority** | Not present | **Blocked** | Select license; select/own package names; complete metadata; restore `publish=false` until closed. |
| 1 — representation pair/driver | Passed | **Passed mechanically** | Re-run exact final candidate and actual CI. |
| 2 — representation inheritance | Passed | **Passed mechanically; human pending under Gate 3** | Keep model; complete human provenance tasks. |
| 3 — DSL/generated DX | Blocked | **Blocked; protocol needs adjustment** | Run final-surface study with 3 humans, or paired variant study with more participants. |
| 4 — atoms/query/header | Passed | **Conditional** | Release `with_*` namespace; fix stale diagnostic; rerun UI/property suite. |
| 5 — runtime trace | Passed | **Passed mechanically, exact-HEAD rerun required** | Keep observer truth; optional panic diagnostic improvement. |
| 6 — pagination | Passed | **Passed mechanically** | Document controller metadata sensitivity; exact-HEAD/CI rerun. |
| 7 — rate/server delay | Passed | **Conditional** | Replace ambient wall-clock read or downgrade injected-time claim; clarify whole-plan accounting. |
| 8 — streams | Passed | **Passed for private choice with portability limitation** | Run platform build/lifecycle; keep performance monitoring. |
| 9 — information-flow security | Passed | **Passed mechanically with one policy question** | Decide least-authority treatment of `observed_url`; retain unknown-header tests. |
| 10 — scale/docs/hygiene | Blocked | **Blocked** | Human reviewer, CI docs checker, branch-diff cleanup, exact-HEAD run, actual platform CI. |

# 14.1 Overall state by evidence dimension

| Dimension | Current level | Comment |
|---|---:|---|
| Semantic architecture | 93/100 | Coherent owners and distinctions; no broad redesign needed. |
| Mechanical implementation | 88/100 | Strong code/test body, but source-of-truth contradictions remain. |
| Runtime behavior evidence | 90/100 reported | Good deterministic suites; not independently rerun here. |
| Security/information flow | 90/100 | Major response-header repair landed; URL/controller authority should be consciously accepted or narrowed. |
| DSL cognitive evidence | 0/100 direct | No unfamiliar human has completed tasks. |
| Rustdoc/diff human evidence | 0/100 direct | Review packet ready but empty. |
| Stream implementation evidence | 84/100 | Good private-design benchmark; one-host synthetic limitation. |
| Large-contract/IDE evidence | 78/100 | Useful scale measurements; interactive and hosted-CI evidence missing. |
| Documentation truth | 78/100 | Much improved, but optional secret and status contradictions remain. |
| Git/release discipline | 94/100 | Excellent preservation and refusal to merge prematurely. |
| Packaging mechanics | 75/100 | Extracted packages reportedly work, but names/metadata/publication path are unresolved. |
| Legal/registry readiness | 15/100 | No license and façade package name already occupied. |
| Merge readiness under current policy | 60/100 | Strong branch, but mandatory P0/human/legal gates remain. |
| Stable public-release readiness | 40/100 | Package identity and human evidence are decisive blockers. |

These are decision aids, not calibrated measurements.

---

# 15. Exact remediation sequence

# Phase A — preserve and establish authority

1. Keep `backup/v2.1-prototype-unreviewed` and `release/v2.1-readiness` unchanged as historical anchors.
2. Create `fix/v2.1-release-truth` from `adad5326`, or continue on the release branch if the team prefers fewer branches.
3. Add Gate 0 to release status and claim ledger.
4. Restore `publish = false` to all public packages until Gate 0 closes.
5. Mark root implementation prompt/remediation plan as historical or move them out of active product docs.
6. Remove ignored `concord_v2_1/` from final distributable workspace.

# Phase B — resolve package/legal identity

7. Owner chooses license.
8. Owner checks and secures final crates.io package namespace.
9. Rename package family if needed; preserve `concord` Rust import through `[lib] name` or dependency alias.
10. Add full package metadata and release ordering.
11. Decide version/SemVer policy and lifecycle name (`beta` versus `rc`).

# Phase C — make one language true

12. Defer/remove optional static secrets from active v2.1, or fully implement them by explicit owner choice.
13. Remove all stale `with_*` reservations and active spec references.
14. Fix `host` → `origin` diagnostic.
15. Correct C0 baseline version.
16. Reconcile README, FINAL, ADRs, gate status, and version vocabulary.
17. Update/supersede stale implementation plan and repository map.
18. Run retired-language/source-of-truth scans.

# Phase D — close runtime/release tooling gaps

19. Introduce private wall-clock injection/capture for `Retry-After`, or correct the Gate 7 claim.
20. Decide whether `observed_url` remains in observer and limiter APIs before public freeze.
21. Document whole-plan limiter accounting.
22. Add bounded observer-panic diagnostic if selected.
23. Move docs checks into cross-platform `xtask`; run them in CI.
24. Add merge-base `git diff --check` to the release command.
25. Fix all branch-diff whitespace errors.

# Phase E — machine evidence on exact candidate

26. Run complete clean suite on exact new HEAD.
27. Run package extraction/downstream validation using final package names.
28. Run local registry or staged publication rehearsal.
29. Run actual Ubuntu, Windows, and macOS CI/equivalent.
30. Record exact commit, toolchain, commands, logs, and limitations.

# Phase F — human evidence

31. Revise the Gate 3 protocol:
    - use three humans for final-surface comprehension; or
    - use at least four to six for paired `with_x` comparison.
32. Recruit unfamiliar relevant Rust users.
33. Run tasks without coaching and record anonymized evidence.
34. Run independent Rustdoc/diff review before reviewer is trained by the full task packet.
35. Apply concrete findings.
36. If public syntax/API changes, rerun all impacted mechanical gates and package evidence.

# Phase G — finalization and merge

37. Set final stable version only after every mandatory gate passes.
38. Remove `publish = false` only in the explicit release-enablement commit.
39. Run exact final verification one last time.
40. Merge locally:

```bash
git switch main
git merge --no-ff release/v2.1-readiness
```

41. Rerun complete verification on the merge commit.
42. Record merge SHA and final gate table.
43. Do not tag, push, or publish without a separate explicit authorization.

---

# 16. Merge alternatives

# Alternative A — keep branch until stable-ready

**Recommended.**

Benefits:

- honors existing project policy;
- keeps `main` as the last accepted baseline;
- prevents unresolved package identity from becoming authoritative;
- human findings can still change public surface cheaply;
- final merge has one clear meaning.

Costs:

- large branch remains outstanding longer;
- future parallel work must target/merge from release branch;
- review may be harder if `main` diverges.

Use when license/package/human evidence can be obtained reasonably soon.

# Alternative B — merge as beta integration

Possible only through an explicit policy change.

Required conditions before such a merge:

- all internal P0 contradictions fixed;
- Gate 0 publishing disabled, not falsely passed;
- version changed to beta/preview;
- `publish = false` restored;
- exact machine suite and platform CI pass;
- docs clearly say human gates remain open;
- no stable tag/publication.

Benefits:

- reduces long-lived branch divergence;
- makes the v2.1 implementation the active development base;
- human testing can use `main`.

Costs:

- changes meaning of `main` and current release policy;
- downstream Git users may treat it as authoritative;
- can create social pressure to waive remaining gates;
- rollback becomes more complex.

Use only if `main` is explicitly a development branch and the owner accepts that governance change.

# Alternative C — merge current RC as-is

**Reject.**

It would make contradictory docs/parser behavior, an impossible package name, false evidence rows, and open human gates authoritative. The fact that most machine tests pass does not repair those boundary failures.

---

# 17. Final assessment by subsystem

| Subsystem | Grade | Final comment |
|---|---:|---|
| Product thesis/boundary | 95 | Clear typed-contract compiler, not a generic HTTP escape. |
| DSL tree/ownership | 94 | Strong and local; source-of-truth cleanup remains. |
| Values/presence/defaults | 95 | Excellent semantics; human ergonomics pending. |
| Atoms/cardinality | 95 | Correct owner and explicit sequence meaning. |
| Query/header patch language | 91 | Good word-visible algebra; compile/user evidence mostly sound. |
| Representation model | 92 | Coherent generic pair and inherited owner model. |
| Generated direct/unbound API | 91 | Strong equivalence and capability shaping. |
| Core executor/deadline | 92 | Very good identity/order/repetition model; wall-clock claim needs correction. |
| Body/replay | 92 | Sound separation of page/attempt replay. |
| Status/effect/retry | 93 | One of the design’s strongest areas. |
| Authentication | 94 | Deep private-state model; optional-secret surface contradiction is outside core. |
| Rate limiting | 90 | Strong whole-plan model; extension accounting language needs precision. |
| Pagination | 94 | Explicit completeness, narrow controller, aggregate bounds. |
| Streams | 88 | Good public carriers/private erasure; evidence limited but sufficient for provisional selection. |
| Information-flow security | 91 | Response-header authority substantially repaired; URL/controller residuals explicit. |
| Observation | 87 | Truthful and narrow; silent panic and full observed URL deserve review. |
| Plans/descriptions/diff | 89 | Mechanically useful; final human penetrability unproven. |
| Runtime endpoints | 88 | Appropriate lower-confidence bounded escape. |
| Extras | 88 | Good semantic implementations; platform evidence remains. |
| Macro/semantic IR | 88 | Canonical direction good; leftover namespace/grammar drift shows convergence incomplete. |
| Tests/evidence infrastructure | 93 | Exceptional breadth and gate discipline. |
| Documentation mechanics | 86 | Strong anchors and checks, but not in CI and active contradiction remains. |
| Documentation cognitive quality | Unknown | No qualifying human evidence. |
| Git/process discipline | 94 | Correct non-merge and preservation choices. |
| Package/release engineering | 45 | Good local packaging mechanics, but namespace/legal/metadata strategy incomplete. |
| Stable release readiness | 40 | Decisive external and internal gates remain. |

---

# 18. Final stop/commit decision

## Objective reached

A coherent v2.1 architecture and substantial implementation prototype exist. The major semantic and runtime design work should be preserved.

## Decision

```text
DO NOT MERGE adad5326 AS-IS.
CONTINUE THE RELEASE BRANCH.
DO NOT REDESIGN v2.1 GENERALLY.
ADD GATE 0 AND FIX THE CONCENTRATED TRUTH/PACKAGE DEFECTS.
MERGE ONLY AFTER HUMAN, LEGAL, PACKAGE, CI, AND EXACT-HEAD GATES PASS.
```

## Claims supported strongly enough to retain

- exact Rust payload types independent from Concord presence;
- explicit `each` cardinality;
- canonical atom role;
- generic representation pairs with core-sealed acquisition;
- endpoint status/effect ownership;
- one executor/deadline;
- separate policy retry, auth recovery, and physical recovery;
- whole-plan rate admission with future-only feedback;
- explicit pagination completeness and controller ownership;
- read-only response-header-free general observation;
- distinct public stream carriers.

## Claims still provisional

- unfamiliar-user comprehension;
- final optional-control ergonomics;
- inherited representation discoverability;
- Rustdoc and semantic-diff penetrability;
- interactive IDE experience at large scale;
- stream performance portability;
- need and comprehension of optional static credentials;
- broad value of full observed URLs in extension contexts.

## Newly disconfirmed claims

- baseline package version was `1.0.0`;
- `git diff --check` established branch-diff cleanliness;
- optional static secret is implemented as documented;
- package publication is blocked only by the license;
- the façade can be published under the current `concord` package name;
- exact final HEAD has already completed the full aggregate verification;
- all docs source-anchor/retired-vocabulary enforcement runs in CI;
- production server-delay handling is fully based on an injected wall clock.

## Revisit trigger

Reopen architectural choices only if:

- human tasks falsify comprehension;
- final package namespace forces a harmful public import shape that aliases cannot solve;
- actual platform CI exposes a runtime/feature contract defect;
- representation/stream implementation fails final resource budgets;
- a real API proves a deferred feature necessary.

Otherwise, the next work is convergence and evidence—not another design expansion.
