Original source SHA: a915c8d05fb1eee5ca428a35b73c1e06828713cb
Independent snapshot HEAD: 583bef9370a7bc806d48ccd03a65f7e718dc08d1
Review mode: execute; requested model: gpt-6.1-sol; requested effort: xhigh.
Verify effective settings; report personally executed versus inspected-only evidence.
Never push, publish, modify global settings, bypass sandbox, or claim acceptance before triage.

## Supplied document: design_prompt.md

# Codex task — blind design challenge: native headers and the body/response DSL

You are an independent reviewer from a different provider. Nobody has chosen a design yet; you
see the owner's charter (`CHARTER.md`), the comparison corpus (`CORPUS.md`) and the current source
(this snapshot of `a915c8d`). You do not see, and must not guess, any proposal by the local team.
Your job is to challenge the problem *before* a design is chosen, with executable evidence.

Read first: `CHARTER.md`, `CORPUS.md`, then `crates/concord/src/{__private.rs,repr.rs,prepared.rs,value.rs,lib.rs}`,
`crates/concord-macros/src/{syntax.rs,model.rs,emit.rs}`, `v4/SEMANTICS.md`,
`v4/refinement/DECISIONS.md` (D06, D07, D12 especially). Rust 1.97.1 is pinned; MSRV is 1.85
(`cargo +1.85`).

## Part 1 — native header boundary (compile everything you claim)

Under `target/probes/headers/` build small crates that depend on this snapshot's `concord` (path
dependency) or directly on `http`/`reqwest` as needed, and determine with compiled code:

1. Which input-boundary designs for header values are coherent on stable 1.85 and 1.97.1: e.g. a
   Concord trait implemented for `HeaderValue`/`&HeaderValue`/text/integers with an explicit
   formatting escape; reusing `http`'s own `TryFrom<_> for HeaderValue` impls; a syntax-directed
   choice in the DSL; any other design you consider viable. Show which pairs of impls conflict
   (E0119 etc.) and which inference failures appear (integer literals, `&'static str`, aliases,
   `Option<HeaderValue>`, `Vec<HeaderValue>` with `each`, foreign `Display` types, borrowed
   values read from `&self` state in generated code).
2. What generated code must look like for: a header value read by reference from a shared owner
   (`Arc` state) per request; an initializer `= HeaderValue::from_static("...")`; a `static`
   reused with `.clone()`; literal header templates lowered to compile-time-validated constants.
   What allocations are unavoidable (HeaderValue clone is a `Bytes` clone; header-map growth).
3. Sensitivity: how the native flag must combine with Concord's declaration/sticky classification
   in direct values, `each`, `override`/`append`/`remove` and inherited composition, and how every
   generated `Debug` (config struct, client, scope, request) and `Prepared`'s `Debug` must render a
   `HeaderValue` that is sensitive / not sensitive / not UTF-8. Identify every place in the current
   source that would reject a `HeaderValue` (no `Display`) or leak it.
4. The visible-ASCII restriction on formatted text: keep, relax, or split by source? Give the
   concrete product reason either way.

## Part 2 — body/response DSL (complete alternatives, not token counts)

The current surface (`body b: T via R`, `-> T via R`) is the control. Build, as far as you judge
useful, at least two materially different alternatives against the corpus: (A) HTTP-contract
defaults declared on the client/scope with endpoint exceptions, (B) a locally explicit compact
format annotation without inherited defaults. Write the 20-endpoint corpus in the control syntax
(it must compile against this snapshot) and sketch A and B in full. You may prototype grammar
changes in this snapshot's macro if that is the fastest way to get evidence; production sources
must be restored or the change reported.

For each design give: declaration → generated signatures → calls → rustdoc summary → an
intentional error and its diagnostic → native customization (merge-patch content type, image/png
upload, streaming, multipart) → implementation/codegen cost. Then fill the charter's §5.3 truth
table (items 1–12) for your recommended design, with explicit answers for: no body; optional body
absent vs JSON `null` vs empty text/bytes; a no-output endpoint under a JSON response default (204);
JSON string vs raw text; payload aliases (`type Raw = String`, `type Unit = ()`); nearest-owner
precedence and clearing; missing format; Content-Type and Accept; custom codecs; native exits;
status/metadata; cfg/features/renamed crates.

## Part 3 — traps

