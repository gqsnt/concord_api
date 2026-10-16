# Decision record — Concord v3 implementation target

- **Status:** Selected implementation for the unpublished `3.0.0-beta.1` beta;
  beta integration and stable publication are owner decisions recorded in
  [`release_gate_status.md`](release_gate_status.md)
- **Decision owner:** Concord project owner, through the selected v3
  implementation specification (Edition 1, 25 September 2026)
- **Date:** 2026-10-16 (current V3-only scope; original implementation decisions September 2026)

## Decision

Implement Concord V3 as the only supported product: one canonical grammar,
one managed runtime, current V3 tooling and no older-version aliases or
conversion path. The owner's 16 October instruction supersedes the original
migration and older-binary continuity requirements; [ADR 0027](adrs/0027-v3-only-product.md)
records the scope change. ADRs 0014–0026 continue to define the unrelated V3
semantics.

## Decisive properties

1. A bare name binds only the current owner; ancestors are qualified
   explicitly, so parent edits cannot capture references (ADR 0022).
2. Direct placement is a normal introduction; explicit mappings can coexist
   with it; introduction, replacement and removal have structural
   preconditions (ADR 0015).
3. Query/header programs are evaluated symbolically before encoding;
   suppressed encoders never run; key sensitivity is sticky (ADR 0015).
4. Payload assignment, exact slot replacement, conditional assignment, omission
   and retained reset are different operations with different names; ordinary
   conversions are closed (ADR 0014).
5. Caller controls are a public unsealed trait while execution authority
   stays in core bindings; a timeout preference never removes a cap (ADR 0026).
6. Representations are selected by direction and trait obligation;
   negotiation is static; one prepared body owns the exact media type
   (ADRs 0016, 0018).
7. Expected absence is explicit, head-only and capability-restricted, with a
   buffered alternative (ADR 0019).
8. Credential storage follows strict latest-started explicit intent; the
   installed version is a separate counter; local installation belongs to the
   storage result and precedes its one final observation; optional static
   secrets are not part of v3 (ADRs 0020, 0021).
9. Rate numbers, identities and key schemas are separate; labels and encoding
   IDs are part of the schema; admission is atomic; shared adapters check
   schemas before state use (ADR 0023).
10. One-page await is distinct from lazy, prefix and complete traversal; one
    aggregate deadline covers a traversal; a started page fetch that is dropped
    terminates it (ADR 0017).
11. Stream acquisition is distinct from consumption, under finite configurable
    leases; an expired pending-read idle deadline or lease wins over data or an
    end of body observed afterwards (ADR 0011 as extended by spec §15).
12. Runtime endpoints enter the same owner projection and symbolic program and
    cannot create private authority (ADR 0024).
13. Every path enters one executor whose journal owns dispatch knowledge; no
    error path fabricates "not dispatched" (spec §13).
14. Runtime configuration uses primitive counts validated at build
    (ADR 0025).
15. Effective contracts are exported as the versioned `concord.contract/3`
    structural format with axis-classified diffs (spec §18).
16. Only the V3 language, public types, runtime and inspection format are
    supported. An unsupported spelling is rejected, and current schema
    registration cannot certify state from older binaries (ADR 0027).

## Costs accepted

- V3-only source and API support; applications target the canonical V3 interface;
- more generated methods per value (`replace_x`, `set_x_if_some`);
- stricter structural diagnostics for patch programs and names;
- explicit rate labels, window sets and encoding IDs; atomic admission for
  custom limiters;
- body-bearing generated requests are not `Clone`;
- strict page-fetch cancellation and finite stream leases;
- a slow final observer can delay delivery of a storage result beyond its
  nominal deadline (it cannot change it);

## Historical implementation evidence

The records below describe their original source revisions. Migration-tool
repairs are historical after ADR 0027; they are not current product gates.

R7 source qualification uses native Windows and Linux/WSL, pin `1.97.1` and
MSRV `1.86.0`, with full maintained implementation gates passing on candidate
`e19c7bf`. [R7 validation](reviews/r7_validation.md) owns exact source/command
evidence and limitations; the earlier Linux 1.95 evidence remains historical.

This record does not claim:

- unfamiliar-human comprehension or independent Rustdoc/semantic-diff review;
- full independently re-established external Edition 1 conformance;
- license authority, package control or publication readiness.

