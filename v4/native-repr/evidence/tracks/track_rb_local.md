# Track RB — representation DSL alternative B (locally explicit formats)

Model: Claude Opus 5.5 (`claude-opus-5-5`), running as a sub-agent. I can't observe my own
reasoning-effort setting from inside the session, so effort is not reported. Workspace:
`/home/canarit/projects/concord_v4_native_work/snapshots/repr-b-a915c8d` (an independent repo;
`.concord-review.json` says `original_commit a915c8d05fb1…`, 443 files; HEAD `03a8726`). Nothing was
committed or pushed. Toolchains: 1.97.1 (pinned), 1.85.1 (MSRV), nightly 1.101 (expansion only).
Every build ran `--offline`/`--locked`. No new crates.io packages were added.
`serde_urlencoded 0.7.1` was already in Cargo.lock; the only lockfile change is one new dependency
edge.

I did not read other tracks' reports, alternative A among them. Every A comparison below rests on
my own sketch (`exp/spellings/a_sketch_for_comparison.txt`), which is marked as such.

---

## 1. The chosen B: the format is a word, placed where the body or output is declared

```text
GET|POST|… name(values) [-> FORMAT Payload] { statements }
body (in the parameter list, at most one):
      FORMAT name: T          required body
      FORMAT name?: T         optional body (absent: no body, no Content-Type)
      FORMAT("media/type") name: T    this endpoint's media type for the body
FORMAT: json | text | bytes | form | a type implementing Encode<T>/Decode<T> (e.g. Csv, codecs::Csv)
no `->`: no typed output — any 2xx succeeds, the body is drained (unchanged)
```

The corpus in B (`exp/corpus_b/src/api.rs`, compiled):

```rust
GET product(id: u64) -> json Product { path "products/{id}"; }
POST create(json product: NewProduct) -> json Product { path "products"; }
PATCH update(id: u64, json("application/merge-patch+json") patch: ProductPatch) -> json Product { path "products/{id}"; }
DELETE delete(id: u64) { path "products/{id}"; }
PUT set_discount(id: u64, json discount: Option<Discount>) { path "products/{id}/discount"; }
GET description(id: u64) -> text String { path "products/{id}/description"; }
PUT set_image(id: u64, bytes("image/png") png: Bytes) { path "products/{id}/image"; }
GET export() -> Csv Vec<ProductRow> { path "export.csv"; }
POST import(Csv rows: Vec<ProductRow>) -> json ImportReport { path "import"; }
POST login(form credentials: LoginForm) -> json TokenResponse { path "account/login"; }
POST search(json query?: SearchQuery) -> json SearchResults { path "account/search"; }
GET version() -> json String { path "account/version"; }
```

The same rows in the control (`exp/corpus_control/src/api.rs`):

```rust
POST create(body product: NewProduct via Json) -> Product via Json { path "products"; }
PATCH update(id: u64, body patch: ProductPatch via Json) -> Product via Json {
    path "products/{id}";
    headers { "content-type" = "application/merge-patch+json"; }
}
POST login(body credentials: LoginForm via Form) -> TokenResponse via Json { path "account/login"; }
// plus: use concord::{Binary, Json, Text}; use crate::form::Form; and a 13-line downstream Form codec
```

The runtime traits are unchanged: `Encode<T>` keeps `CONTENT_TYPE`, and `Decode<T>`, `Prepared`,
`Decoder`, drain and status handling are as before. The DSL no longer mirrors the trait
(`T via R`). It mirrors reqwest's own vocabulary instead: `.json()`, `.form()`, `.text()`,
`.bytes()`. It also mirrors Concord's placement grammar: `query x`, `header("k") x`, `json x`,
`json("media/type") x`.

## 2. Spellings explored before choosing

I wrote the full corpus in three B spellings. Only β was compiled; α and γ are text, in
`exp/spellings/`.