List the concrete traps any candidate must survive, each as a runnable check (compile-pass,
compile-fail or loopback wire assertion) you have written under `target/probes/`. Rank by severity.

## Output

A report (your final message) with: environment and effective settings (model and effort as
observed, not assumed), what you personally executed vs only inspected, Part 1–3 findings with file
paths of the probes, your recommended design for each question and the strongest alternative you
rejected, and open risks. Be concrete; quote compiler output where it decides a question.


## Supplied document: CHARTER.md

# Owner charter — Concord V4: native headers, representation design, focused hardening

(Owner request of 2026-11-02, reproduced for specialists and reviewers. Sections on team
organisation are abbreviated; every product, semantic and evidence requirement is kept.)

## 1. Mission

The owner accepts the lean direction but requests two substantive improvements:

1. Headers must accept native `reqwest::header::HeaderValue` directly, including prebuilt static
   values, without forcing them through text formatting and reparsing.
2. The request-body encoding and response-decoding DSL must be reconsidered. The current
   `body x: T via R` / `-> T via R` surface does not feel sufficiently natural. This is a
   product-design request, not merely a documentation request.

Also close the localized findings from the latest review. Improve authoring, calling, runtime
behavior, generated documentation and maintenance together. Deliver implemented changes, tests and
completed independent review — not a proposal alone. Prioritize correctness and coherent design over
speed. Consequential questions need executable alternatives, a decision and an implementation.

## 2. Starting point

Branch `concord-v4-refine`, HEAD `a915c8d` (docs-only after code revision `b0fe26e`). Records are
evidence to check, not instructions overriding this request.

## 3. Fixed product boundaries

Concord is a small typed API-domain layer over a caller-supplied `reqwest::Client`. Keep clients,
scopes, typed endpoints, meaningful required/optional/defaulted values, predictable composition,
sensitivity-aware Concord diagnostics, minimal representations and practical native exits.

Do not add: authentication orchestration or credential fetching/refresh; retry/replay management;
rate admission; pagination traversal; a managed executor, plugin registry or generic middleware
framework. Ordinary supplied secrets and cursor parameters remain data.

Keep payload types visible in generated caller signatures. Keep native request/response access and
the ability to retain a typed decoder after native execution. Do not introduce a second transport
implementation.

An explicit representation default is not forbidden inference. The prohibition is guessing protocol
meaning from the spelling of a Rust type. Equally, do not force one universal encoding abstraction
across path segments, query pairs, headers and bodies: these targets have different semantics.
Prefer existing Rust and native HTTP types where they already model the problem correctly. Measure
the complete cost to authors, callers and maintainers, not just runtime line count.

## 4. Native header support — required outcome

### 4.1 User scenarios

Support an ordinary typed header value sourced from:
- an owned `HeaderValue`, including one supplied at client construction or through a setter;
- a `const` or `static` initialized with `HeaderValue::from_static` and reused across requests. A
  straightforward borrowed/cloned route must work without requiring a wrapper around the native type;
- an optional or defaulted native header value;
- repeated native header values through the existing repeated-value mechanism (`each`);
- a native value whose bytes are valid for the native header type but are not representable as a
  normal string;
- a value already marked sensitive through its native sensitivity flag.

Keep ordinary textual header authoring convenient. Preserve useful numeric and custom-formatting
routes or provide a small, explicit, documented migration — not a hidden break.

Desired capability, not frozen grammar:

```rust
use concord::reqwest::header::HeaderValue;
static ACCEPT_JSON: HeaderValue = HeaderValue::from_static("application/json");
```

The chosen DSL must let an ordinary header input/default hold `ACCEPT_JSON.clone()` or
`HeaderValue::from_static(...)`, without converting it back into a `String`.

### 4.2 Semantics and implementation requirements

- A native value is already a header value. Preserve its bytes exactly. Do not call `to_str`,
  `Display`, `Debug`, `to_string`, or reparse bytes merely to insert it.
- Clone or move it as ownership requires, without mutating shared caller-owned values. A required
  clone is not a failure; unnecessary formatting, allocation and validation are.
- Decide separately how text, templates and native values enter the final native header map.
- Reassess the current visible-ASCII policy: do not reject a valid native value solely because the
  old formatting helper only handled text. Any retained restriction needs a concrete product reason
  and a clear migration explanation.