Implementation gaps against the specification found while writing these
records (2026-09-26), all closed later the same day in this tree:

- `RuntimeConfigBuilder::aggregate_max_sends` (spec §20.4) was missing; it is
  implemented (ADR 0025);
- the `just verify-implementation` recipe named in spec §28.2 was missing; it is
  in the `justfile` and excludes the external license/registry/tag/human gates
  (`deny`, `workflow`, `package`, `release-audit`), which stay in the release
  report;
- generated endpoints validated their static `ACCEPT` only at head preparation;
  the macro now emits the per-endpoint const assertion (spec §9.5, ADR 0016);
- the façade and macro crate-level rustdoc described the v2.1 value-control
  wording; it was corrected (ledger claim C62);
- NDJSON had no request-side records (spec §9.5 "typed framed request/response
  records"); `NdJson` now implements `RequestRepresentation<Vec<T>>` and
  `PageRequestRepresentation<Vec<T>>`.

A later audit of all 87 normative rules (2026-09-26) found and closed in this
tree: provider acquisitions without their own operation identity and journal
(EXEC-01); role-trait errors without coded, placement-spanned diagnostics and
coded macro messages without owner and correction (DIAG-01); incomplete
extension-trait contracts (EXT-01); "stable" wording for navigation-derived
endpoint IDs (IDENTITY-01); an unclassified root-default timing change in the
migration tool (MIGRATE-04, new `C3-MIGRATE-ROOT-DEFAULT-TIMING`); and missing
tests for several implemented clauses (CONTROL-02, RETRY-01/02, STREAM-01/02,
VALUE-04/05/06, PRODUCT-04, PAGE-01, ORIGIN-01, EXPLAIN-01/02/03, RATE-02,
LANG-02, LIMIT-01/02).

An independent external review of the return associated with commit
`fc3e4d1` (26 September 2026) retained the candidate but found three
source-level conformance defects, each repaired in this tree (with follow-up
repairs after post-fix review notes) and still awaiting the final recorded
validation on the final corrected digest (see
[`release_gate_status.md`](release_gate_status.md#external-review-of-the-fc3e4d1-return)):

- **F01** (V3-E21 storage case reopened; V3-E15 oracle extended): explicit
  storage emitted its final observation before local installation. Storage now
  installs in the executor's finalization step, inside the still-open journal;
  deadline and cancellation are decided before installation (around the
  material's `expires_at()` read); the storage result is fixed first and then
  observed once; spent material is released after that observation; a
  cancelled provider leader publishes `Reevaluate` before dropping retired
  material (ADR 0021).
- **F02** (V3-P10 reopened): ready data or an end of body could erase an
  already expired pending-read idle deadline. `StreamLease::check(now)` now
  judges the total lease, then an outstanding pending read's idle, at every
  poll and at every observed read completion, and a cooperative empty-chunk
  yield keeps the acquisition pending (ADR 0011).
- **F03** (V3-O04 reopened): `migrate-v3` could rename an unrelated type's
  setter. Caller rewrites now require proved receiver identity and dispatch
  (next section).

## MIGRATE-03 receiver identity (external review F03)

- **Status:** Selected (v3.0.0-beta.1); tests in tree, recorded in the r2
  validation run
- **Specification:** §25.1–§25.2 (MIGRATE-03), obligation V3-O04
- **Date:** 2026-09-26

**Context.** At `fc3e4d1` the caller visitor inferred receivers by name:
generated types were indexed by unqualified leaf name, a type path kept only
its last segment, field types were the union of every same-named field of any
struct, functions were one global name map, a client was matched by the
second-to-last path segment and UFCS by the leaf type name. An unrelated
`other::ItemRequest`, or `other.request` on an unrelated struct, offering both
`set_x` and `replace_x` was therefore renamed to a call that still compiled and
returned a different result. A syntax parser plus an optimistic name heuristic
is not proof enough for an automatic edit.

**Decision.** Every automatic caller rewrite — a `set_x` → `replace_x` rename
(raw `r#set_x` → `r#replace_x`), a `.paginate(PageLimit::..)` terminal with
its `.await`, annotation and import edits, a runtime `NonZero` argument's
`.get()`, and a move of a v2.1 crate-level `ClientBuilder` reference into the
generated support module — passes one gate: it is made only when the tool
proves that the call reaches the generated item. Everything unproved is left
unchanged and reported with the reason. This is not a Rust type checker; it is
a bounded name-resolution model, default-deny wherever the model cannot see.

**Bounded name-resolution model** (`tools/xtask/src/migration/resolve.rs`,
`CrateModel`; per crate, read-only):

- the module tree from the crate roots (`lib`, `main`, `bin`, `tests`,
  `examples`, `benches`, `build.rs`) following `mod` declarations, `#[path]`
  and inline modules; a file no root reaches keeps a layout-derived module
  path, and another target of the package reaches the library by its crate
  name (`migration/mod.rs` `Layout`);
- per module and per block: items (structs with field types, functions with
  return types, type aliases; enums, traits and constants as non-generated),
  `use` imports with renames, globs and visibility, `extern crate`, and
  generic parameters, with Rust's shadowing (local items over imports,
  explicit names over globs, block items over outer scopes, generic parameters
  over items) and exact `self::`/`super::`/`crate::` prefixes;
- each proved `api!` invocation places its v2.1 generated surface at its
  module (`Client`, `ClientBuilder`, the support module with the unbound
  root, scope modules, endpoint requests and intents); hidden `__concord*`
  items are unknown;
- fields are typed through their owning struct, calls through the function
  the path names, constructors (`Client::builder()`, `new()`, `unbound()`)
  only when `Client` resolves to a generated client, qualified and `<T>::`
  UFCS through their type path (trait-qualified UFCS is never proved);
- standard wrappers (`Option`, `Result`, `Box`, `Arc`, `Rc`) and references
  stay around the type: only an explicit `?`, `.unwrap()`, `.expect(..)` or
  `*` unwraps, and method chains advance only through methods of the v2.1
  generated surface with their result types (`callers.rs`);
- pattern-position macros hide every visible binding; a statement macro is
  binding-free only when its path resolves to the standard library, `log` or
  `tracing`;
- anything the model cannot see (a missing module file, an item macro other
  than `api!` and the standard ones, an unresolved import, a foreign glob in
  a block, a generic field's substitution, a user method's return type) makes
  a name unknown; a path is never resolved from its last segment alone. Glob
  lookups are memoized per query, so glob re-export graphs resolve in linear
  time.

**Default-deny gate** (`tools/xtask/src/migration/callers.rs`,
`rust_file.rs`):

- *Macros.* Only the expression arguments of standard-library expression
  macros whose path resolves to `std`/`core`/`alloc` (`assert*`, `vec!`,
  `format!`, `print*`, `write*`, `panic!`, `dbg!`, ...) are rewritten;
  token-only standard macros are left alone silently; candidates inside any
  other macro, or inside an item carrying an attribute macro of unknown
  expansion, are reported (`#[tokio::test]`, `#[tokio::main]` resolved to the
  `tokio` crate and the unreplaced prelude `#[test]`/`#[bench]`/
  `#[global_allocator]` are known).
- *cfg.* A `let`, a parameter or `self`, a `use`, an item (including a module
  declaration or module file, a function, a type, a constant and an `api!`
  invocation) or a field gated by `#[cfg]`/`#[cfg_attr]` whose predicate is
  not trivially true makes every name it declares, imports or shadows
  uncertain; only a predicate built from `all`, `any` and `not` alone counts
  as constant, and features are never evaluated. A gated `impl` is treated as
  present, which can only block a proof.
- *Dispatch completeness.* A call exact at the first method-probe step (an
  inherent generated method on the receiver's own type) stays provable in any
  crate. A call that reaches the generated method only after an auto-deref or
  auto-ref step is proved only when the crate model is complete (no opaque
  item macro or `include!`, no unknown attribute macro, no derive other than
  the standard ones and `serde`'s, no missing or unreached module), no trait
  of another crate other than the standard library, Concord and the known
  `serde` items may be in scope, and no trait implemented in the crate
  provides a method of that name at an earlier step.
- *Cross-crate.* A receiver that resolves to a type of another crate is
  reported, also when that crate's `api!` is part of the same run (the
  diagnostic names the declaring crate).
- *`api!` itself.* An invocation whose path ends in `api` is migrated only when
  it is proved, within its file, to be Concord's `api!`.
- *Declared types, visibility, traits (round 4).* A declared type always wins
  over an initializer: a `let` annotation that does not resolve exactly (a
  `dyn`, `impl`, generic or unexpandable alias) makes the binding unknown; an
  `async fn` call is an opaque future; a field or item is used only when it is
  provably visible from the call site (`pub`, `pub(crate)`, `pub(super)`,
  `pub(in path)`, private); an `impl`'s trait path is resolved through imports
  and renames; `#[cfg_attr(.., macro_use)]` counts as `#[macro_use]`; inline
  `#[path]` follows rustc; a package library's incompleteness applies to its
  tests, examples, binaries and benches; a bare `api!` is proved only by an
  unconditional `#[macro_use] extern crate concord` without a crate-local
  `macro_rules! api`; a local `concord` shadows the crate; edition-2015 paths
  and `[lib]`/target `path` entries are honored.
- *Shadowing, mounts, manifests, values (round 5).* An item, import or glob of
  any block between a use site and an outer `let` binding shadows the binding
  (rustc rule) or makes it unknown; a module file compiled more than once (by
  several crate roots or several `#[path]` mounts) is never rewritten; the
  manifest is parsed as TOML (vendored `toml`) and an edition that cannot be
  determined resolves paths both ways, unknown unless both agree; a generated
  optional setter used as a function value (`Type::set_x` not in call
  position) is reported.
- *Robustness and determinism.* A file nesting delimiters or generic brackets
  deeper than 256 levels is reported and left unchanged; migration runs on a
  256 MiB stack; receiver inference is memoized (linear in chain length);
  imports and files are walked in sorted order, so reasons are byte-identical
  between runs; name resolution is bounded at 48 steps (`MAX_DEPTH`).

**Diagnostics** (severities from `tools/xtask/src/migration/report.rs`, matching
the code table of [the historical diagnostic table](https://github.com/gqsnt/concord_api/blob/ae6006184f3dc903fdfe66c572098ac810e2c6c5/docs/migration_v3.md#diagnostic-codes)):

| Code | Severity | Status after the repair |
|---|---|---|
| `C3-MIGRATE-SETTER-UNRESOLVED` | error | Existing; now also for a receiver resolved to another type, a wrapper, another crate's type, unproved dispatch, a macro or `#[cfg]` site, and UFCS on unknown types (previously skipped silently) |
| `C3-MIGRATE-RUNTIME-NONZERO-UNRESOLVED` | error | Existing; now also when the `configure_runtime` receiver is not a proved generated builder |
| `C3-MIGRATE-PAGINATE-UNRESOLVED` | error | New: a `.paginate(..)` not proved to be the generated traversal of Concord's `PageLimit`; since R5 also type-qualified `paginate` calls and function values |
| `C3-MIGRATE-GENERATED-MEMBER-REMOVED` | error | New (R5): a call of a member v2.1 generated and v3 removes (bound-request `debug`, builder `<name>_provider`); an application trait method of that name could be selected after migration; never rewritten |
| `C3-MIGRATE-BUILDER-PATH` | note (automated) | New: a proved `ClientBuilder` reference now names the support module |
| `C3-MIGRATE-BUILDER-PATH-UNRESOLVED` | error | New: a possible `ClientBuilder` reference that could not be moved |
| `C3-MIGRATE-API-MACRO-UNRESOLVED` | error | New: an `api`-named macro not proved to be Concord's `api!` |
| `C3-MIGRATE-NESTING-LIMIT` | error | New: a file nesting delimiters deeper than 256 levels |
| `C3-MIGRATE-GENERATED-NAME-SHADOW` | error | New (R3): a call of a name only v3 generates (`replace_<v>`, `set_<v>_if_some`, `bind`, the bound request's `description` and the custom-provider builder setter since R4, …) on a receiver that is or may be generated; the application method it named would be shadowed by v3's inherent method; never rewritten |
| `C3-MIGRATE-GENERATED-NAME-COLLISION` | error | New (R3): v2.1 values whose v3 generated names collide on one surface; reported at the declaration |
| `C3-MIGRATE-DEPENDENCY-IDENTITY` | error | New (R3): the package manifest does not establish that the Concord crate names denote the genuine Concord packages; the file is left unchanged |

**Limits of the model** (also in
[the historical model limits](https://github.com/gqsnt/concord_api/blob/ae6006184f3dc903fdfe66c572098ac810e2c6c5/docs/migration_v3.md#limits-of-the-model)):
textual `macro_rules!` scope is not modeled (a crate-local macro name counts
as defined everywhere in its crate); only the listed external items are known
(the standard library, Concord, the `log`/`tracing` expression macros,
`serde`'s derives and three traits, `tokio`'s two attribute macros), so any
other crate's item blocks the proofs even when harmless; a module file
compiled more than once is modeled at its first mount and never rewritten;
a crate outside the run is invisible, so a call into it that the tool cannot
type is reported only if the run declares a same-named optional value; files
deeper than 256 nesting levels are not examined; resolution chains longer than
48 steps are unknown; doc-comment examples (doctests) are not scanned; a
dependency's Concord identity is checked from the package manifest (R3:
`package =` renames, `path` targets, `[workspace.dependencies]`; `[patch]`/
`[replace]` redirection and foreign `extern crate` aliases are refused), registry and
`git` dependencies by package name only, and a file whose manifest declares no
Concord dependency (or a standalone file) still assumes the Concord names are
genuine; a proved rewrite inside `dbg!`/`assert!` changes the text those macros
print.

**Candidate discovery (R3, review B01).** Setter candidates are found by the
v2.1 declared value and generated method role, never by name shape: a value
named `x_if_some` has the exact v2.1 setter `set_x_if_some`, which migrates to
`replace_x_if_some` when proved (v2.1 generates no `_if_some` methods, so a
conditional-shaped call of an undeclared value is not a candidate). A call of
a name only v3 generates on a receiver that is or may be generated is reported
(`C3-MIGRATE-GENERATED-NAME-SHADOW`), because v3's inherent method would shadow
the application method the v2.1 code called. Since R4 the v3-only inventory is
guarded against generator drift: `migration::tests::surface_parity` compares it
with the public methods the real generator emits for a representative fixture
(`tools/xtask/fixtures/generated_surface/`, extracted by the test-only
`concord-macros` module `generated_surface`) and a frozen v2.1 member list, so a
newly generated method fails the test until the inventory is updated. Since R5
the comparison is two-way: every removed member also needs a disposition
(manual diagnostic, automatic rename, dedicated rule such as `paginate`, or a
reasoned no-risk classification), and a signature change needs an entry.
Since R6 a removed member referenced inside a macro the tool does not expand is
recognized on qualified paths without a following `(..)` (function values such as
`pass!(Type::debug)` and grouped callees `pass!((Type::debug)(..))`), reported
with the removed-member or paginate diagnostic and never rewritten; a nested
macro name (`log::debug!`) and trait-qualified paths are not reported.

**Review history.** The repair went through five rounds; rounds 1–4 were each
followed by an independent reviewer who did not write them (four reviews), and
round 5 was verified against the fourth review's compiled controls only: round 1 (bounded model), round 2
(wrappers, chains, pattern macros, globs, builder paths), round 3
(default-deny gate), round 4 (third-review classes D1–D8 plus a
type-assignment audit), round 5 (fourth-review classes N1–N4). The fourth
review found every earlier probe closed; round 5's fixes were verified by the
fourth review's own compiled controls re-run against the final tool
(`evidence/r2/F03/after/round5_review4_compiled_controls_rerun`); no fifth
independent review was run.

**Consequences.** No automatic edit of an unrelated receiver; the cost is that
some genuine calls are reported for a manual rename. The existing nested-Option,
Riot/Catalog recording and crate-wide Riot caller tests
(`riot_usage_setters_resolve_per_owner_across_the_crate`) are kept beside the
new identity tests (claim C54).

**Evidence** (tests in tree; recorded in the r2 validation run): see claim C70
in the [claim ledger](claim_evidence_ledger.md).

## Revisit triggers

- measured caller tasks show a v3 control or placement spelling is routinely
  misread;
- a real API needs speculative `when_present` without a fallback;
- a provider contract needs resumable page cancellation or aggregate body reuse;
- a custom limiter cannot implement atomic admission for a legitimate backend;
- the owner selects publication, which reopens package identity and freeze
  evidence.

## History

The v2.1 decision record (the "independent hybrid v2.1 candidate" implemented
from v2.0.0) is superseded by this record. Its full text is retrievable with
`git show baseline-v2.1:dev_doc/decision_record.md`. Its decisive properties —
Rust spelling cannot change presence or cardinality; separate owners for
presence, defaults, placement, representation, status and effect; the result
arrow is the exact awaited type; distinct replay authorities; controller-owned
pagination; read-only observation; one core executor; visible effective
inheritance — are all retained in v3.
