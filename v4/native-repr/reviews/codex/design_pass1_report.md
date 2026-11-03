I recommend **a borrowed header-input trait with explicit formatting escapes**, and **independent request/response defaults inherited from client or scope, with endpoint exceptions**. Both recommendations have executable prototype evidence. This is a design review, not release acceptance.

The reproducible entry point is [run_review.py](/home/canarit/projects/concord_v4_native_work/snapshots/codex-design-a915c8d/target/probes/run_review.py). It uses offline, locked builds and distinguishes expected compiler failures from successful builds.

**Environment and evidence**

- Observed HEAD: `583bef9370a7bc806d48ccd03a65f7e718dc08d1`.
- Observed toolchains: Rust **1.97.1** and **1.85.1** through `+1.85`; nightly **1.101.0-nightly** for expansion inspection and the non-blocking Clone check.
- Requested model/effort: `gpt-6.1-sol` / `xhigh`. **Effective model and effort were not exposed by the available tools or selected environment variables, so I cannot attest them.**
- The supplied original SHA is unavailable as a Git object in this snapshot. I verified snapshot identity and unchanged files, rather than claiming an independent comparison against that object.
- Root `Cargo.lock` SHA-256: `1880bdf8dfe1402b2e823ebc893b4f1a37025b5e026c73c2372ffccdfc1e8269`.
- All **444 baseline files remain unchanged**; Git status is clean. Changes exist only under `target/probes/`. No push, publication, global-setting change, or sandbox bypass occurred.

Personally executed:

- **54 header/coherence checks**, **8 DSL diagnostic checks**, and **6 native-placement negative checks**, with their expected outcomes on both stable toolchains.
- All **20 endpoints in each of three designs**, against the same loopback wire assertions, on both toolchains.
- Presence, alias, precedence, clearing, custom-codec, native-header, sensitivity, and snapshot checks.
- Native-header runs with JSON enabled and disabled.
- **311 existing tests** across the selected body, representation, sensitivity, composition-oracle, formatting-error, header-limit, and value suites.
- Positive/negative downstream conditional-Clone checks on MSRV, current stable, and nightly.
- Two baseline → mutant failure → restored-pass experiments.

Inspected-only evidence includes dependency allocation internals and generated-code structure. Rustdoc HTML was generated and its text inspected; visual inspection failed because the browser access host was unavailable. The local documentation server was stopped. Detailed identities are retained in [environment-and-integrity.json](/home/canarit/projects/concord_v4_native_work/snapshots/codex-design-a915c8d/target/probes/environment-and-integrity.json).

For headers, the important boundary results are:

| Boundary | Compiled result | Consequence |
|---|---|---|
| Blanket `T: Display` plus native `HeaderValue` implementation | E0119 | Cannot preserve the unrestricted fallback this way |
| Blanket `T: Display` plus `&HeaderValue` implementation | E0119 | Borrowing does not resolve coherence |
| Blanket native `TryFrom<T>` plus blanket `Display` | E0119 | Two unrestricted conversion families conflict |
| Native `TryFrom<T>` plus distinct `Format<T>` | Pass | Viable native-first alternative |
| Borrowed `TryFrom<&T>` plus distinct `Format<T>` | Pass | Viable, with borrowed-input limitations |
| Concrete native/text/numeric implementations plus reference forwarding and `Format<T>` | Pass | My recommendation |

The compiler decides the first question explicitly:

```text
error[E0119]: conflicting implementations of trait `H` for type `HeaderValue`
note: upstream crates may add a new impl of trait `std::fmt::Display`
for type `http::HeaderValue` in future versions
```

The strongest rejected header alternative is **direct reuse of native `TryFrom` conversions with `Format<T>`**. It is coherent and implemented in the runnable `headers/src/bin/conversion_trait.rs`, but generated borrowing makes its authoring behavior uneven:

- Owned integer literal `7`, `&str`, owned `HeaderValue`, and `&HeaderValue` work.
- `&u64`, `&&str`, `u8`, and scalar `Option<HeaderValue>` fail.
- Adding a borrowed integer implementation to the blanket borrowed-conversion trait produces E0119.
- `via_native("x".into())` produces **E0283**, because the conversion target is ambiguous.

The recommended concrete trait handles borrowed native values, strings, and numeric types consistently. Foreign `Display` types migrate explicitly to `Format<T>`. The native type itself needs no wrapper. Aliases and renamed dependencies work through Rust’s type system.

The integrated prototype is [native/src/main.rs](/home/canarit/projects/concord_v4_native_work/snapshots/codex-design-a915c8d/target/probes/native/src/main.rs), backed by an isolated runtime/compiler fork. It compiles owned, borrowed-static, initialized, optional, and repeated native values; custom formatting on endpoint and client owners; setters; snapshots; opaque bytes; and disabled declarations.

Generated preparation must **borrow shared owner state**, then clone the native header into the request map. Moving a field from its `Arc` owner fails with E0507. The prototype preserves these separate paths:

- `= HeaderValue::from_static(...)`: remains an ordinary initializer.
- `= ACCEPT.clone()`: clones the static when the owner is constructed.
- `&'static HeaderValue`: works through reference forwarding.
- A literal template without holes: becomes a Rust-validated `static HeaderValue`.
- A template with holes: retains formatting and validation.

Initializer counters confirm one evaluation per owner construction, with no reevaluation during preparation or cloning. Arbitrary expressions are not macro-evaluated or hoisted. The resulting code is retained in [native/expanded.rs](/home/canarit/projects/concord_v4_native_work/snapshots/codex-design-a915c8d/target/probes/native/expanded.rs).

Native forwarding performs no text conversion or reparsing. Pointer assertions demonstrate payload reuse for static and dynamically created values. That is **not a zero-allocation claim**: `HeaderValue::clone` clones `Bytes`, whose first clone can require shared metadata; request/header-map storage and growth remain separate costs. Allocation counts were not measured.

For sensitivity, I recommend this precise rule:

```text
outgoing sensitivity =
    incoming native flag OR Concord’s effective declaration/name classification
```

Concord’s existing compile-time classification remains sticky through append, override, removal, reintroduction, and inherited composition. A runtime native flag remains **value-local**:

- Forwarding a flagged value preserves its flag.
- Unclassified repeated values preserve each occurrence’s individual flag.
- A Concord-classified slot marks every outgoing occurrence sensitive.
- Removing an unmarked native value removes that value.
- Replacing it with a different public value does not dynamically classify the replacement.
- Optional fallback preserves whichever value survives.

This satisfies the existing classification guarantees without introducing runtime taint propagation. Tests check flags separately from equality and confirm that synthetic secrets reach the server. Same-name header order is asserted; no cross-name wire-order promise is made.

Every generated config/client/scope/request `Debug` must apply declaration classification and inspect native sensitivity. Sensitive values render redacted; public native values use escaped native debug rendering, including non-UTF-8 bytes. Optional absence remains visible as absence. `Prepared` must check sensitivity before rendering bytes.

Current-source rejection and hazard sites are:

| Current location | Finding |
|---|---|
| `__private.rs::Parts::header`, `header_each` | Require `Display`, rejecting native values |
| `DebugDisplay`, `DebugEach`; `emit.rs::debug_fields` | Public native owner fields also fail independently of insertion |
| `emit.rs::source_code` template formatting | Native values cannot implicitly become template text |
| Query/path/subdomain formatting helpers | Correctly reject native cross-placement reuse |
| `Parts::header`’s `set_sensitive(sensitive)` | Correct for newly parsed text; would clear incoming flags if mechanically reused for native values |
| `prepared.rs::DebugHeaders` | Already checks flags; public opaque bytes currently become `<non-ASCII>` |
| `prepared.rs` native-Debug wording | “prints everything” is inaccurate for flagged headers; full native URLs remain exposed |

Direct native values currently fail compilation, so I am **not claiming an existing direct-native disclosure**. The clearing mutation demonstrates the disclosure a naïve implementation would introduce.

I recommend **splitting the ASCII policy by source**: preserve native bytes exactly, while retaining visible ASCII plus tab for formatted text and templates. The product reason is to prevent ordinary Unicode display text from silently becoming opaque HTTP bytes. Native construction provides the explicit route for APIs that require those bytes.

For body/response syntax, the full compiled alternatives are:

- [Control: `T via R`](/home/canarit/projects/concord_v4_native_work/snapshots/codex-design-a915c8d/target/probes/corpus/src/control.rs)
- [A: inherited HTTP contracts](/home/canarit/projects/concord_v4_native_work/snapshots/codex-design-a915c8d/target/probes/corpus/src/contract.rs)
- [B: local format annotations](/home/canarit/projects/concord_v4_native_work/snapshots/codex-design-a915c8d/target/probes/corpus/src/local.rs)

A uses:

```rust
request Json;
response Json;

POST create(body item: NewProduct) -> Product { path "products"; }
GET description(id: u64) -> String {
    path "products/{id}/description";
    response Text;
}
DELETE remove(id: u64) { path "products/{id}"; }
```

B uses locally explicit annotations:

```rust
POST create(body[Json] item: NewProduct) -> [Json] Product;
GET description(id: u64) -> [Text] String;
```

All three generate ordinary payload APIs: `create(item: impl IntoValue<NewProduct>)`, `prepare() -> Result<Prepared<Product>, Error>`, and `send() -> Result<Product, Error>`. Calls remain ordinary payload calls; optional search uses `search().set_query(payload)`.

Generated rustdoc states the effective request body, output representation, URL, and drain behavior in every alternative. A production implementation of A should additionally identify the owner supplying an inherited format.

Intentional errors were executed:

| Design | Diagnostic |
|---|---|
| Control, missing representation | “the response representation is explicit: write `String via Json`…” |
| A, body below a cleared default | “request body has no representation; add `request Json;` here or on an owner…” |
| B, `[Text] u32` output | E0277: “`Text` cannot decode a `u32` response” |