- Prove the implementation on the declared MSRV (1.85).
- Do not recognize `HeaderValue` by parsing a Rust type-name string: aliases, qualified paths and
  renamed dependencies must work.
- Do not use runtime `Any`/`TypeId`, unstable specialization or unsafe code as a shortcut. A small
  trait/adapter is acceptable when justified; do not build a generic conversion framework.
- Compile concrete coherence tests before committing to traits. In particular, do not assume an
  unrestricted `impl<T: Display> ...` fallback can coexist with native-type implementations.
- Compare a native-conversion-first boundary with an explicit formatting escape against another
  viable design. Include foreign custom types, aliases, borrowed inputs and inference failures in the
  experiment.
- Do not over-promise zero allocation: distinguish an already-created header's payload reuse from
  allocations in the header map or overall request.
- Test constant/literal paths and inspect generated code. Only statically known literals may be
  lowered to native compile-time-validated constants. Arbitrary Rust expressions must not be
  evaluated by the macro or hoisted in ways that change initializer semantics.

### 4.3 Sensitivity is part of the value

- Never clear an incoming native sensitivity flag just because the declaration has no marker. At
  minimum the outgoing flag must preserve incoming sensitivity and add Concord's effective
  classification. Do not blindly reuse `set_sensitive(declaration_sensitive)`.
- Audit generated `Debug` for config/client/scope/request types as well as `Prepared`, formatting
  failures, conversions and inherited composition. A header can compile and transmit correctly while
  its owner `Debug` still rejects its type or discloses its content. Both paths must work.
- Keep existing sticky classification guarantees. Define precisely how runtime-native sensitivity
  participates in repeated values, inherited replacement and removal. Use the smallest conservative
  rule that satisfies the guarantees; no generic runtime taint engine.
- If an explicit user conversion extracts bytes into an unrelated value, distinguish that
  application responsibility from direct native forwarding.
- A native header is not implicitly a string for query/path/template reuse. Unsupported
  cross-placement reuse needs a clear diagnostic or explicit conversion, never lossy implicit
  decoding.
- Tests must check sensitivity flags separately from value equality, and include positive controls
  showing secrets really reach the local server.

### 4.4 Header acceptance cases

Owned/static/defaulted/optional/repeated values; aliases and renamed dependencies; empty versus
absent; native bytes; setters and snapshot behavior; native-sensitive plus declaration-sensitive
combinations; unmarked custom header names; override/remove/append; full-map errors; generated
diagnostics and Debug; disabled-feature/cfg declarations. Preserve same-name value order. Do not
restore a cross-name wire-order promise. Use raw-byte assertions where a text-only test server
would conceal behavior.

## 5. Reopen the body/response DSL as a product decision

Do not merely rename `via`, add an alias, or improve documentation while retaining the same
repetitive primary authoring path. The owner explicitly requests a substantive revision. Simple
existing runtime traits may be retained if appropriate; the DSL need not mirror them.

### 5.1 Separate the questions before choosing syntax

- Domain input/output type: `NewItem`, `Item`, `String`, `Bytes`.
- Request-body presence: absent body, present empty body, JSON null.
- Request representation: JSON, form, text, bytes, native body, custom format.
- Response representation: JSON, text, bytes, custom decoder.
- Response handling: buffered typed output, no typed output/drain, native response.
- Status and metadata: success check, unread error response, headers/status before consumption.
- Scope/default selection: explicit client/scope default versus endpoint exception.

Do not conflate serialization with compression, Content-Type with Accept, or body decoding with an
HTTP execution policy. Request encoding and response decoding must be independently selectable: an
API may accept a form and return JSON.

### 5.2 Compare complete alternatives

Use the current refined DSL as the control. Build at least two materially different alternatives:

A. An HTTP-contract-oriented design. Endpoint signatures describe Rust inputs/outputs. Separate
   declarative request/response choices may be inherited from an explicitly configured client or
   scope, with local exceptions.

B. A locally explicit format-oriented design. A compact, readable annotation or format-qualified
   body/response declaration avoids broad inherited defaults.

Hypothetical direction worth testing (neither keywords nor grammar mandated):

```text
client Catalog {
    base "https://catalog.example.test";
    request Json;
    response Json;

    GET item(id: u64) -> Item { path "items/{id}"; }
    POST create(body item: NewItem) -> Item { path "items"; }
    GET health() -> String { path "health"; response Text; }
    DELETE remove(id: u64) { path "items/{id}"; }
}
```

