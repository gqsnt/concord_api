# Track A — DSL designer (authoring): D04, D05, D06, D07, D14

## Source identity and session

- SNAP `/home/canarit/projects/concord_v4_refine_work/snapshots/authoring-8f7adc2`: `.concord-review.json`
  `original_commit` = `8f7adc2019f3ee31af087c69286d559bca1c173a` (tree `26e6a14b…`); snapshot HEAD
  `29eb333`, clean. **SNAP was not modified.** Prototype macro changes live in copies under EXP.
- EXP `/home/canarit/projects/concord_v4_refine_work/exp/authoring`.
- Model: Claude Opus 5.5 (`claude-opus-5-5`). Requested effort xhigh; effort is not observable
  from inside the session.
- Toolchain: EXP pins `rust-toolchain.toml` = 1.97.1 (same as SNAP). Every result quoted below was
  re-run on 1.97.1 by `EXP/rerun_all.sh` (logs `EXP/out/rerun/`). An earlier pass ran on the machine
  default (stable 1.99.0) before the pin was added; nothing quoted depends on that pass.
- Interrupted once by the host restart (~00:43); work resumed from intact files.
- No timing measurements were taken (concurrent builds).
- Writes: only EXP and this report (plus two short-lived temp files in the session scratchpad, since
  deleted). SNAP `git status` is clean. EXP build dirs: `target` 5.3G, `target-proto_keep` 2.2G,
  `target-proto_alt` 0.8G, `target-expand` 0.5G; all disposable.

## Recommendations at a glance

| ID | Disposition | Smallest implementation | Counterexample intentionally left unsupported |
|---|---|---|---|
| D04 | **CHANGE (small)**. Keep the control's two behaviours. An `override` whose source can be absent must say what happens then: `override "k" = v else inherit;`. Also: `remove` applies before the owner's own contributions, so `remove` + a direct placement works. Plus better diagnostics and caller docs | ~60 macro lines: parser clause, one error, removes processed first, `removed_by` in the error, a doc note on `set_`/`maybe_` | Per-call "inherit / send nothing / set" on one endpoint still needs `x?: Option<T>` + `override "k" = each x else inherit` |
| D05 | **KEEP** | Docs only: native recipe for interleaving; `each` iterates in collection order (HashSet is nondeterministic); `tags=a,b` needs a newtype | `a=1&b=2&a=3` stays native (~12 lines via `into_parts`) |
| D06 | **CHANGE**: one permanent spelling `-> T via R` / `body b: T via R`. **REJECT** a scoped representation default | Representation traits generic over the payload (`DecodeAs<T>`/`EncodeAs<T>` on marker types); the macro emits `T` directly in signatures; mechanical migration | Defaults (no per-endpoint `via`); `Bytes via Binary` is slightly redundant |
| D07 | **KEEP** `Display` + newtypes; **DEFER** the per-placement adapter; **REJECT** a wire-value trait. Fix the diagnostic span; document the hazards | Errors re-spanned to the value's type (`Span::call_site().located_at(ty)`); docs recipe | One input with several spellings in one endpoint (U21) needs several inputs or a 3-line inherent wrapper |
| D14 | **KEEP** the reject-by-default attribute policy. **CHANGE**: forward `#[cfg]` (whole subtree) and `#[deprecated]` (navigation method) on scopes and endpoints, and fix the dropped docs of required values | ~60 macro lines (prototyped, warning-free, no dangling items) | Mutually exclusive same-name `#[cfg]` endpoints; `#[allow]`/`#[cfg_attr]`; per-endpoint visibility |

## Defects found in the control (separate from design opinions)

