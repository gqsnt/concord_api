# Track RA — representation alternative A (HTTP-contract defaults)

Specialist report, 2026-11-02. Workspace: `snapshots/repr-a-a915c8d` (independent git repo,
`.concord-review.json` original commit `a915c8d05fb1eee5ca428a35b73c1e06828713cb`, snapshot HEAD
`8320ae8`). Nothing committed or pushed. All paths below are relative to that snapshot unless
absolute.

## 0. Verdict in one paragraph

A works and is coherent: one statement form (`request R;` / `response R;`) that is a default on
a client/scope and an exception on an endpoint, nearest owner wins, request and response
independent, endpoints without `->` always drain. On the 20-endpoint corpus it produces a
**byte-identical caller API** (102 identical `pub fn` signatures), passes the **same 18 wire
tests** as the control, cuts representation mentions from 26 to 10 (2 defaults + 8 exceptions
that are exactly the non-JSON endpoints), costs nothing measurable in `cargo check`, and improves
several diagnostics (feature-disabled `Json` reported once; `via` migration error names the exact
replacement). Its weakest point is real and cannot be designed away: **a forgotten exception
for a payload type that both representations accept (`String`, `Vec<u8>`, `Option<T>`…) is not
a compile error** — `-> String` silently means "JSON string" (runtime decode error) and
`body note: String` silently sends `"note"` as JSON (no client-side error at all). Local
readability of a single endpoint is also strictly worse than control.

## 1. Effort observability, provenance, ran vs reasoned

- **Effort:** I cannot independently observe or measure a reasoning-effort setting for this run;
  the only model identity available to me is the environment's (`claude-opus-5-5`).
- **Toolchains:** rustc 1.97.1 (8bab26f4f) pinned; 1.85.1 for MSRV; nightly 1.101.0 (e9bbadf68)
  only for `-Zunpretty=expanded`; hyperfine 1.19.0. All builds `--offline` (`--locked` in the
  snapshot workspace). No new crates.io dependency in Concord; the corpora use `serde_urlencoded
  0.7.1` (already in the lockfile) and reqwest's own `multipart` feature (cached
  `mime_guess`/`unicase`; it enters only the standalone corpus lockfiles).
- **Control identity:** `exp/control_src/` is `git archive HEAD crates/concord
  crates/concord-macros Cargo.lock` taken before any edit; 156/156 blobs re-hashed identical to
  HEAD. `exp/corpus_control` depends on it, `exp/corpus_a` on the prototype `crates/concord`.

**Ran (executed, results quoted below):** baseline workspace tests (443 passed); control corpus
(18 wire tests); prototype workspace tests (443 passed, 0 failed, 8 ignored — same count as
baseline), all four CI feature lanes for `-p concord` (374 / 349 / 365 / 351 passed), clippy
`--workspace --all-targets` (0 warnings), `cargo fmt --check`; MSRV 1.85.1 (354 passed; only the
trybuild `ui` binary mismatches — **pre-existing**: the control's `ui` mismatches identically on
1.85 because `.stderr` files are pinned to 1.97.1); `corpus_a` (18 shared wire + 10 A probes);
renamed-dependency/no-default-features probe; 10 intentional-error captures; nightly
expansions; signature diff; hyperfine timings at 21 and 1050 endpoints.

**Reasoned, not executed:** cost of a built-in `Form` (not implemented); usefulness of
clearing a default (not implemented); rendered rustdoc HTML (I inspected the generated
`#[doc]` attributes in the expansion, not rendered pages); long-term maintenance judgments.
Prose in `docs/*.md` was **not** rewritten (only fenced Rust blocks were migrated mechanically,
plus two DIAGNOSTICS excerpts and `lib.rs` crate docs by hand) — prose still says `via`.

## 2. The A model (settled decisions)

### 2.1 Grammar

```text
client Name(values) { base "..."; request R; response R; statements; children }
scope  name(values) { request R; response R; statements; children }
METHOD name(values, body b: T | body b?: T) -> T { request R; response R; statements }
R := any Rust type path (Json, concord::Text, c4::Binary, Csv, a type alias, ...)
```