Test the strongest viable version of each proposal. Do not reject defaults solely because `String`
can be either JSON string data or raw text: an explicitly chosen JSON default gives it a well-defined
meaning, while a raw-text exception can be explicit. A no-output endpoint need not inherit a body
decoder merely because siblings do. The previous rejection of defaults (decision D06) is evidence to
reconsider, not a veto. Conversely, do not adopt defaults just because they are illustrated here.
Their exception rules, local readability, feature behavior and maintenance cost must earn their
place.

For every alternative show: declaration → generated signatures → ordinary calls → rustdoc →
intentional error → native customization → implementation/codegen cost. Include a representative
20-endpoint API with mostly JSON plus text, bytes, no-output and custom-format exceptions. Do not
select from a two-endpoint showcase. Choose one coherent primary design; do not ship several
synonymous DSLs. Exceptional syntax can remain where it has a distinct purpose. Keep the generated
API ordinary: users pass/receive payloads, not incidental transport wrappers, unless the operation
explicitly requests a native result.

### 5.3 The full contract (truth table required before implementation)

1. No request body: representation defaults must not manufacture a body or misleading content type.
2. Required, optional and defaulted inputs: distinguish omitted body, JSON null, empty text and
   empty bytes.
3. A no-output endpoint under a JSON response default must not become JSON decoding of an empty 204.
4. JSON string versus raw text; unit-valued JSON versus deliberately ignored output.
5. Payload aliases: no syntax-based special casing of `String`, `Option`, `Bytes`, `()`, or a name
   containing `Json`. Explicit DSL output modes may have syntax; aliases must not silently change
   behavior.
6. Independent input/output defaults: nearest-owner precedence, endpoint overrides, whether
   clearing a default is useful and how it is expressed.
7. Missing format: a precise compile-time error or an explicitly documented default, never
   undocumented type guessing.
8. Media-type headers: no hidden content negotiation; explain whether headers are emitted,
   inherited, overridden or checked.
9. Custom codecs: a foreign payload type through a normal downstream Rust type/function/trait, not
   a registry.
10. Native and one-shot data: streaming responses, request-specific content types and multipart use
    native reqwest where appropriate rather than expanding a buffered codec framework.
11. Status and metadata: preserve 2xx/non-2xx behavior, unread error responses,
    metadata-before-decoding and the protected native execution/decoder route.
12. Integration: feature-disabled codecs, cfg-disabled endpoints, renamed dependencies,
    documentation and migration diagnostics.

Keep the current explicit 2xx convenience policy unless a demonstrated problem warrants changing
it. Representation syntax does not authorize a new status-policy system. Preserve raw access for
other status/streaming needs. A declared codec does not by itself promise body-size limits or
bounded drain duration. Review `Encode`/`Decode`, constant content-type handling, decoder storage
and ownership against these cases. Reuse existing simple mechanisms where they fit; do not prescribe
dynamic dispatch, a shared universal codec trait, or additional crates before a prototype proves
the need.

## 6. Localized review findings (same campaign)

- `emit.rs::value_notes`: the `contains` scan is quadratic when one value feeds N distinct
  placement descriptions. Preserve deterministic order, add efficient membership tracking,
  benchmark that exact shape; do not claim it explains other regressions without attribution.
- Conditional `Clone` (`for<'__concord> T: Clone`): a compatibility risk, not a current defect. Add
  independent positive/negative downstream tests on MSRV/current stable and a non-blocking nightly
  lane. Keep the strategy isolated; compare an ordinary alternative only if it improves total cost.
- Multipart recipe: replace the handwritten MIME writer with reqwest multipart in a downstream
  example crate, preserving parts + decoder; exercise field names, filenames, bytes, content type
  and status/error handling; enable the feature only where needed; verify separate consumers so
  workspace feature unification cannot conceal core requirements. No multipart policy in Concord.
- Documentation: never advise changing genuinely required inputs to optional/defaulted inputs just
  to reduce method arity (recommend meaningful scopes, domain types or a convenience method with a
  named input record). Correct remaining native-Debug wording without claiming native URL redaction.