| # | Defect | Reproduction | Prototype fix |
|---|---|---|---|
| DEF-1 | Docs on **required** values (client, scope, endpoint) are silently dropped from rustdoc, even under `#![deny(missing_docs)]`. Example: `examples/shapes` S2's "The API key…" doc never renders | `EXP/control/src/d14_docs.rs`; `cargo doc -p authoring-control`; the markers `REQUIRED-*-VALUE-DOC` appear in no page (`DEFAULTED…`/`OPTIONAL…` do) | Adds an `# Arguments` section to `new`, scope entries and endpoint entries (proto_keep; verified in rustdoc) |
| DEF-2 | Exact replacement cannot use the direct-placement form. `GET exact(query locale?: String) { query { remove "locale"; } }` fails with "already set by `Api`; use `override`, `append` or `remove`" — the author already wrote `remove`. Cause: direct placements are composed before block statements | probe `d04_p1_placement_exact` | `remove` statements apply to the inherited state first (proto_keep test `placement_after_remove_is_exact_replacement`) |
| DEF-3 | After an enclosing `remove`, `override`/`remove` say "no enclosing owner sets it", which is false. UI test `override_after_remove.stderr` pins the misleading text | probe `d04_p2` | "cannot override query key `locale`: `Api.legacy` removes it; introduce it again with `"locale" = ...`" |
| DEF-4 | Missing `Display` (placements, templates, path holes) and a missing/incorrect representation (`-> Item`, `body b: NewItem`) are reported on the **entire `api!` invocation**, 2–5 times. UI tests `option_has_no_wire_spelling`/`vec_needs_each` pin the whole-macro span | probes `d07_p1..3`, `d06_p1..2` | Generated calls re-spanned with `located_at` (errors now point at the value's type). `#[diagnostic::on_unimplemented]` on `Encode`/`Decode` ("`Item` is not a response representation … name `Json<Item>`…"). Output case: 2 of 3 errors now located |
| DEF-5 (minor) | `"k" = a; remove "k";` in one owner is accepted. `a` passes the liveness check ("used") but never reaches the wire | probe `d04_p4_same_owner_remove` | Rejected once `remove` applies to the inherited state |
| DEF-6 (minor) | The owner attribute filter accepts any `#[doc(...)]` list form. `#[doc(inline)]` produces rustc warnings spanning the macro. `pub(crate) GET` gives an unhelpful "expected `scope`, an HTTP method…" | probes `d14_a7`, `d14_a6` | Not prototyped. Restrict the filter to `doc = ".."`, `doc(hidden)`, `doc(alias)`; mention that visibility comes from the client |

Running the SNAP suite against proto_keep (`EXP/out/rerun/proto_keep_snap_suite.log`): every runtime
test passes. Six UI snapshots differ, and only in the intended diagnostics:
`duplicate_header_case`, `duplicate_inherited_key`, `override_after_remove`, `override_without_target`,
`option_has_no_wire_spelling`, `vec_needs_each`. Two fixture lines in `composition.rs` needed
`else inherit`; the compiler pointed at both. For an optional override with **no** target, the new
"say what absence means" error currently pre-empts the "no enclosing owner sets it" error, because
resolution errors return before composition. A real implementation should report the target error
first.

---

## D04 — Inherited optional override (U10; C08–C10)

**User problem.** U10: inherit a locale, optionally override it, remove it, reintroduce it. The
smallest counterexample is `override "locale" = page_locale;` with `page_locale?: String` absent:
does the request carry the inherited locale or none?

**Control behaviour (executable, `control/tests/d04_override.rs`, `d04_fresh_fixture.rs`, 9 tests
pass on the unmodified macro).** Root `query locale: String = "en-US"`,
`query("tag") each tags = [a,b]`, block `"v" = "1"`.

| Case | `override "k" = v` (control) | `remove "k"; "k" = v` (control exact) | Evidence |
|---|---|---|---|
| no live writer on the chain | compile error "no enclosing owner sets it" (catches typos) | `remove` errors the same way | probes `d04_p5`, UI `override_without_target` |
| several parent occurrences (`tag=a&tag=b`), `v` present | `tag=t`: one occurrence, at the slot's first position | `tag=t` | `several_parent_occurrences` |
| `v` absent | inherited kept: `locale=en-US` (or the scope's value) | no `locale` | `inherit_and_optional_override`, `exact_replacement_and_removal` |
| caller `maybe_v(None)` | same as absent: the caller **cannot** suppress the inherited value | no `locale` | B3 |
| present-empty `each` (`Some(vec![])`) | present: clears (`locale=en-US&v=1`) | clears | `present_empty_each_vs_absent` |
| absent `each` (`None`) | inherited kept | nothing | same |
| template with two optional holes, one absent | whole source absent: inherited kept | nothing | `partially_present_template_falls_back` |
| remove, then reintroduce in a descendant | — | reintroduced at the key's **original** slot position (`locale=nl&tag=…`, not at the end) | `remove_then_reintroduce`, B7/B12 |
| `override` after an enclosing `remove` | compile error (misleading text, DEF-3) | — | probe `d04_p2` |
| `override` after a reintroduction | legal again; falls back to the reintroduced value | — | `remove_then_reintroduce` |
| multiple optional ancestors (store, page) | innermost present wins; all absent gives the root default | exact at the endpoint discards the scope's value | `multiple_optional_ancestors` |
| header family (`accept-language`, loopback) | identical semantics; names case-insensitive | identical | `header_family_on_the_wire` |
| exact via direct placement (`query locale?` + `remove`) | — | **compile error (DEF-2)** | probe `d04_p1` |

**Alternatives built.**

- **Alt-X** (exact primitive + explicit fallback): plain `override` is exact (an absent source sends
  nothing); fallback is `override "k" = v else inherit;`. Prototyped in `EXP/proto_alt`
  (`proto_alt.patch`). On the translated fixture the wire is identical to the control
  (`altx_d04.rs::fresh_fixture_actual_results`). Executable hazard: an author who writes the ordinary
  "optionally override" as plain `override` silently drops the inherited default on the common path
  (`altx_forgotten_else_inherit_drops_the_default`: the `x-api-version` header disappears).
  Migration is also silent: every existing optional `override` (S2, docs, tests) compiles unchanged
  and changes its wire behaviour.
- **Alt-Y** (recommended; prototyped in `EXP/proto_keep`): control semantics unchanged, but an
  optional source must carry the clause. Error at the `override` token:
  `this override reads an optional value, so it can be absent; say what absence means: override "locale" = ... else inherit; keeps the inherited value, remove "locale"; "locale" = ...; sends nothing`
  (probe `d04_alty_missing_clause`). `else inherit` on an always-present source is an error
  ("never applies"). Exact replacement keeps a single spelling, `remove "k"; "k" = v;`, and with
  remove-first it also works with direct placements. All control truth-table tests pass on
  proto_keep with `else inherit` added (`proto_keep/tests_crate/tests/d04_*.rs`).

**Prediction questions (U10).** Fixture: `EXP/fresh/decl_control.rs` / `decl_altx.rs`; 13 calls
B1–B13 in `fresh/questions.md`; authoring tasks A1 (optional override with fallback) and A2 (exact).
Simulated fresh participants (two sonnet subagents, no tools, equal one-line docs per operation;
raw answers in `fresh/responses.md`; **an agent simulation, not a human study**):

- Control: A1 and A2 correct. B7 and B12 wrong: both assumed a reintroduced key moves to the end.
- Alt-X: A1 and A2 correct. B12 wrong (same reason).
- Both participants listed the same uncertainties: where a replaced or reintroduced key lands,
  whether a present-empty `each` counts as present, and whether reintroduction after `remove` is
  legal.
- The coordinator's longer fresh-use trial of the control: the participant read `override` as
  "always replace", needed `lang?: Option<String>` + `override … = each lang` (found only in
  SEMANTICS.md) for inherit/none/set, and never discovered `remove "k"; "k" = v`.

**What a fresh user is likely to get wrong under each design.**

- **Control:**
  - `override` with an absent value is read as "replace with nothing". This is the coordinator's
    trial; the keyword alone does not convey the fallback.
  - `maybe_x(None)` is read as "drop the inherited key". The generated docs only say "`None` leaves
    it absent".
  - The exact idiom is not discovered. The crate docs never mention `remove "k"; "k" = v`; only
    SEMANTICS.md does.
  - Direct placement + `remove` fails (DEF-2).
  - The reintroduced slot position.
- **Alt-X:** forgetting `else inherit` gives a silent drop on the common path; plus the slot
  position.
- **Alt-Y:** the slot position only. The other control traps either become a compile error that
  names both choices, or are covered by the generated note: "When `store_locale` is absent, the
  inherited `locale` query parameter is sent unchanged (`override` in `Shop.store`)" (verified in
  proto_keep rustdoc).

**Trade-offs.**

- Alt-Y costs two tokens per optional override (S2: 2 sites; SNAP fixtures: 2 sites) and one
  parser clause.
- It has no runtime or codegen effect, and the composition algorithm is unchanged.
- Migration is compiler-guided: each site errors with the fix.
- It keeps one spelling per behaviour. `else inherit` is required exactly when the source is
  optional, and forbidden otherwise.
- Docs burden: one line in the operations table.

**Recommendation: CHANGE (Alt-Y + DEF-2/3 fixes + generated fallback note).** Also document in the
crate docs (not only SEMANTICS) that:
- exact replacement is `remove "k"; "k" = v;`;
- a reintroduced key keeps its original slot position;
- `x?: Option<T>` + `each` gives the per-call inherit/none/set choice.

Rejected: Alt-X, because of the silent failure on the common path and the silent migration.

**Left unsupported:** a dedicated caller-side "remove the inherited value" control. The route is the
three-state `x?: Option<T>` + `each … else inherit`.

## D05 — Query/header ordering (U11, U32)

The header-order SEMANTICS/test fix belongs to the coordinator and was not touched.

**U11 (control, `control/tests/d05_query.rs`, 4 tests pass):**

- `query("tag") each tags: Vec<String>`: `["b","a","b"]` gives `tag=b&tag=a&tag=b`. `vec![]` gives
  no occurrence and no `?`.
- An optional collection, absent or `Some(vec![])`, is identical on the wire when introduced; the
  two only differ under `override`. `[""]` gives `tag=`.
- Repeated keys stay grouped by slot: `a=[1,3], b=2` gives `a=1&a=3&b=2`.
- `each` over a `HashSet` produced 57 and 63 distinct wire orders in 64 requests across two runs (a
  `BTreeSet` is stable). This is ordinary collection order and needs a doc note.
- Comma joining (`tags=a,b`) is not automatic: a `Csv` newtype gives `tags=a%2Cb`, and `tags=` when
  empty.

**U32 (native exit; real code in `send_with_filters`, loopback-verified `/filter?page=2&f=a&op=eq&f=b&op=ne`):**

```rust
let (client, mut request, decoder) = endpoint.prepare()?.into_parts();
{ let mut pairs = request.url_mut().query_pairs_mut();
  for (field, op) in conds { pairs.append_pair("f", field).append_pair("op", op); } }
let response = client.execute(request).await /* -> reqwest::Error, application wraps it */;
decoder.decode(response).await   // status check + declared decoding preserved
```

To reorder declared keys exactly (`a=1&b=2&a=3`), use `request.url_mut().set_query(Some(..))`
(test `u32_reorder_declared_keys_natively`).

Cost: about 10 lines in a helper. Concord's redacted `Prepared` Debug is lost, and the transport
error is a raw `reqwest::Error` whose `Display` can contain the URL, as SEMANTICS §9 states for
user-driven execution.

Core support would need either dynamic keys, which defeat compile-time sensitivity, override and
remove, or a multi-key "sequence" operation. No studied API needs that.

**KEEP**; document the recipe. Left unsupported: interleaved repeats in the DSL.

## D06 — Body/response representation (U02, U23)

**Control** (`control/src/u23.rs`, `control/tests/d06_repr.rs`, 3 loopback tests pass) covers:
- 7 JSON endpoints, one `Binary` download and upload, an empty-output `DELETE`;
- a required body and an optional body (absent: no body and no content-type);
- a custom NDJSON format in both directions;
- `-> Json<String>` and `-> Json<Option<Item>>`.

Generated signatures as rendered by rustdoc (`out/rerun/sigs_control.txt`):

```text
pub fn create(&self, item: impl IntoValue<<Json<NewItem> as Encode>::Value>) -> InventoryItemsCreate
pub fn import(&self, rows: impl IntoValue<<NdJson<NewItem> as Encode>::Value>) -> InventoryItemsImport
pub async fn send(self) -> Result<<Json<Item> as Decode>::Output, Error>
```

Every endpoint's caller-facing signature is an associated-type projection. rustc error messages do
normalise it ("expected `String`, found `Item`", probe `d06_p4`). The usual author mistake (`-> Item`,
`body item: NewItem`) produces 3–4 E0277/E0599 errors, all spanning the whole macro (DEF-4).

**Alternative 1: payload/representation split, no default.** Prototyped in `EXP/proto_alt`
(`concord::via`: `EncodeAs<T>`/`DecodeAs<T>`, glue `Via<R, T>`; macro emits payload types). Same
fixture: `proto_alt/tests_crate/src/lib.rs`, and the same wire (`via_d06.rs::same_wire_as_control`).
rustdoc (`out/rerun/sigs_via.txt`):

```text
pub fn create(&self, item: impl IntoValue<NewItem>) -> InventoryItemsCreate
pub fn import(&self, rows: impl IntoValue<Vec<NewItem>>) -> InventoryItemsImport
pub async fn send(self) -> Result<Item, Error>
pub fn prepare(self) -> Result<Prepared<Via<Json, Item>>, Error>
```

- **Custom-format cost.** NDJSON is 22 non-blank lines (a marker plus two impls) versus 29 in the
  control (PhantomData marker, `Value`/`Output` associated types).
- **Mismatch diagnostic.** "`Text` cannot decode a `Item` response" (probe `d06_via_p1`). Spans need
  the same `located_at` fix as DEF-4.
- **Declaration cost.** `Item via Json` versus `Json<Item>` is about the same; `Bytes via Binary`
  versus `Binary` is two tokens more.

**Alternative 2: scoped default (`format Json;` on the client; exceptions `via`).** No new
generated code; the endpoint text loses the representation. Executable demonstration of what a
`Json` default would make two innocent declarations mean (`via_d06.rs::default_traps_made_explicit`):

| Author writes, under a `Json` default | Actually means | Result |
|---|---|---|
| `-> String` | `String via Json` | text/plain reply: "JSON decode error (syntax) at line 1, column 1" |
| `DELETE purge() -> ()` | `() via Json` | 204 reply: "JSON decode error (unexpected end of input)" |

The saving is about two tokens per endpoint. It adds an inheritance concept, and readers of one
endpoint cannot see its representation.

**Recommendation.**

- CHANGE to the single spelling `-> T via R` / `body b: T via R`; keep "no `->`" for no typed output.
- Make the representation traits generic over the payload on non-generic markers (`Json`, `Text`,
  `Binary`, user markers).
- Drop the assoc-type `Encode`/`Decode` and generic `Json<T>`. Don't keep both spellings.
- Add `on_unimplemented` messages and located spans.
- `Prepared`/`Decoder` may take `<R, T>` instead of `Via<R, T>`.
- Migration is mechanical: `Json<T>` to `T via Json`, `Text` to `String via Text`, `Binary` to
  `Bytes via Binary`.
- REJECT the scoped default.

Confidence is medium: the decisive evidence is the caller-facing signatures and custom-format
simplicity, not a fresh-user trial. **If the coordinator rejects the migration**, the minimum is the
DEF-4 fixes (`on_unimplemented` + located spans, prototyped in proto_keep), which do not fix the
projection signatures.

**Left unsupported:** representation defaults or inference of any kind.

## D07 — Wire formatting (U21, U22)

**Control** (`control/src/u21.rs`, `control/tests/d07_wire.rs`, loopback):

- **U21.** One `Day` with path `20261102`, query `2026-11-02` and header
  `Mon, 02 Nov 2026 00:00:00 GMT` requires three values and three spellings:
  `api.report(CompactDay(d), d, HttpDay(d))`. The caller can pass inconsistent days. A naive single
  value sends one spelling everywhere.
- **U22.** A foreign `foreign_ts::Timestamp` (modelled on chrono: human `Display`
  `2026-11-02 13:05:09 UTC`) placed directly **compiles and silently sends**
  `since=2026-11-02+13%3A05%3A09+UTC`. The route is `Rfc3339(ts)`/`UnixSecs(ts)` newtypes (orphan
  rule: no `impl Display`). `Duration`/`SystemTime` have no `Display`, which gives a compile error
  spanning the whole macro (DEF-4); with the proto_keep fix it points at the type.
- **Boilerplate.** 5 newtypes are 42 non-blank lines (the formatting logic itself is shared with
  the alternative). Each needs a struct, a derive and an `impl Display` block (about 6 lines of
  ceremony), and the caller wraps at every call.

**Alternative: per-placement adapter `via f` with `fn(&T) -> impl Display`.**
`control/src/u21_adapter.rs` is a hand-written expansion derived from the real
`out/control_expanded.rs`; changed lines are marked `ADAPTER`. Same wire
(`adapter_alternative_same_wire`):
- one input `api.report(d)`; foreign values passed unwrapped;
- 5 adapter fns in 15 lines;
- no new traits, no orphan issue;
- generated `Debug` shows the adapted spelling.

It needs grammar at three sites: placement, block reference and template hole (`{day via compact}`).
It does **not** prevent the silent human-`Display` case (an un-adapted placement still uses
`Display`), and the signature no longer shows the wire spelling (`set_since(impl IntoValue<Timestamp>)`
versus `IntoValue<Rfc3339>`).

**Alternative: wire-value trait without a blanket `Display` impl (reasoned only).** This is the only
option that rejects foreign human `Display` at compile time. But every user type that already has a
correct `Display` needs a second impl, Concord must enumerate std impls, foreign types still need
newtypes (orphan rule), and it does nothing for U21. REJECT.

**Recommendation.**

- KEEP `Display` + newtypes.
- Fix DEF-4 (located spans; prototyped).
- Document the two hazards with the executable examples: a foreign human `Display` is accepted
  silently; one value means one spelling.
- Document the newtype recipe and, for U21, the 3-line inherent wrapper on the generated type
  (`impl Analytics { fn report_for(&self, d: Day) -> AnalyticsReport { self.report(CompactDay(d), d, HttpDay(d)) } }`).
- DEFER the adapter. Revisit if real users report call-site wrapping friction, or several APIs need
  one input with several spellings.

**Left unsupported:** one input with several spellings inside one endpoint.

## D14 — Attributes and generated naming (U27, U18)

**Control (probes `d14_*`, unmodified macro):**

- `#[cfg]`, `#[deprecated]`, `#[allow]`, `#[cfg_attr]` on endpoints, scopes, the client, or values:
  rejected at the attribute ("unsupported attribute here; only documentation comments are allowed").
  This is a limited, correct policy: nothing half-works.
- `#[doc(hidden)]` on an endpoint or value works, hiding the struct, the navigation method and the
  setters (checked in rustdoc). It works because any `doc(...)` passes the filter (DEF-6).
- Visibility is the client's for every generated type; `pub(crate) GET` is a parse error with an
  unhelpful message.
- Required-value docs are dropped (DEF-1).

**Prototype (proto_keep):**
- `#[cfg(..)]` on a scope or endpoint is copied to every generated item of the subtree (structs,
  state, impls, Debug) and to the parent's navigation method.
- `#[deprecated]` goes only on the navigation method.

Results:
- `probe d14_proto_cfg_ok` compiles under `#![deny(warnings)]` with a cfg'd endpoint, a cfg'd scope
  and a deprecated endpoint. A client value used only by the disabled endpoint raises no warning.
- `d14_proto_cfg_dangling`: no method `beta`, and no type `ApiLabsOne`; rustc notes "found an item
  that was configured out" at the declaration.
- `d14_proto_deprecated_call`: the warning appears only at the caller's `.old()`.
- Limitation (`d14_proto_cfg_dup_names`): two mutually exclusive same-name endpoints are still
  rejected, because the macro cannot evaluate `cfg`.

**Names (control).**

| Case | Generated name / behaviour |
|---|---|
| Nested scopes and acronyms | concatenated as written: `HTTPApiApiV2UrlMapGetUrl` |
| Raw identifiers | stripped: `r#type` gives type `HTTPApiApiV2Type`, method `r#type()` (probe `d14_n1`, compiles under `deny(missing_docs)`) |
| PascalCase collisions (`scope a { GET b_c() }` vs `GET a_b_c()`) | macro error with both spans |
| Collisions across two `api!` invocations in one module, or with a user DTO (`Shop` + `item` vs `struct ShopItem`) | rustc E0428 at the macro plus noisy follow-ups (19 error lines for two clients) |
| Two independent clients in one module | fine |

Recommend documenting "one client per module or distinct prefixes"; the macro cannot see other
invocations.

**Recommendation.**

- KEEP reject-by-default.
- CHANGE: accept `#[cfg]` and `#[deprecated]` on scopes and endpoints, with the subtree semantics
  above.
- Fix DEF-1 and DEF-6.
- Keep rejecting `#[allow]`/`#[cfg_attr]` and attributes on values and on the client (wrap the whole
  `api!` in a `#[cfg]` module instead).

**Left unsupported:** cfg-exclusive same-name alternatives; per-endpoint visibility.

---

## Observations for other tracks (not my decisions)

- **D02/D08 inference.** `impl IntoValue<T>` positional and setter parameters defeat inference:
  - `api.set(v.into_iter().collect())` → E0283 "type annotations needed" (fix:
    `collect::<HashSet<_>>()`). Probe `x_collect_inference`; seen in `d05_query.rs`.
  - `api.get("42".parse()?)` → the misleading "`!` is not accepted for a value of type `u64`"
    (never-type fallback). Probe `x_parse_inference`.
- **Liveness is syntactic.** A value is "used" even if every chain overrides or removes its key
  (DEF-5 is the same-owner extreme).
- **Generated endpoint docs** say only "`GET` endpoint `Shop.store.page`". They mention no
  path/query keys, defaults or output (guide 05).

## Commands actually run (all from EXP unless noted)

- `cargo test --offline -p authoring-control`: control fixtures (d04, d04_fresh_fixture, d05,
  d06, d07); all pass.
- `./run_probes.sh`: compile probes against unmodified SNAP; output in `out/probes/*.stderr`.
- `RUSTC_BOOTSTRAP=1 cargo rustc -p authoring-control --lib -- -Zunpretty=expanded` gives
  `out/control_expanded.rs` (separate target `target-expand`).
- `cargo doc --no-deps -p authoring-control` plus `python3 -I tools/docsigs.py …` for rendered
  signatures and doc placement.
- proto_keep and proto_alt: `cargo test -p proto_*-tests`; `cargo test -p concord` (SNAP suite on
  proto_keep); `./run_probes_proto.sh <proto> [glob]`; `cargo doc`.
- Two fresh-prediction subagents (sonnet). Prompts are `fresh/prompt_*.md`; answers are in
  `fresh/responses.md`.
- **Rerun everything:** `EXP/rerun_all.sh` (pins 1.97.1 via `EXP/rust-toolchain.toml`; logs in
  `out/rerun/`). `EXP/make_patches.sh` regenerates `proto_keep.patch` and `proto_alt.patch` (source
  diff versus SNAP).
- **Pitfall found and fixed:** cargo hashes workspace-member path packages relative to the workspace
  root, so `proto_keep/crates/concord` and `proto_alt/crates/concord` overwrote each other in a shared
  target dir (stale "`concord::via` not found"). The scripts now use `target-proto_keep` and
  `target-proto_alt`.

## Reasoned (not measured)

- Compile-time cost of Alt-Y, remove-first, the docs notes and located spans: nil to negligible
  (macro-time only).
- The `via` design adds one zero-sized glue type per endpoint type.
- The `cfg` prototype re-parses the generated items of cfg'd owners only.
- The adapter's call-site ergonomics claim is based on the hand-written expansion, not a fresh-user
  trial.