The prototype deliberately accepts a comparison superset. Production should choose one primary surface and update the retained `via` wording in trait diagnostics.

All designs passed merge-patch content-type overrides, PNG uploads, downstream CSV, form requests returning JSON, streaming through `send_raw`, metadata before decoding, and retained typed decoders after native execution. A separate example also compiles a downstream codec for foreign `Ipv4Addr` payloads.

**Multipart is not verified.** The standalone consumer fails before compilation because the offline cache lacks `mime_guess`:

```text
error: no matching package named `mime_guess` found
required by package `reqwest v0.13.5`
```

Its attempted recipe is retained in `target/probes/multipart/src/lib.rs`; it is not counted as compile or wire evidence.

A and B elaborate into the existing representation model. After normalizing module names, crate paths, and whitespace, **all three corpus expansions are identical**. No dynamic codec machinery is needed. A adds inheritance resolution, clearing rules, provenance documentation, and migration diagnostics. B has simpler resolution and stronger local readability.

I prefer **A** because the corpus has a shared JSON contract with meaningful exceptions. **B is the strongest rejected alternative**: it is correct and self-contained, but repeats the shared contract throughout this API. This preference is supported by executable semantics, not a claimed usability study.

The recommended design’s §5.3 contract is:

| # | Explicit answer |
|---|---|
| 1 | Without a body declaration, defaults create neither a body nor a content type. |
| 2 | Required `Option<T>` with `None` sends JSON `null`. An optional DSL body can be absent; present empty text/bytes remain present. Defaulted placed inputs retain eager initialization. **Body initializers remain rejected**, consistently across the tested surfaces. |
| 3 | No arrow means drain and return `()`, including under a JSON default and on 204. |
| 4 | `String` under JSON means JSON string data; `response Text` means raw UTF-8. Explicit unit output decodes JSON `null`; it does not mean ignore. |
| 5 | `Raw`, `Unit`, `Maybe`, `Blob`, and a type named `ContainsJson` behave according to the selected format. Aliases do not change semantics. |
| 6 | Request and response defaults resolve independently, nearest owner first. Endpoints override either. `request -;` / `response -;` clear defaults and require explicit choices when needed. |
| 7 | A body or typed output without an effective format is a compile-time error. No Rust-type guessing. |
| 8 | Encoder content type is emitted only for a present body; composed headers win. Concord adds no `Accept`. The loopback client itself emitted reqwest’s `*/*`. Response content type does not select decoding. |
| 9 | Keep downstream `Encode<T>` / `Decode<T>` implementations and ordinary marker types; no registry. |
| 10 | Keep streaming, request-specific native bodies/content types, and multipart at native exits. Streaming is executed; multipart remains blocked as above. |
| 11 | Preserve 2xx convenience behavior, unread non-2xx responses, metadata-before-consumption, and protected `Decoder::finish`. No size-limit or bounded-drain promise follows from a codec. |
| 12 | Unused defaults introduce no codec bounds; disabled endpoints compile without their payload types/codecs. Renamed dependencies work. Diagnostics and documentation need production migration updates. |

The runnable traps are ranked below; paths are relative to `target/probes/`.

| Severity | Trap and retained check |
|---|---|
| P0 | Clearing native flags exposes secrets: `run_mutants.py`, `native-mutant-clears-flag.log` |
| P0 | Owner `Debug` protection must accompany insertion: `native/src/main.rs`, covering all generated owner types and `Prepared` |
| P1 | No-output endpoints accidentally decode empty bodies: `corpus/tests/truth.rs`, drain mutation |
| P1 | Trait coherence and borrowed conversion gaps: `headers/src/bin/coherence_*`, `conversion_*` |
| P1 | Absence, JSON null, empty text/bytes, and aliases: `corpus/tests/truth.rs` |
| P1 | Composition, optional fallback, empty `each`, snapshots, and same-name order: `native/src/main.rs` |
| P1 | Opaque bytes must survive transport: raw-byte loopback assertions in `native/src/main.rs` |
| P1 | Native values must not silently become query/template text: `native-fail/src/bin/*` |
| P1 | Status errors must preserve unread responses and native execution must retain decoding: `corpus/tests/wire.rs` |
| P2 | Invalid constants, full maps, and failing `Display`: `headers/src/bin/const_invalid.rs`, `pass.rs`, and native formatting assertions |
| P2 | Conditional Clone compatibility: `headers/src/bin/clone_pass.rs`, `clone_fail.rs`, including nightly logs |

Remaining risks are concrete: multipart needs its missing dependency and wire assertions; the prototype needs production diagnostics, documentation, and literal-static deduplication; visual rustdoc inspection remains incomplete. I did not run Criterion, allocation profiling, compile-scale benchmarks, packaging checks, or a fresh-use study. Inspection confirms the quadratic `value_notes` membership scan, but I have not benchmarked that shape or attributed other regressions to it.