| Spelling | Body | Output | Format/body tokens (20 endpoints) | Chars |
|---|---|---|---|---|
| control `T via R` | `body p: NewProduct via Json` | `-> Product via Json` | 61, plus 2 imports | 1761 |
| **β — format as the body placement (chosen)** | `json p: NewProduct` | `-> json Product` | **26** (a regex count that includes one doc-comment mention; 25 in the declarations), no imports | **1550** |
| α — format word before the type | `body p: json NewProduct` | `-> json Product` | 36 | 1596 |
| γ — one attribute per endpoint | `#[json] POST f(body p: NewProduct) -> Product` | (same attribute) | 36 tokens, but each `#[..]` is 3–4 tokens | 1693 |

Why β:
1. **Reach.** It has the fewest tokens, and they sit next to the data they govern. The format
   word *replaces* `body`, so a JSON-in/JSON-out endpoint pays exactly one word per direction.
2. **Consistency.** It extends the existing placement grammar instead of adding a construct. A
   media type sits in the same `(..)` slot a header's name does.
3. **Mixed directions.** Mixed endpoints stay natural, with no extra syntax:
   `form credentials: LoginForm) -> json TokenResponse`. γ needed `#[body(form), output(json)]`.
4. **α's weak spots.** α keeps `body` and gets awkward with media types:
   `body patch: json("…") ProductPatch`.
5. **γ's weak spots.** γ detaches the format from what it governs; you have to look up to learn
   that `-> String` is JSON. Its `#[json(content_type = …)]` silently applies to the body only. γ
   is really a one-endpoint default, which is A's mechanism in miniature.

## 3. Settled semantics

- **Independent directions.** Each direction carries its own word, so there is nothing to inherit,
  override or clear. Wire tests: #18 form in/JSON out; #11 Csv out; #12 Csv in/JSON out.
- **Output modes.**
  - No `->` drains, outputs `()`, and accepts any 2xx (#6: a 204 and a 200 with a non-JSON body
    both succeed).
  - `-> json ()` is JSON unit: it decodes `null`, and an empty 204 is a decode error (probe).
  - "Deliberately ignored" means no `->`. There are no new modes.