- Usability: the next fresh-use trial starts with README/quickstart and ordinary generated docs and
  looks up details on demand; record lookup effort, incorrect predictions and workarounds.
- Release preparation: inspect rendered rustdoc when a rendering tool is available (never disable
  sandbox security); check packaging from the packaged source; keep fixture reproducibility while
  excluding unnecessary test-only material from release artifacts; do not publish. Keep the
  static/runtime base consistency test.

## 7. Experiments, tests and performance

Retain existing oracles, regression tests, feature isolation, doc examples, positive controls and
mutation evidence; update them to the selected surface without weakening assertions. Add
cross-feature tests for native headers plus representation defaults and exceptions: a
runtime-sensitive header stays protected without a compile-time marker; an optional native header
does not affect whether a body exists; a codec default does not affect ordinary header presence or
inheritance. Measure static/native/text headers separately (several values, repeated preparation
from a reused client/scope); separate header payload reuse from header-map allocations; demonstrate
no `Display` requirement for native inputs with compile-positive tests. Re-run compile-scale
workloads (distinct placement descriptions for one value; repeated constant headers; typed
body/response declarations; inherited default exceptions; shared contributions; multi-crate
consumers). Report absolute timings, baseline, identities, feature sets, sample counts and noise.
Keep `#[inline]` unless new paired measurements justify a change. For important defects:
baseline pass → intended mutant/assertion failure → restored pass.

### Performance infrastructure — Criterion

Replace the custom `Instant`-based runtime harness with a maintained Criterion suite; keep the HTTP
loopback infrastructure and `hyperfine` compile benchmarks. Criterion covers request construction,
native versus formatted headers, request composition, body encoding, response decoding and complete
HTTP round trips; the same scenarios in Concord and equivalent hand-written reqwest; realistic
parameter combinations and payload sizes; setup, input construction, request construction,
execution and decoding separated where relevant; `black_box`, batching, meaningful groups;
statistical confidence, absolute times, relative differences, environment limits; raw Criterion
data retained; allocations measured separately; a reproducible baseline before implementation
changes; no expensive benchmarking in ordinary CI (dedicated jobs only). A benchmark is not a
correctness test.

## 8. Fixed process rules

No push, publish, merge, reset, global-setting change or destructive cleanup. Preserve V3, the
alpha, all branches/tags and user work. A reviewer cannot certify its own implementation.
Execution reviews build from fresh writable disposable snapshots with source/lockfile identity.


## Supplied document: CORPUS.md

# Comparison corpus — representation DSL (20 endpoints) and header scenarios

Every representation alternative (control `via`, A contract defaults, B local format) implements
**this same API** so they can be compared line by line. The API is fictional; base
`https://shop.example.test/v2`. Wire facts below are the contract — an alternative that cannot
express one must say so explicitly.

## Domain types (shared; serde derives as needed)

```rust
pub struct Product { pub id: u64, pub name: String, pub price_cents: u64 }       // JSON object
pub struct NewProduct { pub name: String, pub price_cents: u64 }                // JSON object
pub struct ProductPatch { pub name: Option<String>, pub price_cents: Option<u64> } // merge patch
pub struct Discount { pub percent: u8 }                                         // JSON object
pub struct ProductRow { pub sku: String, pub name: String, pub price_cents: u64 } // CSV line
pub struct ImportReport { pub imported: u32, pub rejected: u32 }                // JSON
pub struct OrderPage { pub orders: Vec<Order>, pub next_cursor: Option<String> } // JSON
pub struct Order { pub id: u64, pub status: String }                            // JSON
pub struct NewOrder { pub product_id: u64, pub quantity: u32 }                  // JSON
pub struct LoginForm { pub username: String, pub password: String }             // form fields
pub struct TokenResponse { pub access_token: String, pub expires_in: u64 }      // JSON
pub struct SearchQuery { pub text: String, pub limit: Option<u32> }             // JSON
pub struct SearchResults { pub hits: Vec<Product> }                             // JSON
```

`Csv` is a **downstream** custom format (hand-written, no extra crate): one header line
`sku,name,price_cents`, one line per row, content type `text/csv; charset=utf-8`.

## Client

`Shop`, required sensitive header `authorization` (String), header `x-client-version` initialized
to `"7"`. Scopes: `catalog` (path `catalog`), `orders` (path `orders`), `account` (no path).

## Endpoints (method, path, request body, response)