- `request R;` / `response R;` are ordinary owner statements (like `path`, `headers`), at most
  once per owner, before children. **The same keyword is the default (client/scope) and the
  exception (endpoint)** — an endpoint is simply the nearest owner of itself. One spelling, no
  second synonymous DSL; `via` is removed.
- Signatures carry Rust types only: `POST create(body product: NewProduct) -> Product`.
- `T via R` now fails with the exact replacement (§6, E5). `exp/tools/migrate_via.py` converts
  mechanically.

### 2.2 Semantics

| Question | Decision | Reason |
|---|---|---|
| Precedence | Nearest owner on the endpoint's chain, **independently** for request and response | an API may take forms and return JSON (#18); the probe `nearest_owner_wins_independently` overrides the response at a scope while requests stay JSON |
| No `->` under a response default | No typed output: any 2xx, body drained, default ignored | a response default means "how typed outputs are decoded", not "every response is decoded" (fixes D06 trap 2); rustdoc says the default "does not apply" |
| No `body` under a request default | No body, no content type; default ignored | item 5.3.1 |
| JSON unit vs no output vs ignored output | no `->` = no typed output (drained, never parsed); `-> ()` = a typed unit decoded by the effective representation (JSON `null`); `-> serde::de::IgnoredAny` = JSON validated then discarded | type-based, no spelling magic |
| `-> ()` under an **inherited** representation | compile error with both remedies; accepted when the endpoint spells its own `response` | catches the common D06 mistake. This is the only spelling-keyed code and it is a **diagnostic, never a behaviour**: alias `type Unit = ()` is accepted and behaves exactly like `-> ()` + local `response Json;` (probe `unit_alias_decodes_json_null_and_no_output_drains`) |
| Endpoint-local statement that can never apply (`response` without `->`, `request` without `body`) | compile error | an exception that does nothing is a misunderstanding; inherited defaults that do not apply are fine |
| Missing representation | compile error at the payload type naming both statement forms and the trait (§6, A-E1) | 5.3.7 |
| Clearing a default | **Not provided** | No corpus endpoint needs it; any subtree can override with a concrete type; the only use is to force explicitness in a heterogeneous subtree, achievable by not declaring the default on the client (declare it on the uniform scopes). It would add a sentinel (`response none;`) with a third meaning |
| Media type | The representation proposes `CONTENT_TYPE`; a declared `content-type` header disposes (runtime rule unchanged: representation's value only if the headers lack one). #5 = inherited `Json` + endpoint `headers { "content-type" = "application/merge-patch+json"; }`; #10 = `request Binary;` + `image/png` header. A per-type alternative (`request MergePatch;`, downstream 12 lines) also works | media type is a header fact; no new syntax. Rustdoc now says when a declared `content-type` replaces the representation's |
| Accept | Never emitted (representation defaults never add headers); declare `accept` as an ordinary header if the API needs it. Response `Content-Type` is never checked | no hidden negotiation; a codec default must not change header composition (CHARTER §7) |
| Feature-disabled `Json` | The default is emitted once as a hidden alias at its owner → **one** E0425 at the declaration with rustc's "gated behind the `json` feature" note; inheriting endpoints add no errors | control reports one error per `via Json` |
| `#[cfg]` | Aliases/endpoint items carry the owner's cfgs; cfg'd-out scopes/endpoints may name types that do not exist | probe `a_semantics.rs` (`cfg(any())` scope with `response DoesNotExistWhenDisabled;`) compiles |
| Renamed dependency | `R` is a path resolved at the call site: `request c4::Binary; response c4::Text;` | `exp/probe_nofeat_a` (`c4 = { package = "concord", default-features = false }`) |
| Custom codec | Ordinary `Encode<T>`/`Decode<T>` impls on a downstream marker, named like any built-in | `Csv` (#11/#12), `Form` (#18), `MergePatch` in `exp/shared/codecs.rs` |
| Built-in `Form` | **Keep downstream** (decided, not implemented) | Downstream is 12 lines through the public trait with zero Concord change and passes #18 in both corpora; serde_urlencoded cannot encode nested/sequence fields, so a built-in would pick a form dialect (a policy); +1 feature lane. A recipe suffices |
| Type spelling | Nothing keyed on `String`/`Option`/`Bytes`/`Json*`; aliases of payloads and of representations behave identically | probes in `exp/corpus_a/tests/a_semantics.rs` |

### 2.3 Rustdoc of the effective representation

Each endpoint method states the effective representation and **where it came from**; client and
scope docs state their defaults. Quoted from `exp/measurements/corpus_a_expanded.rs`:

```text
/// Body (`product`): `NewProduct`, encoded with `Json` (inherited from the client `Shop`).
/// Output: `Product`, decoded from a 2xx response with `Json` (inherited from the client `Shop`).
/// Output: `String`, decoded from a 2xx response with `Text` (declared on this endpoint).
/// Output: none (`()`): any 2xx succeeds and the response body is drained; the response default `Json` (inherited from the client `Shop`) does not apply.
/// Representation defaults for endpoints below: request bodies `Json` (inherited from the client `Shop`); typed outputs `Json` (inherited from the client `Shop`). An endpoint's own `request`/`response` statement replaces them; an endpoint without `->` drains its response.
```

(The `Content-Type: the declared content-type header when present, else Json's.` suffix for
#5/#10 was added after the expansion snapshot; it is in the final prototype.) Control equivalents:
`/// Body (`product`): `NewProduct` via `Json`.` / `/// Output: `String` via `Text`.` /
`/// Output: none (`()`): any 2xx succeeds and the response body is drained.`

## 3. The corpus in both syntaxes

Domain types, codecs, native recipes and wire tests are **shared files** (`exp/shared/`), so the
only difference between the crates is the `api!` block. Full files:
`exp/corpus_control/src/lib.rs`, `exp/corpus_a/src/lib.rs`.

Control (excerpt; every payload names `via`):

```rust
scope catalog {
    path "catalog";
    GET products(query page?: u32) -> Vec<Product> via Json { path "products"; }
    POST create(body product: NewProduct via Json) -> Product via Json { path "products"; }
    PATCH patch(id: u64, body patch: ProductPatch via Json) -> Product via Json {
        path "products/{id}";
        headers { "content-type" = "application/merge-patch+json"; }
    }
    DELETE delete(id: u64) { path "products/{id}"; }
    PUT set_discount(id: u64, body discount: Option<Discount> via Json) { path "products/{id}/discount"; }
    GET description(id: u64) -> String via Text { path "products/{id}/description"; }
    PUT upload_image(id: u64, body png: Bytes via Binary) {
        path "products/{id}/image";
        headers { "content-type" = "image/png"; }
    }
    POST import(body rows: Vec<ProductRow> via Csv) -> ImportReport via Json { path "import"; }
}
scope account {
    POST login(body form: LoginForm via Form) -> TokenResponse via Json { path "account/login"; }
    POST search(body query?: SearchQuery via Json) -> SearchResults via Json { path "account/search"; }
    GET version() -> String via Json { path "account/version"; }
}
```

A (same excerpt):

```rust
base "https://shop.example.test/v2";
request Json;
response Json;

scope catalog {
    path "catalog";
    GET products(query page?: u32) -> Vec<Product> { path "products"; }
    POST create(body product: NewProduct) -> Product { path "products"; }
    PATCH patch(id: u64, body patch: ProductPatch) -> Product {
        path "products/{id}";
        headers { "content-type" = "application/merge-patch+json"; }
    }
    DELETE delete(id: u64) { path "products/{id}"; }
    PUT set_discount(id: u64, body discount: Option<Discount>) { path "products/{id}/discount"; }
    GET description(id: u64) -> String { path "products/{id}/description"; response Text; }
    PUT upload_image(id: u64, body png: Bytes) {
        path "products/{id}/image";
        request Binary;
        headers { "content-type" = "image/png"; }
    }
    POST import(body rows: Vec<ProductRow>) -> ImportReport { path "import"; request Csv; }
}
scope account {
    POST login(body form: LoginForm) -> TokenResponse { path "account/login"; request Form; }
    POST search(body query?: SearchQuery) -> SearchResults { path "account/search"; }
    GET version() -> String { path "account/version"; }
}
```

| Metric (api! block, doc comments excluded) | Control | A |
|---|---|---|
| Non-blank lines | 49 | 51 |
| Non-whitespace characters | 1732 | 1675 (−3%) |
| Representation mentions | 26 (`via`) | 10 (2 defaults + 8 exceptions) |

## 4. Generated signatures and ordinary calls

`exp/measurements/corpus_{control,a}_sigs.txt`: all 102 `pub fn` lines sorted — `diff` is
empty ("SIGNATURES IDENTICAL"). From the A expansion:

```rust
pub fn create(&self, product: impl ::concord::IntoValue<NewProduct>) -> ShopCatalogCreate
pub fn set_discount(&self, id: impl ::concord::IntoValue<u64>, discount: impl ::concord::IntoValue<Option<Discount>>) -> ShopCatalogSetDiscount
pub fn search(&self) -> ShopAccountSearch
pub fn set_query(mut self, value: impl ::concord::IntoValue<SearchQuery>) -> Self
pub fn maybe_query(mut self, value: ::core::option::Option<SearchQuery>) -> Self
ShopCatalogCreate:      pub async fn send(self) -> ::core::result::Result<Product, ::concord::Error>
ShopCatalogDelete:      pub fn prepare(self) -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error>
ShopCatalogDescription: pub async fn send(self) -> ::core::result::Result<String, ::concord::Error>
ShopAccountVersion:     pub async fn send(self) -> ::core::result::Result<String, ::concord::Error>
```

(#8 raw text and #20 JSON string have the same signature in both alternatives; only the
declaration/rustdoc distinguish them.) Ordinary calls are identical in both crates — the wire
tests are one file, `exp/shared/wire_tests.rs`:

```rust
api.catalog().set_discount(7, None).send().await?;            // body `null`
api.account().search().send().await?;                         // no body, no content type
api.account().search().set_query(SearchQuery { .. }).send().await?;
let () = api.catalog().delete(7).send().await?;               // empty 204 OK
let v: String = api.account().version().send().await?;        // JSON "2.4.1" -> 2.4.1
```

## 5. Wire evidence

`exp/shared/wire_tests.rs` (loopback server copied from `crates/concord/tests/support`), run
against both crates: **control 18/18, A 18/18**. Covers #1–#20 (all of #5, #6, #7, #15, #19,
#20 required), empty text/bytes bodies, N1 multipart, N2 per-request content type, redacted
`Debug`. Decisive assertions: #5 `header_all("content-type") == ["application/merge-patch+json"]`
(exactly one line); #6 empty 204 **and** a non-JSON 200 succeed, 404 is `ErrorKind::Status` with
the unread body; #7 body `null` + `application/json`; #15 `request().body().is_none()`, no
content type; #19 absent → no body/no CT, `maybe_query(None)` likewise; #20 `"2.4.1"` decodes,
raw `2.4.1` is `ErrorKind::Decode`; every request carries `accept: */*` (reqwest's default — no
negotiation).

A-only probes, `exp/corpus_a/tests/a_semantics.rs` (10/10 pass): aliases (`Txt`, `Unit`,
`MaybeDiscount`, `Raw`, a type named `JsonDocument`, representation aliases `Wire`/`Plain`);
nearest-owner independence; `#[cfg(any())]` scope/endpoint naming nonexistent types; merge-patch
as an exception; and the two forgotten-exception cases:

```text
forgotten exception -> Display: GET Ex.health: response not decoded: JSON decode error (syntax) at line 1, column 1
forgotten_request_exception_is_silent_on_the_client: body == br#""fragile""#, content-type application/json, send() Ok
```

## 6. Intentional errors (actual compiler output)

Captured from `exp/errors/{control,a}` (`exp/measurements/err_*.txt`).

**E1 — raw-text endpoint, representation forgotten.** Control:

```text
error: the response representation is explicit: write `String via Json` (or `via Text`, `via Binary`, or your own representation type)
  --> src/lib.rs:15:25
15 |         GET health() -> String { path "health"; }
   |                         ^^^^^^
```

A with no default anywhere:

```text
error: the output `String` has no response representation: declare `response R;` on this endpoint, an enclosing scope or the client (`Json`, `Text`, `Binary`, or your own type implementing `concord::Decode<String>`); an endpoint without typed output omits `->`
  --> src/lib.rs:15:25
```

A **under a JSON default: no compile error** (runtime decode error, §5) — the decisive difference.

**E2 — `-> ()` for "no output".** Control: `error: an endpoint without typed output has no `->`:
remove `-> ()` (the body is drained)`. A:

```text
error: `-> ()` is a typed output decoded with `response Json` (inherited from the client `E2`): an empty 204 would be a decode error. For no typed output (any 2xx, body drained) remove `-> ()`; to decode a unit with `Json`, declare `response Json;` on this endpoint
  --> src/lib.rs:25:35
```

**E3 — bytes endpoint, JSON representation.** Both report serde's bound (Concord's
`on_unimplemented` does not fire because the `T: DeserializeOwned` blanket impl matches); control
points at the written `Json`, A at the endpoint's `Bytes`:

```text
error[E0277]: the trait bound `concord::bytes::Bytes: serde::Deserialize<'de>` is not satisfied
   --> src/lib.rs:36:31
 36 |         GET image(id: u64) -> Bytes { path "products/{id}/image"; }
    = note: required for `Json` to implement `concord::Decode<concord::bytes::Bytes>`
```

For a payload with no impl at all, A's relocated label reads (UI test
`repr_inherited_unsupported.stderr`): ``this output's response representation is `Text` (declared
on the endpoint or inherited); it needs `Text: Decode<Item>` `` at the endpoint's `Item`. Initially
the inherited type's tokens carried the *client's* span, so every failing endpoint pointed at the
client line; fixed by naming a hidden per-owner alias located at the endpoint's payload type.

**E4 — `Json` with the `json` feature off, two endpoints.** Control: 2 × E0425 (one per `via`).
A: **1** error, at the default:

```text
error[E0425]: cannot find type `Json` in crate `concord`
   --> src/lib.rs:45:27
 45 |         response concord::Json;
note: found an item that was configured out
205 | #[cfg(feature = "json")]
    |       ---------------- the item is gated behind the `json` feature
```

Hazard found with it (`err_a_cfg_default_hazard.txt`): a **client-level** `response c4::Json;`
whose only users are `#[cfg(feature = "json")]` endpoints still fails without the feature (the
default is checked eagerly). Remedy: declare such a default on a `#[cfg]`-gated scope. Control
has no such hazard.

**E5 — leftover `via` (A).** `error: `via` was replaced by representation statements: write
`response concord::Text;` in this endpoint's block, or once on an enclosing scope or the client
for every endpoint below it` at the `via` token.

**E6 — exception that cannot apply (A).** `error: this endpoint has no typed output (`->`): its
response body is drained, so a `response` representation would never be used (remove it, or
declare `-> T`)`.

Also: duplicate statement (`repr_duplicate.stderr`), unknown default reported even when unused
(`repr_unknown_default.stderr`). Pre-existing wart, not fixed: a `///` doc comment directly before
`response R;` is parsed as a child attribute and yields "expected `scope`, an HTTP method … or an
owner statement (…, `response`), found `response`" (same for `path`).

## 7. Native customization (identical code for both)

`exp/shared/native.rs`, compiled unchanged against both crates: streaming #17 via `send_raw` +
status check + `chunk()` loop; multipart N1 via `into_parts` →
`RequestBuilder::from_parts(..).multipart(reqwest::multipart::Form)` → `decoder.finish(..)`
(typed `Product`, 413 still `ErrorKind::Status`); per-request `image/webp` via `into_parts` +
`headers_mut().insert(CONTENT_TYPE, ..)`; merge patch either as a declared header or as the
downstream `MergePatch` representation exception. No Concord change was needed for any of them;
`Prepared`/`Decoder` are untouched by A.

## 8. Implementation cost

| Item | Value |
|---|---|
| Prototype diff (`exp/prototype_a.diff`, 1804 lines) | 51 files, +666 −229 incl. tests/docs/examples |
| Macro + runtime source (`crates/*/src`) | +403 −126 lines; non-comment non-blank: **+333 −101** (most of it diagnostics, rustdoc text and the resolution in `model.rs`); runtime: doc/label text only, no behaviour change |
| New concepts in the macro | per-owner effective `Repr` (type, origin, alias) resolved during `flatten`; 4 new compile errors; 1 hidden alias per declaring client/scope |
| Tests changed | 7 new UI tests (`repr_*`), 2 rewritten (`representation_missing_via`, `representation_alpha_spelling`: the alpha `Json<T>` hint is gone), 4 stderr re-snapshotted for line text only; `migrate_via.py` rewrote 57 occurrences in 18 `.rs` files and 29 in fenced blocks of 6 `.md` files; `lib.rs` crate docs and 2 DIAGNOSTICS excerpts by hand |
| Expanded size (nightly `-Zunpretty=expanded`, whole crate incl. shared modules) | 21 endpoints: control 5407 lines / 249 KB, A 5473 / 254 KB (+1.2%); 1050 endpoints: 126 383 / 5.36 MB vs 129 258 / 5.61 MB (+2.3% lines). Difference = longer doc strings + 2 aliases per declaring owner; the generated *code* differs only in `decoder::<__concord_Shop_response, T>` vs `decoder::<Json, T>` |
| `cargo check` of the corpus crate (hyperfine, 2 warm-up + 10 runs, `touch src/lib.rs`, deps built; machine shared with other tracks, load avg 4.6 — **rough**) | 21 endpoints: control 158.5 ± 10.4 ms, A 169.4 ± 38.4 ms; 1050 endpoints in one `api!` (`--features scaled`, generated by `exp/tools/scale_corpus.py`): control 1.160 ± 0.025 s, A 1.181 ± 0.034 s. **No measurable difference** (raw: `exp/measurements/check_times.json`) |

## 9. Truth table — CHARTER §5.3 under A

| # | Requirement | A's answer | Evidence |
|---|---|---|---|
| 1 | No body: no manufactured body/content type | Request default consulted only when `body` is declared; bodyless → `None`, no CT; local `request` on a bodyless endpoint is an error | `c15_cancel_no_body`, `c01_c02`, `probe_nofeat_a` test; UI `repr_unused_local` |
| 2 | Omitted body vs JSON null vs empty text vs empty bytes | `body b?: T` absent → no body/CT; required `Option<T>` + Json → `null`; `""` / `Bytes::new()` → present empty body with CT; bodies cannot have defaults (unchanged) | `c19`, `c07`, `option_alias_body_none_is_null`, `c16_c10_empty_bodies_are_present` |
| 3 | No-output endpoint under JSON default must not decode an empty 204 | No `->` = drain, default ignored (rustdoc says so); literal `-> ()` under an inherited default = compile error | `c06_delete_drains` (204 empty and non-JSON 200 OK); UI `repr_inherited_unit`; `err_a_e2` |
| 4 | JSON string vs raw text; JSON unit vs ignored | `-> String` under Json = JSON string; raw text = `response Text;`. Unit: `-> ()` with local `response Json;` decodes `null`; ignored = no `->` | `c08_c20_text_vs_json_string`; `unit_alias_…` probe |
| 5 | No spelling special-casing; aliases don't change behaviour | Behaviour is purely type/trait-driven; the one spelling-keyed code is the `-> ()` *diagnostic* (alias `Unit` passes it and behaves like `-> ()` + local statement) | `a_semantics.rs` alias probes |
| 6 | Independent defaults, nearest owner, overrides, clearing | Yes / yes / endpoint statement / clearing not provided (reasons §2.2) | `nearest_owner_wins_independently` |
| 7 | Missing format | Without any default: precise compile error. **Under a default: no error, the default applies** (documented in rustdoc with origin) | `representation_missing_via.stderr`, `err_a_e1`; runtime probes §5 |
| 8 | Media-type headers, no negotiation | Representation CT only when headers lack `content-type`; declared header wins (exactly one line); rustdoc says so; no Accept; response CT not checked | `c05`, `c09_c10`, `n2`, `assert_client_headers` |
| 9 | Custom codecs downstream | Ordinary trait impls; named in defaults or exceptions | `Csv`, `Form`, `MergePatch` (`exp/shared/codecs.rs`); `c11_c12`, `c18` |
| 10 | Native/one-shot data | `send_raw`, `into_parts`, `Decoder::finish`, reqwest multipart — unchanged | `c17`, `n1`, `n2` |
| 11 | Status and metadata | Runtime untouched: 2xx policy, unread error response, `send_raw` before consumption, decoder after native execution | `c06` (404), `n1` (413 after native send) |
| 12 | Integration | Feature-disabled default: one error at the declaration (but eager: client-level gated default needs a cfg'd scope); cfg'd owners may name absent types; renamed deps via paths; rustdoc states origin; `via` migration error + `migrate_via.py` | `err_a_e4`, `err_a_cfg_default_hazard`, `a_semantics` cfg probes, `probe_nofeat_a`, `examples/renamed` |

## 10. Honest assessment versus control

- **One endpoint in isolation — control wins.** `GET version() -> String via Json` is complete
  on one line. In A, `GET version() -> String { path "account/version"; }` requires reading up
  to two enclosing owners (or the rustdoc) to know the bytes on the wire. A no-output endpoint
  reads the same in both.
- **A 20-endpoint API — A wins on signal.** 26 `via` → 10 statements; the 8 exceptions are
  exactly the 8 non-JSON operations, so heterogeneity is visible at a glance; signature lines read
  as Rust. Character count −3%, line count +2 (exceptions written as statements). Rustdoc
  compensates for non-locality by naming the origin on every endpoint.
- **Migration from `via` — cheap and mechanical, safe only before hoisting.** The tool migrated
  every code occurrence in the repository to endpoint-local statements and the full suite stayed
  green (443). Hoisting to client defaults is a manual second step, and it is exactly the step that
  creates the weakest-point exposure for *future* endpoints.
- **Maintenance — modest.** +333/−101 code lines in the macro, none in runtime behaviour; a new
  resolution axis that is deliberately simpler than query/header composition (no
  append/override/remove). Two error-placement subtleties were found and fixed in the prototype
  (inherited spans; duplicate resolution errors). Docs need a real rewrite (prose not done here).
- **Error quality — mixed.** Better: one error for a disabled `Json`, actionable `via`/`-> ()`/
  dead-exception errors, E0277 at the endpoint. Worse: the trait label cannot name the origin
  ("declared on the endpoint or inherited"), and the main authoring mistake (forgotten exception
  for a type both formats accept) produces **no** compile error.
- **Weakest point:** silent inheritance for dual-meaning payloads. `-> String`/`-> Vec<u8>` turn
  a forgotten `response Text;`/`Binary;` into a runtime decode error; `body note: String` with a
  forgotten `request Text;` sends a JSON string that only the server can reject. D06's trap 2
  (empty 204) is fully solved; trap 1 is converted from "silent wrong meaning" into "documented
  well-defined meaning", but remains invisible at the endpoint and uncatchable by the compiler.
  Mitigation if A is chosen: declare defaults on uniform scopes rather than the client in
  heterogeneous APIs, and rely on the rustdoc origin line.

## 11. Artifact paths

- Prototype diff: `exp/prototype_a.diff` (tracked files outside `exp/`, new UI tests included).
- Corpora: `exp/corpus_control/`, `exp/corpus_a/`; shared: `exp/shared/{domain,codecs,native,support,wire_tests}.rs`.
- Probes: `exp/corpus_a/tests/a_semantics.rs`, `exp/probe_nofeat_a/`, `exp/errors/{control,a}/`.
- Tools: `exp/tools/{migrate_via.py,migrate_via_md.py,scale_corpus.py,doc_lines.py}`.
- Measurements: `exp/measurements/` (expansions, signature lists, `err_*.txt`, `check_times.json`).
- Pristine control: `exp/control_src/`. Build dirs `exp/target*` (~5.5 GB) are disposable.