- **Absent body vs JSON `null`.**
  - `json d: Option<Discount>` is a required body; `None` sends `null` with
    `application/json` (#7).
  - `json q?: SearchQuery` is optional; absent means no body and no Content-Type (#19).
  - Empty text or empty bytes is a present body: the Content-Type is sent. One observed fact,
    identical in the control: hyper sends **no** `content-length: 0` for an empty body.
- **Media types belong to the body representation.**
  - `json("application/merge-patch+json")` is validated by the macro, then emitted as
    `const __CT: HeaderValue = HeaderValue::from_static(..)`, so it is checked at compile time
    and costs nothing at runtime.
  - It is sent exactly when the body is. This removes a real control trap: the control's only
    route is a declared `content-type` header, which is sent with no body when an optional body is
    absent. The probe `exp/corpus_control/tests/probes.rs::header_media_type_is_sent_without_a_body`
    passes on the control.
  - A declared `content-type` header still replaces the format's *default* (the documented rule,
    still needed for dynamic values).
  - A declared header **plus** a body media type is a compile error, not a silent precedence rule.
  - A media type on an output (`-> json("…") T`) is rejected: the response Content-Type is not
    checked and no Accept is sent.
- **Accept.** Concord never adds it. The wire test asserts `accept: */*`, which is reqwest's
  default.
- **Missing format.** A missing format is an exact compile error; see §6.
- **Feature-disabled `json`/`form`.**
  - The word expands to a located `$crate::Json`/`$crate::Form`. With the feature off, rustc
    reports one error at the word, plus its own "gated behind the `json` feature" note.
  - I measured a `compile_error!` `macro_rules` variant first. Its message was nicer, but it
    produced a cascading second E0277 (`format () cannot decode`), so I dropped it.
- **`#[cfg]` endpoints and renamed dependency.** `#[cfg(any())] GET disabled() -> json DoesNotExist`
  compiles. Under `concord4 = { package = "concord" }`, `json`, `text` and `crate::Lines` work
  without any import or `concord4::` prefix. In the control, `via concord4::Text` must be
  qualified or imported.
- **Custom codec.** Downstream `Csv` is an ordinary `impl Encode<Vec<ProductRow>>` /
  `impl Decode<…>`, with no registry. Its encode error is a Build error raised before sending
  (w12).
- **Form: built in behind a non-default `form` feature.**
  - It uses serde_urlencoded (already in the lock) and provides `Encode` + `Decode`, about 28
    runtime lines.
  - Evidence:
    - The downstream route costs every user 13 lines plus a direct `serde_urlencoded` dependency.
    - OAuth token endpoints need it.
    - reqwest 0.13 itself made `form` opt-in, and mirroring that keeps the default dependency set
      unchanged.
    - Under B, a downstream `Form` would be spelled with a type while `json` is a word, an
      asymmetry the built-in avoids.
  - This decision is mostly independent of syntax; the control could adopt it too.
- **No type-spelling special cases.** The probes cover:
  - aliases `type Str = String`, `type Unit = ()`, `type MaybeNote = Option<Note>` and
    `type Raw = Bytes`;
  - a payload named `JsonNote`;
  - a format-type alias `formats::Json = concord::Text`, which behaves as Text.

  Behaviour follows only the written format. The macro looks at type spellings solely to choose
  *error hints* (`-> ()`, alpha `Json<T>`, `via Json`).
- **Generated rustdoc** (rendered and checked in `exp/target/doc/corpus/api/struct.ShopCatalog.html`):
  > Body (`patch`): `ProductPatch` encoded as JSON (`Content-Type: application/merge-patch+json`).
  > Output: `Product` decoded as JSON (2xx).
  > Body (optional `query`): `SearchQuery` encoded as JSON (`Content-Type: application/json`); absent: no body and no `Content-Type`.

  The control renders `` Body (`patch`): `ProductPatch` via `Json`. `` and puts `content-type`
  under Headers. The B docs state the effective content type, and render types without
  `Vec < T >` spacing.

## 4. Declaration → signatures → calls

The generated signatures are **identical** in both syntaxes, quoted from the
`-Zunpretty=expanded` output in `exp/expanded/`:

```rust
pub fn update(&self, id: impl ::concord::IntoValue<u64>, patch: impl ::concord::IntoValue<ProductPatch>) -> ShopCatalogUpdate
pub fn prepare(self) -> ::core::result::Result<::concord::Prepared<Product>, ::concord::Error>
pub async fn send(self) -> ::core::result::Result<Product, ::concord::Error>   // (send body elided)
pub fn set_query(mut self, value: impl ::concord::IntoValue<SearchQuery>) -> Self   // optional body
pub fn maybe_query(mut self, value: ::core::option::Option<SearchQuery>) -> Self
```

The calls are therefore identical too. `exp/corpus_*/tests/wire.rs`, `src/domain.rs` and
`src/native.rs` are **byte-identical** between the two corpora (checked with `cmp`), for example
`api.catalog().update(7, patch).send().await?`.

## 5. Native customization (identical source in both, `src/native.rs`)

- Streaming: `invoice(id).prepare()?.send_raw()`, then a `chunk()` loop with our own status check.
- Multipart:
  1. Declare `POST attach(id) -> json Product` with no body.
  2. Call `into_parts()`, then `RequestBuilder::from_parts(..).multipart(form)` using reqwest's own
     multipart writer. The `multipart` feature is enabled only in the corpus crates.
  3. Finish with `decoder.finish(..)`. Headers, the 2xx check and JSON decoding are kept, and a
     413 response becomes `ErrorKind::Status`.
- Per-request timeout: `into_parts()`, then `timeout_mut()`, then `Decoder::finish`.
- Merge-patch and PNG need no native exit in B: they are body media types. In the control they are
  header statements.

## 6. Intentional errors — actual diagnostics (rustc 1.97.1)

Probes: `exp/probes/errors_b/`, `exp/probes/errors_control/` (full and short outputs are in the
`*.txt` files next to them), the UI tests `crates/concord/tests/ui/format_*.rs`, and
`exp/probes/no{json,form}_*`.

| Mistake | Control | B |
|---|---|---|
| no format on output | `the response representation is explicit: write \`Product via Json\` (or …)` | `the output format is explicit: \`-> json Product\` (formats: \`json\`, \`text\`, \`bytes\`, \`form\`, or a format type such as \`Csv\`)` |
| format can't carry the type | `` `Text` cannot decode a `Product` response `` (label on `concord::Text`) | `` the output format `Text` cannot decode a `Product` `` (label on the `text` word) |
| forgot import / unknown custom format | `cannot find type \`Json\` in this scope` | (built-in words need no import) `cannot find type \`Csv\` in this scope` |
| `body` kept, no format | `the body representation is explicit: write \`Product via Json\`` | `a body is declared by its format, in place of \`body\`: \`json product: Product\`` |
| body forgotten (plain value) | `value \`product\` is never used; place it …` | same, plus `…, or make it the body by naming its format (\`json product: Product\`)` |
| other syntax's order guessed | `-> Json Product` → `write \`Json via Json\`` (wrong advice) | `-> Product json` → `the format comes first: \`-> json Product\`` |
| old `via` | — | `` `via` was replaced: the format comes first, `-> json Product` `` |
| media type on output | — | `a response format takes no media type: Concord does not check the response \`Content-Type\` and sends no \`Accept\`; …` |
| header + body media type | silently header wins | `the body declares its media type and the headers also set \`content-type\`; keep one …` |
| `json` with feature off | `unresolved import \`concord::Json\`` + gated note | `error[E0425]: cannot find type \`Json\` in crate \`$crate\`` + note `the item is gated behind the \`json\` feature` |

Two of B's error messages came out of this probing. The swapped-order check and the "make it the
body" hint were both added after a first probe gave `cannot find type json` and the bare liveness
message respectively.

## 7. Truth table — CHARTER §5.3 under B

| # | Item | B behaviour | Evidence |
|---|---|---|---|
| 1 | no body | no word → no body, no Content-Type; no default exists to manufacture one | w15, w06 (run) |
| 2 | required/optional/defaulted; null/empty | `json d: Option<T>` None → `null`; `json q?: T` absent → nothing; empty text/bytes → present, with CT; a body default is still rejected | w07, w19, probes (run) |
| 3 | no-output under JSON | no default; no `->` drains any 2xx | w06 (run) |
| 4 | JSON string vs raw text; JSON unit vs ignored | `-> json String` / `-> text String`; `-> json ()` / no `->` | w20, probes (run) |
| 5 | aliases, no spelling special cases | behaviour follows the word only; hints may read spellings | probes (run); one caveat below |
| 6 | independent defaults, precedence, clearing | n/a by construction: always independent, nothing to clear | w18, w11/12 (run) |
| 7 | missing format | precise compile errors | §6 (run) |
| 8 | media-type headers | body CT = format const or declared body media type (sent iff a body is present); a declared header replaces the default; both → compile error; no Accept; response CT not checked | w05, w10, probes, control trap (run) |
| 9 | custom codec | downstream marker + `Encode`/`Decode`, used as `Csv rows:` / `-> Csv T` | w11/12 (run) |
| 10 | native/one-shot | streaming, multipart, timeout via native exits; no codec growth | native_* (run) |
| 11 | status/metadata | runtime unchanged; non-2xx keeps the unread response | w06 404, multipart 413 (run) |
| 12 | integration | no-default suite 349 pass; feature-off error; cfg; renamed; rustdoc; migration errors and script | verify.sh, probes (run) |

Caveat on item 5: in format position a single **lowercase** identifier must be a built-in word.
That rule catches typos such as `qurey page: u32`, but it means a lowercase format *type* must be
path-qualified (`self::csv`). This is a rule about the format's name, not the payload's.

## 8. Implementation cost

- **Macro and runtime diff**: `+514 −108`. That comes from `syntax.rs +316/−52`, `emit.rs +85/−16`,
  `model.rs +39/−4`, `repr.rs +49/−21` (28 of those lines are `Form`), `__private.rs +11/−7` and
  `lib.rs +15/−8`.
  - Roughly 100 of the syntax lines are migration and error hints (the `via` replacement, alpha
    spellings, swapped order, legacy `body`).
  - The grammar itself (format enum, body and output parsing, media-type check) is about 170
    lines.
- **Whole prototype including migration**: 42 files, `+669 −239`, saved at `exp/prototype_b.diff`
  (1879 lines).
- **Expansion of the corpus** (nightly `-Zunpretty=expanded`): control 5431 lines, B 5447 (+16,
  +0.3%).
  - B passes one `Option<HeaderValue>` argument per body.
  - Its merge-patch path is a compile-time const. The control's goes through the runtime header
    path (Display → ASCII check → `from_str`) plus a static `HeaderName`.
- **`cargo check` of the leaf crate** (rough numbers). Setup: `CARGO_INCREMENTAL=0`, dependencies
  prebuilt, `touch src/api.rs`, hyperfine 1.19.0, i5-13600K, other tracks possibly running.
  - Corpus (21 endpoints): control median 0.260 s (mean 0.268 ± 0.030, n=20); B 0.252 s
    (0.261 ± 0.029).
  - 50× scale (150 scopes, 1050 endpoints, `exp/tools/scale.py`), run 1: control 3.406 s
    (n=10); B 3.583 s, which includes a 6.4 s first-run outlier.
  - 50× scale, run 2 with the order swapped: B 3.453 s (3.488 ± 0.082, n=12); control 3.441 s
    (3.561 ± 0.309).
  - **There is no measurable difference.** The raw JSON is in `exp/timing/`.
- **Migration**:
  - `exp/tools/migrate_via.py` rewrote 66 code and 31 Markdown-code-block occurrences across 27
    files. The only code fix-ups were 7 now-unused imports.
  - The script wrongly rewrote one V3 example inside `docs/MIGRATION.md`, which I reverted by
    hand.
  - Prose that I did not migrate: most of `docs/MIGRATION.md` §"Representations: `T via R`" and
    the `DIAGNOSTICS.md` heading names.

## 9. Existing tests — what changed and why

- **Result:** `scripts/verify.sh --msrv` passes every gate: fmt, clippy `-D warnings`, test (443,
  the same count as the baseline of 443), no-default (349), json-only (365),
  reqwest-defaults-only (351), all three doc builds, msrv-concord (368) and msrv-examples (69).
- **One FAIL line:** "tracked files changed". It fires because the working tree is intentionally
  dirty, not because anything changed during the run.
- **Other suites:**
  - `concord --features form`: 374 pass.
  - corpus_b on 1.85.1: 26 pass. With the later empty-body probe it is 27 on 1.97.1; the corpus
    was not re-run on 1.85.1 after that.
  - corpus_control: 21 pass.
- **What changed:** test and example declarations were migrated by the script, plus import
  removals. These UI `.stderr` files were re-blessed because their wording and spans changed:
  `body_errors`, `representation_alpha_spelling`, `representation_missing_via`,
  `representation_unsupported_payload`, `sensitive_body`, `unused_value`.
  - The `representation_unsupported_payload` span now points at the format word, not at a
    `concord::Text` path.
- **What was added:** UI tests `format_migration_via`, `format_errors` (7 cases); a rewritten
  `docs/AUTHOR_GUIDE.md` §5; and updated `DIAGNOSTICS.md` excerpts, which are checked by
  `doc_excerpts`.
- **Mutation check:**
  1. Baseline: everything passes.
  2. Mutant (ignore the declared media type in `encode_body`): w05, w10 and the probe fail.
  3. Restored: everything passes again.

## 10. Assessment

- **Reading one endpoint in isolation: B is best.**
  - The line states the format, the media type, whether the body is present, and whether there is
    an output. There are no lookups and no ancestor that can change its meaning.
  - `-> json String` vs `-> text String` is visible at the use site. D06's traps
    (`String` under a default; `DELETE` decoding an empty 204) cannot occur.
  - A has to answer "which default reaches this endpoint?" from up to two owners away, and editing
    a scope default changes endpoints that were never touched.
- **A 20-endpoint, mostly-JSON API (repetition): A wins.**
  - B writes `json` 17 times in the corpus (11 outputs, 6 bodies). In all it writes 25 format
    words: one per typed output (15) and one per body (10).
  - My A sketch needs 2 default lines plus about 9 exceptions, plus `body` ×10.
  - B removes the control's *per-endpoint* doubling (`via Json` twice), the `body` keyword and the
    imports. Format tokens drop from 61 to 26. It does **not** remove cross-endpoint repetition;
    that is the defining trade.
- **Migration from `via`: B is cheap and mechanical.** It is the same information reordered, the
  compiler gives exact replacements, and the script does it. A also needs restructuring, deciding
  where defaults live.
- **Maintenance: B is small.**
  - No inheritance, precedence, clearing or exception rules, and no composition-time representation
    state.
  - About 170 grammar lines plus hints; the runtime and codegen are unchanged in shape.
  - A adds owner-level state that every endpoint must resolve, and makes the rustdoc depend on
    ancestors.
- **Error quality: B's is equal or better.**
  - Errors point at the word.
  - The control gives wrong advice for format-first guesses (`Json via Json`), and B fixes the
    header/media-type silent precedence.
  - The worst B message is the feature-off case (`cannot find type Json in crate $crate`, though
    rustc's note names the feature).

**B's weakest point:** cross-endpoint repetition in mostly-JSON APIs. If the owner's
"repetitive" means "I write JSON on every endpoint", B does not deliver; it delivers
"naturally readable and locally complete". Secondary weaknesses:

- `json product: NewProduct` no longer contains the word "body". A reader learns that a format
  word marks the body; rustdoc and the error hints say so.
- Built-in words and custom formats have different spellings (`json` vs `Csv`). The general
  mechanism also accepts `concord::Json Product`, a second, undocumented route to the same thing.
- The `bytes` word maps to the marker `Binary`, and trait errors print `concord::Binary`.

**My recommendation:** if the decision weighs local reading, error quality and maintenance at
least as much as cross-endpoint keystrokes, B-β is a coherent primary design. If the owner's
primary complaint is cross-endpoint repetition, B alone does not answer it.

## 11. What I ran vs reasoned; artifact paths

**Ran:**
- every wire, probe and UI test cited above;
- the verify script, including MSRV;
- the expansions, rustdoc and hyperfine timings;
- the mutation check;
- the error probes, the feature-off probes and the renamed-dependency probe.

**Reasoned, not run:**
- α, γ and the A sketch, which were not compiled;
- the A-side counts;
- the claim that hyper's missing `content-length: 0` is shared, which is observed in both corpora
  but not root-caused in hyper.

All paths below are under `/home/canarit/projects/concord_v4_native_work/snapshots/repr-b-a915c8d/`.

**Prototype and corpora:**
- prototype diff: `exp/prototype_b.diff`;
- control corpus: `exp/corpus_control` (depends on the pristine copy `exp/pristine`, built with
  `git archive HEAD crates`);
- B corpus: `exp/corpus_b`.

**Probes and tools:**
- probes: `exp/corpus_*/tests/probes.rs` and `exp/probes/{errors_b,errors_control,nojson_b,nojson_control,noform_b,renamed_b}`;
- tools: `exp/tools/{migrate_via.py,scale.py}`.

**Outputs:**
- spellings: `exp/spellings/`;
- expansions: `exp/expanded/`;
- timings: `exp/timing/`.

**Build directories** (`exp/target*`) can be deleted.