| # | Endpoint | Request body | Response (2xx) |
|---|---|---|---|
| 1 | `GET catalog/products?page=` (`page` optional u32) | none | JSON `Vec<Product>` |
| 2 | `GET catalog/products/{id}` | none | JSON `Product` |
| 3 | `POST catalog/products` | JSON `NewProduct` (required) | JSON `Product` |
| 4 | `PUT catalog/products/{id}` | JSON `NewProduct` (required) | JSON `Product` |
| 5 | `PATCH catalog/products/{id}` | `ProductPatch` serialized as JSON but sent with `Content-Type: application/merge-patch+json` | JSON `Product` |
| 6 | `DELETE catalog/products/{id}` | none | 204, no typed output (any body drained) |
| 7 | `PUT catalog/products/{id}/discount` | JSON `Option<Discount>` — **required body**; `None` sends the JSON text `null` | 204, no typed output |
| 8 | `GET catalog/products/{id}/description` | none | raw UTF-8 text (`text/markdown`) → `String` |
| 9 | `GET catalog/products/{id}/image` | none | raw bytes → `Bytes` |
| 10 | `PUT catalog/products/{id}/image` | raw bytes `Bytes`, content type `image/png` | 204, no typed output |
| 11 | `GET catalog/export.csv` | none | custom `Csv` → `Vec<ProductRow>` |
| 12 | `POST catalog/import` | custom `Csv` ← `Vec<ProductRow>` | JSON `ImportReport` |
| 13 | `GET orders?status=&cursor=` (both optional String) | none | JSON `OrderPage` |
| 14 | `POST orders` | JSON `NewOrder` | JSON `Order` |
| 15 | `POST orders/{id}/cancel` | none (no body at all, no content type) | 204, no typed output |
| 16 | `POST orders/{id}/notes` | raw text `String` (`text/plain; charset=utf-8`) | 204, no typed output |
| 17 | `GET orders/{id}/invoice.pdf` | none | raw bytes → `Bytes` |
| 18 | `POST account/login` | form-urlencoded `LoginForm` | JSON `TokenResponse` |
| 19 | `POST account/search` | **optional** JSON `SearchQuery` — absent means no body and no content type | JSON `SearchResults` |
| 20 | `GET account/version` | none | a JSON **string** (`"2.4.1"`) → `String` |

Also required, outside the 20: a streaming download and a multipart upload shown through the native
exits (`Prepared::into_parts` / `send_raw` / `Decoder`), not through a codec.

## Header scenarios (for the native-header track)

H1 owned `HeaderValue` client value (config and setter). H2 `static ACCEPT: HeaderValue =
HeaderValue::from_static(..)` reused as an endpoint default and as a per-call value. H3 optional
and initialized native values. H4 `each` over `Vec<HeaderValue>` (order preserved). H5 bytes valid
for `HeaderValue` but not UTF-8/visible ASCII (`HeaderValue::from_bytes(b"caf\xe9")`). H6 a value
whose native sensitive flag is set, declared without `#[sensitive]`, on an unlisted header name.
H7 the same value fed through `override`/`append`/`remove` on inherited headers. H8 aliases
(`type Hv = HeaderValue;`), qualified paths, a renamed `reqwest`/`concord` dependency. H9 numeric
and custom-`Display` header values (current behaviour) and their migration. H10 cross-placement
reuse: the same native value used in a query or path template → clear diagnostic.


## Supplied document: ENV.md

# Execution environment for this run

- Sandbox: workspace-write confined to this snapshot, network access enabled for this run only so
  loopback tests can bind sockets (the Linux workspace sandbox forbids all sockets otherwise). Do not
  contact external services; everything needed is local.
- `CARGO_HOME` is a private registry prefetched with `cargo fetch --locked`, located OUTSIDE the
  snapshot (granted as an extra writable root). `CARGO_TARGET_DIR` is `target/review-build`; probe
  crates may use their own target dirs under `target/`. Build with `--locked` (add `--offline` if
  cargo tries the network; probe crates without a lockfile may use `--offline`).
  Toolchains: 1.97.1 (rust-toolchain.toml), 1.85 (`cargo +1.85`), nightly.
- Put probes under `target/probes/`; restore or report any production-source change; the original
  source integrity is checked after the run. Stop any server you start before finishing.
