# Track H — native header inputs and sensitivity

Workspace: `snapshots/headers-a915c8d` (checked: `.concord-review.json` `original_commit` =
`a915c8d05fb1eee5ca428a35b73c1e06828713cb`; snapshot HEAD `4b928aa`). The prototype is the
uncommitted working tree. Paths below are relative to the snapshot unless absolute.

| Artifact | Path |
|---|---|
| Prototype diff (tracked changes + new files, `exp/` excluded) | `exp/prototype.diff` |
| Coherence lab (own workspace, own lockfile; its `target/` is a 744 MB build cache, gitignored) | `exp/coherence/` (`run.sh`; output in `out/1.97.1/`, `out/1.85/`, `out/summary.txt`) |
| Gate receipt (`scripts/verify.sh --msrv`) | `exp/verify-msrv.txt` |
| Mutation receipt | `exp/mutants/run.py`, `exp/mutants/result.txt` |
| Allocation receipts | `exp/alloc/prototype.txt`, `exp/alloc/baseline-a915c8d.txt` (+ the baseline variant of the test, `exp/alloc/baseline_headers.rs`) |
| Compile-cost receipts | `exp/compile/` (`gen.py`, `check.md`, `check-400.md`, `sizes.txt`, `expanded-small.rs`) |

**Ran** means I compiled/executed it and the receipt is in the paths above. **Reasoned** means it
follows from reading source (cited) and was not executed.

## 0. Decision

**Design A, native-conversion-first.** No new syntax. A value placed directly as a header
(`header("k") x: T`, `"k" = x`, `"k" = each xs`) goes through one small open trait,
`concord::ToHeaderValue { fn to_header_value(&self) -> Option<HeaderValue> }`, implemented for
`HeaderValue` (clone: bytes and flag preserved), `str`/`String` (visible ASCII only), the integers
`u8`–`u64`/`usize`/`i8`–`i64`/`isize`, `bool`, and `&T`/`Box`/`Rc`/`Arc`/`Cow` of those. There is no
`Display` fallback (it cannot coexist: E0119, below). Formatting stays available explicitly through
the existing template syntax (`"k" = "{x}"`), and a domain type implements the trait itself. The
outgoing sensitivity flag is **incoming native flag OR Concord's static classification**, per value.
Literal header values are lowered to compile-time-validated `static HeaderValue`s. Ran: all gates of
`scripts/verify.sh --msrv` pass (1.97.1 and 1.85), 5/5 mutants killed.

## 1. Model

A header value enters the native `HeaderMap` by exactly one of three routes, decided by the
declaration's syntax and the value's *trait impls* (never a type's spelling):

| Route | Declared as | Conversion | Validation | Payload cost per request |
|---|---|---|---|---|
| literal text | `"accept" = "application/json"` | macro emits `static __CONCORD_<Client>_HEADER_VALUE_n: HeaderValue = HeaderValue::from_static("…")` | compile time (macro check + `from_static` in a static initializer) | clone of static `Bytes`: 0 allocations |
| formatted text | `"x-r" = "r={region}"` | `Display` into a `String`, then the `String` impl | build time: visible ASCII, space, tab | `String` + copy |
| value placed directly | `header("k") x: T`, `"k" = x`, `"k" = each xs` | `<T as ToHeaderValue>::to_header_value(&value)` | by the impl: none for `HeaderValue` (already valid), visible ASCII for text | `HeaderValue`: refcount/none; text: 1 copy; integer: `http`'s `From` |

Sensitivity: `value.set_sensitive(true)` only when the static classification says so; never
`set_sensitive(false)`. The flag lives on each `HeaderValue` (Concord's clone, never the caller's).

**Is the effort observable?** For authors of native values: no — `accept: HeaderValue = &ACCEPT`
needs no keyword or wrapper; the diagnostics name the trait only when a type has no impl. For
custom/foreign `Display` types in a *direct* header placement: yes, a compile error with a migration
note (§5.9). At runtime the difference is observable and tested: wire bytes (raw-byte loopback),
flags (`is_sensitive`), payload reuse (pointer equality between the request's bytes and the
`static`'s), allocation counts (§7). The flag is also observable on the wire for HTTP/2: h2 0.4.20
encodes a sensitive value as an HPACK never-indexed literal (`h2/src/hpack/encoder.rs:146`,
`table.rs:231`) — reasoned from source, not run; the old `set_sensitive(declaration)` therefore
also changed HTTP/2 compression behaviour for a natively flagged value.

## 2. Baseline audit (a915c8d): every path that rejects or leaks a `HeaderValue`

Reasoned from source; lines are in the baseline export.

| Path | Effect on a `HeaderValue` |
|---|---|
| `__private.rs:277` `Parts::header<T: Display>` (direct placement, block `"k" = x`) | rejects at compile time (`HeaderValue: !Display`) |
| `__private.rs:314` `Parts::header_each` (`I::Item: Display`) | rejects `each` over native values |
| `__private.rs:284–295` `display` → visible-ASCII check → `HeaderValue::from_str` | if reached through a user `Display` wrapper: re-formats and re-parses; rejects H5 bytes (`caf\xe9`) |
| `__private.rs:297` `value.set_sensitive(sensitive)` | **clears** an incoming native flag when the declaration is not sensitive (H6 leak; also changes HTTP/2 HPACK indexing) |
| `emit.rs:569–578` → `__private.rs:467/499` `DebugDisplay`/`DebugEach` (`Display` bound) | generated `Debug` of config/client/scope/request rejects the type; with a wrapper, prints the value through `Display` **ignoring the native flag** (leak) |
| `emit.rs:1106` literal template passed as `&str` | literal re-checked and copied on every request (no leak; cost) |
| `emit.rs:1111` `format_template(format_args!(…))`, `__private.rs:130/140/247` `encode_segment`/`display_into`/`query` | cross-placement: rejects `HeaderValue` with a bare "doesn't implement `Display`" |
| `prepared.rs:101` `to_str().unwrap_or("<non-ASCII>")` | hides non-text native bytes (no leak; loses information) |
| `Error` `Display`/`Debug` | static text only — no header value can leak (unchanged) |

## 3. Coherence lab (ran, 1.97.1 and 1.85, identical outcomes)

`exp/coherence/` is a standalone workspace: `design_a`…`design_d` are mini "runtime" crates over
`http` 1.5; `user` depends on them **renamed** (`concord4 = { package = "design_a" }`,
`hyperium = { package = "http" }`, `rq = { package = "reqwest" }`) and contains code shaped like
`api!` output: owner state behind `Arc`, `let __o0 = &*self.__s;`, `&__o0.x`,
`if let Some(__v) = &__o0.x { …&*__v… }`, `each` over `&collection`. `HeaderValue` is reached as
`hyperium::HeaderValue`, `::hyperium::header::HeaderValue`, `rq::header::HeaderValue`,
`concord4::HeaderValue` and `type Hv = hyperium::HeaderValue`. Positive tests assert bytes and flags
for every field (`user/src/lib.rs` `mod tests`). `run.sh` checks every `neg_*` probe fails.

### A — native-conversion-first trait (chosen)

Positive (all borrowed from `&self` state, both toolchains): owned/static/aliased/qualified/renamed
`HeaderValue`, `Option<Hv>` through `if let`, `Vec<Hv>` and `Option<Hv>` with `each`, `caf\xe9`
bytes unchanged, native flag kept, `u32`/`u8`/`bool`/`String`/`&'static str`/`Box<str>`/`Arc<str>`/
`Cow<str>`, a local enum implementing the trait, a foreign `IpAddr` through a local newtype,
`Vec<u64>`/`Vec<&'static str>` with `each`. Integer literal: `take(&7)` with
`T: ToHeaderValue` compiles and selects the `i32` impl (fallback); `HeaderValue::from(7)` in an
initializer compiles (`http`'s `From<i32>`).

Negative probes (`out/1.97.1/*.txt`; 1.85 identical):

- `neg_display_blanket` — `impl<T: Display + ?Sized> ToHeaderValue for T` next to
  `impl ToHeaderValue for HeaderValue`:
  ```text
  error[E0119]: conflicting implementations of trait `ToHeaderValue` for type `HeaderValue`
     = note: upstream crates may add a new impl of trait `std::fmt::Display` for type `http::HeaderValue` in future versions
  ```
  So an unrestricted `Display` fallback is impossible next to the native impl, as the charter warned.
- `neg_display_blanket_str` — the same blanket next to `impl … for str`: E0119 outright (`str: Display`).
- `neg_from_blanket` — bridging to `http` with `impl<T> … where for<'a> HeaderValue: From<&'a T>`
  next to `impl … for str`: `E0119 … upstream crates may add a new impl of trait
  std::convert::From<&'a str> for type http::HeaderValue`.
- `neg_marker` — an opt-in marker (`impl<T: HeaderText> ToHeaderValue for T`, `HeaderText: Display`)
  is coherent with the native impls but **not** with `&T`/`Box<T>` forwarding:
  `E0119 … for type &_ … note: downstream crates may implement trait HeaderText for type &_`.
  Rejected (forwarding is needed for `&'static str` fields and `Vec<&str>` with `each`).
- `neg_a_foreign_display` (`IpAddr`): `` error[E0277]: `IpAddr` cannot be sent as a header value ``
  with Concord's note; `neg_a_orphan` (`impl ToHeaderValue for IpAddr` downstream): `E0117 only
  traits defined in the current crate can be implemented for types defined outside of the crate`
  — foreign types need a newtype or a template; `neg_a_option` (`Option<Hv>` scalar) and
  `neg_a_float`/`neg_a_literal_infer` (`f64`, `{float}`): the same custom E0277.
- `neg_a_query` (H10 shape): plain `` `HeaderValue` doesn't implement `std::fmt::Display` `` — the
  prototype adds a diagnostic trait for this (§5.8).

### B — reuse `http`'s `TryFrom<_> for HeaderValue` (reqwest's `RequestBuilder::header` bound)

Positive: native, text, `&'static str`, a local type via `impl From<&Sort> for HeaderValue`, flags
kept. Decisive negatives:
- `neg_b_int_ref` — `each` over `Vec<u64>` (items `&u64`):
  `` error[E0277]: the trait bound `HeaderValue: TryFrom<&u64>` is not satisfied `` (http converts
  integers by value only; `u8`, `i8`, `bool` have no conversion at all). The by-clone variant
  (`T: Clone, HeaderValue: TryFrom<T>`) fixes scalars but not unsized `str`.
- `neg_b_local_display`: `` the trait bound `for<'a> HeaderValue: TryFrom<&'a Region>` is not
  satisfied `` — no way to attach Concord's guidance (foreign trait).
- `neg_b_orphan`: E0117 for foreign types, as in A.
- Ran, positive test `design_b`: `"café"` as a `String` is **accepted** and sent as UTF-8 bytes
  (`TryFrom<&String>` = `from_bytes`, obs-text allowed): the text policy silently disappears and
  cannot be split by type without a Concord trait.

### C — autoref dispatch in generated code (`(&Probe(&v)).__header()`)

Positive on concrete types (native wins, else `Display`). Failure modes, ran:
- `neg_c_generic`: inside generic code the native impl is invisible:
  `` error[E0599]: the method `__header` exists for reference `&Probe<'_, T>`, but its trait bounds
  were not satisfied … `T: std::fmt::Display` `` (with a `T: Display` bound it would compile and
  silently take the text route).
- `neg_c_neither`: E0599 naming only the `Display` bound; method-resolution errors cannot carry
  `on_unimplemented` guidance.
- An integer literal `(&Probe(&7))` silently takes the `Display` route (`{integer}` is not concrete
  during method lookup).
Reasoned: it is specialization emulation (the charter excludes specialization shortcuts), the route
depends on inference state and on which traits are in scope, and generated `Debug` would need the
same trick. Rejected.

### D — syntax-directed `native` keyword (the strongest other viable design)

`header("accept") native accept: HeaderValue` → `header_native<T: Borrow<HeaderValue>>`; otherwise
today's `Display` route behind a blanket Concord trait. Ran: works (also `Box<Hv>`, `Arc<Hv>`), and
— notably — an `on_unimplemented` on a blanket-implemented trait **is** shown on both toolchains:
`` error[E0277]: `HeaderValue` is not formatted header text … note: for a native `HeaderValue`, write
`header("..") native x: HeaderValue` ``. Advantage: no break for `Display` header values. Cost
(reasoned): a keyword at every header placement *and* block reference (`"k" = native x`,
`each native xs`), a per-value consistency rule like `each`, a second Debug route keyed on syntax,
and permanent ceremony that restates what the type already determines (a `HeaderValue` can never
take the text route). Not chosen; its diagnostic technique was adopted for §5.8.

## 4. Why A over D (the decisive trade)

A puts the decision where Rust puts it — a trait impl on the type — and needs no grammar. The only
cost is a compile-time break for direct header placements of types that are neither native, text,
integer nor `bool` (H9), with a note naming two explicit fixes. In the repository and its docs the
census of direct header value types was `String` ×15, `u32` ×1, a generic `T` ×1 and `Faulty` ×1
(the deliberate `Display`-failure fixture), so the break hits one test (§6). D would keep those
compiling but tax every native placement forever. A also keeps the visible-ASCII policy for text
(B cannot), gives integers/`bool` a non-formatting route, and lets domain enums produce
`from_static` values with no allocation (better than their old `Display`).

## 5. Recommended design (implemented)

### 5.1 DSL surface

Unchanged grammar. Examples that now work (all in `crates/concord/tests/native_headers.rs`):

```rust
static ACCEPT_JSON: HeaderValue = HeaderValue::from_static("application/json");
concord::api! {
    pub client Native(
        header("x-owned") owned: HeaderValue,                       // H1 (config + setter)
        header("accept") accept: Hv = ACCEPT_JSON.clone(),          // H2 (alias, static)
        header("x-opt") opt?: concord::reqwest::header::HeaderValue, // H3 (qualified)
        header("x-init") init?: NativeValue = HeaderValue::from_static("init"),
        header("x-list") each list: Vec<HeaderValue> = Vec::new(),  // H4
        header("x-n") n: u32 = 7,                                   // H9 (integer)
    ) { … }
}
api.per_call(&ACCEPT_JSON)            // borrowed route: `&HeaderValue` added to `IntoValue`
```

`IntoValue<HeaderValue>` now also accepts `&HeaderValue` (cloned), mirroring `&String → String`, so
a `static` is passed or used as an initializer without `.clone()` (`= &ACCEPT_JSON`).

### 5.2 Trait (`crates/concord/src/header.rs`, 131 lines incl. docs)

```rust
#[diagnostic::on_unimplemented(message = "`{Self}` cannot be sent as a header value", …)]
pub trait ToHeaderValue { fn to_header_value(&self) -> Option<HeaderValue>; }
```

Open (domain types implement it; it composes: `self.0.join(",").to_header_value()` reuses the text
rule). `Option` rather than an error type: Concord's build errors are static text, users produce
`None` from `HeaderValue::from_str(..).ok()`, and no public error type is added. Integers use
`http`'s `From` (via `u16`/`i16` for `u8`/`i8`). Not implemented: `u128`/`i128`, floats, `char`
(no native conversion; a template keeps their `Display`).

### 5.3 Generated code (ran: `exp/compile/expanded-small.rs`, nightly `-Zunpretty=expanded`)

```rust
static __CONCORD_Shop_HEADER_VALUE_0: ::concord::reqwest::header::HeaderValue =
    ::concord::reqwest::header::HeaderValue::from_static("concord");
…
__p.header(Clone::clone(&__CONCORD_Shop_HEADER_0), "header `x-key`", &__o0.key, true)?;
__p.header(Clone::clone(&__CONCORD_Shop_HEADER_1), "header `accept`", &__o0.accept, false)?;
__p.header(Clone::clone(&__CONCORD_Shop_HEADER_2), "header `x-client`", &__CONCORD_Shop_HEADER_VALUE_0, false)?;
__p.header_each(Clone::clone(&__CONCORD_Shop_HEADER_3), "header `x-tag`", &self.tags, false)?;
if let Some(__v) = &self.trace { __p.header(…, "header `x-trace`", &*__v, false)?; }
__p.header(…, "header `x-region`", &format_template(format_args!("r={0}",
    ::concord::__private::text(&self.region)), &__META, "header `x-region`")?, false)?;
// Debug:
__d.field("accept", &::concord::__private::DebugHeader(&__s.accept));
__d.field("tags", &::concord::__private::DebugHeaderEach(&self.tags));
__d.field("region", &::concord::__private::DebugDisplay(&self.region));
// The initializer stays in its function, evaluated in `new`:
fn __concord_default_accept() -> HeaderValue { <_ as IntoValue<HeaderValue>>::into_value(&ACCEPT_JSON) }
```

Runtime (`__private.rs`): `Parts::header<T: ToHeaderValue + ?Sized>` → `to_header_value()` →
`None` is a build error (`header `x` value is not a valid header value (text must be visible ASCII,
space or tab)`) → `if sensitive { set_sensitive(true) }` → `try_append`. Macro: `Value::text` (set
when a value is read by a path/subdomain/query placement or a template hole) selects
`DebugDisplay`/`DebugEach` vs `DebugHeader`/`DebugHeaderEach`.

### 5.4 Sensitivity rule

- Outgoing flag = **native flag of that value OR the key's static classification on that chain**
  (marker, sticky composition, safety-list name — all unchanged, compile time).
- Per value, no runtime propagation: each `each` item keeps its own flag; an `append`ed value does
  not flag its siblings; `override`/`remove` drop the inherited value from the request (nothing left
  to protect); `override … else inherit` with an absent value sends the inherited value with its own
  flag; exact replacement (`remove` + placement) sends only the new value with its flag. The static
  classification is still sticky (a native value feeding a key that is sensitive on any chain is
  flagged on every chain — `declaration_classification_is_added`).
- Concord flags its own clone; the caller's value and `static`s are never mutated.
- Direct forwarding vs extraction: a value that the application converts itself (e.g.
  `hv.to_str()?.to_owned()` into a `String` value) is new data whose protection is the
  declaration's (`#[sensitive]`, key name), not the original flag — documented in `v4/SEMANTICS.md` §9.
- This is the smallest rule that satisfies "never clear, always add"; a key-level runtime taint
  (flagging all values of a key when one is natively flagged) was considered and not adopted: the
  flag is per value in `http`/h2, and Concord renders per value.

### 5.5 Debug rendering (ran: `generated_debug_of_every_owner`, H5–H7 tests)

| Value | Owner `Debug` (config/client/scope/request) | `Prepared` `Debug` |
|---|---|---|
| marked or statically sensitive | `<redacted>` (unchanged, no conversion runs) | `name: <redacted>` |
| native, flagged, unmarked | `<redacted>` | `name: <redacted>` |
| native, text bytes | `"application/json"` | `name: application/json` |
| native, non-UTF-8 (`caf\xe9`) | `"caf\xe9"` (escaped by `HeaderValue`'s `Debug`) | `name: "caf\xe9"` (was `<non-ASCII>`) |
| `each` of native | `[<redacted>, "text/plain"]` per item | one line per value |
| integer/`bool`/`String` header-only | `"7"`, `"true"`, `"text"` (same as before) | unchanged |
| `String` header-only that is not valid | `<invalid header value>` (was its `Display` text) | — (build error) |
| value also read as text (query/path/template) | `Display`, unchanged | unchanged |

### 5.6 Literal lowering

Only a header template **without holes** is lowered (one `static` per distinct decoded literal,
`{{`→`{` already applied), deduplicated per declaration (linear in distinct literals, like names).
The macro still checks literal characters first (better diagnostic than `from_static`'s compile-time
panic). Query literals, templates with holes, initializers and arbitrary expressions are never
evaluated or hoisted: initializers remain in `__concord_default_*` functions evaluated per owner
creation (ran: `initializers_run_per_client` with a side-effecting initializer; generated code
above). A literal under a `#[cfg(any())]` endpoint still yields a valid, unused static (ran under
`#![deny(warnings)]`).

### 5.7 Visible-ASCII policy: split

- **Text Concord produces or receives as Rust strings** (literals, templates, `str`/`String`):
  keep visible ASCII, space, tab. Product reason: a Rust string is Unicode; `HeaderValue::from_str`
  would send its UTF-8 bytes as obs-text, which RFC 9110 §5.5 deprecates and receivers commonly
  decode as ISO-8859-1 (mojibake) or reject — almost always a bug (`"café"` in a header), caught
  at build time with an error naming the header. Lab B shows the alternative silently sends it.
- **Native values**: no Concord restriction beyond `http`'s own (`0x20–0xFF` except `0x7F`, tab).
  The application chose the bytes explicitly (`from_bytes`), so they are sent unchanged (H5).
- Migration text (docs): "Text header values must be visible ASCII, space or tab. To send other
  bytes deliberately (for example Latin-1 for a legacy server), build a `HeaderValue` with
  `HeaderValue::from_bytes` and declare the value as `HeaderValue`."

### 5.8 Cross-placement (H10)

A hidden `__private::Text: Display` trait (blanket impl, `on_unimplemented`) now bounds
`query`/`query_each`/`encode_segment`/`display_into`/`DebugDisplay`/`DebugEach`, and template holes
pass through `__private::text(&v)`. Ran (`ui/native_header_cross_placement.stderr`):

```text
error[E0277]: `HeaderValue` is written as text here and has no `Display`
 --> tests/ui/native_header_cross_placement.rs:8:38
8 |         GET query(query("accept") a: HeaderValue);
  |                                      ^^^^^^^^^^^ used in a path, subdomain, query value or template
  = note: path segments, subdomain labels, query values and template holes are written with `Display`; …
  = note: a native `HeaderValue` is not text: it can only be placed as a whole header value; to reuse
          its text elsewhere, convert it explicitly (for example `to_str()`) into a `String` value
```

It points at the value's type in the declaration. The path and template cases carry the same
message; their primary span covers the macro invocation with the type labelled (the path case did
so before this change); each case is followed by the existing duplicate for the generated `Debug`
and, for templates, `format_args`'s own error. No implicit decoding exists anywhere.

### 5.9 Migration for numeric and custom-`Display` header values (H9)

- `String`, `&str`, integers `u8`–`u64`/`usize`/`i8`–`i64`/`isize`, `bool`: unchanged spelling,
  now without `Display`.
- Any other type in a *direct* header placement: compile error ``"`T` cannot be sent as a header
  value"`` with the note "…implement `concord::ToHeaderValue` for your own type, or send its
  `Display` text with a template: `headers { "name" = "{value}"; }`". Before/after is in
  `docs/MIGRATION.md` ("Header values: `ToHeaderValue`, not `Display`", compiled as a doctest).
- Templates, paths, labels and query values are unchanged (`Display`).

## 6. Prototype

Code: runtime `header.rs` (new, 131 lines with docs/doctest), `__private.rs` +86/−19, `lib.rs`
+12/−6, `prepared.rs` +6/−7, `value.rs` +16/−2; macro `emit.rs` +70/−22, `model.rs` +12. Docs:
`v4/SEMANTICS.md` §5/§9, `docs/AUTHOR_GUIDE.md` §4.1/§4.8/§6/§7 (new compiled example),
`docs/MIGRATION.md`, `docs/DIAGNOSTICS.md`, `docs/CALLER_GUIDE.md` §8, `docs/MAINTAINERS.md`,
`v4/TEST_MATRIX.md` (rows H1–H14, status "prototype"). The body/response `via` DSL is untouched.
`Cargo.lock` gains one dependency line (`examples/renamed` → `reqwest`, renamed; no new package).

Tests added (ran):
- `crates/concord/tests/native_headers.rs` (17 tests, `#![deny(warnings)]`): H1 config/setter/
  snapshots; H2 static as initializer, per call (cloned and `&STATIC`), setter, payload pointer
  equality, static not mutated; H3 optional/initialized/`maybe_*(None)`; H4 order + per-item flags
  + `each` over `Option`; H5 raw bytes on the wire (the lossy text view shows `caf\u{fffd}`); H6
  flagged/unmarked/unlisted withheld from request `Debug`, `Prepared` `Debug` and reqwest's `Debug`,
  with positive control; H6 OR with marker; sticky static classification added to an unflagged
  value; H7 `append`/`override` (public and flagged)/`remove`/`else inherit` (both branches)/exact
  replacement with flags per value and positive control; generated `Debug` of config, client,
  scope, request; empty vs absent (raw bytes); H9 `u8`/`i64::MIN`/`bool`/text/domain type/`each` of
  domain type/`Display` via template, CR-LF/`é`/DEL rejected without echo; `&'static str`/`Cow`/
  `Arc<str>`/`Box<HeaderValue>`; native `Content-Type` beats the representation's; optional native
  header independent of body presence (4 combinations); full header map with native values;
  `#[cfg(any())]` endpoint with a nonexistent native type and a literal; initializer counting.
- `tests/support/mod.rs`: records `raw_headers` (bytes as received) + `header_bytes_all`.
- `ui/header_value_type.rs` (H9: `IpAddr`, local `Display` type, `Option<String>`, `f64`),
  `ui/native_header_cross_placement.rs` (H10: query, path template, header template).
- `examples/renamed`: `Headers` client through renamed `concord4` and renamed `reqwest013`
  dependencies, an alias and a qualified path; flags and `Debug` asserted.
- `checks/heap/tests/headers.rs`: allocation counts per route with assertions (§7).
- `display_errors.rs`: the header case now uses a template (`"x-v" = "{v}"`) — same assertion
  (build error naming `header `x-v``, no partial text); a direct header placement no longer formats.

UI `.stderr` re-blessed, each intentional:
- `conversions_are_closed.stderr`, `newtype_is_not_converted.stderr`: `IntoValue` note now lists
  `&HeaderValue`; rustc's impl list shows the third impl (and its wording for the list changed).
- `option_has_no_wire_spelling.stderr`, `vec_needs_each.stderr`: first error is Concord's `Text`
  message with the `?`/`each` note instead of the bare `Display` error (DIAGNOSTICS.md excerpt
  updated; checked verbatim by `doc_excerpts`).

Gates (ran, `exp/verify-msrv.txt`): fmt, clippy `-D warnings`, test (465 passed), no-default (367),
json-only (383), reqwest-defaults-only (369), 3× rustdoc `-D warnings`, msrv-concord on 1.85 (385,
includes `native_headers`, added to `scripts/verify.sh`), msrv-examples on 1.85 (73) — all PASS. The
script's last line ("tracked files changed") fails only because the prototype is uncommitted
(tree dirty before the run). Baseline at a915c8d: 443 tests.

Mutations (ran, `exp/mutants/result.txt`): baseline pass → each mutant fails → restored pass:
old `set_sensitive(declaration)` (killed by H4/H6/H7 and `forwarded_forms`), owner `Debug` ignoring the flag (generated
Debug, H6), obs-text accepted for text (H9, Debug), literal not lowered (pointer test), `Prepared`
printing sensitive values (H6, H7). 5/5 killed.

## 7. Allocation honesty (ran: `checks/heap/tests/headers.rs`, counting global allocator)

Allocations per `prepare()` (+drop), minimum of 50 after warm-up, reused client; every row sends
the same four header names, so the header map costs the same in all rows.

| Route (4 values) | a915c8d | prototype | payload allocations per value (prototype) |
|---|---|---|---|
| no headers | 1 | 1 | — |
| header map for 4 names | +2 (656 B) | +2 (656 B) | — (`HeaderMap` indices + entries; names cloned from statics: 0) |
| native `from_static` | n/a | 3 | 0 |
| native shared (`from_bytes`) | n/a | 3 | 0 (refcount) |
| literal template | 11 | 3 | 0 (lowered static) |
| `u64` | 11 | 11 | 2 (`http`'s `From<u64>`: `BytesMut`, then a shrink in `freeze`) |
| `String` | 11 | 7 | 1 (copy into the `HeaderValue`) |
| formatted template | 15 | 11 | 2 (`String` + copy; was 3) |

Payload reuse is separate from map cost: native/literal payloads are reused (pointer equality is
also asserted in `h1_h4_client_values_on_the_wire`), the map still allocates. Not zero-allocation
overall: the request URL and map always allocate. Reasoned (bytes 1.12 `bytes.rs:1184/1486`): the
first clone of a `from_bytes` value allocates one `Shared` control block (promotion) — excluded by
the warm-up. Integers could drop to 1 allocation with a stack buffer + `from_bytes`; not done
(`http`'s conversion is the native one; no regression vs baseline).

## 8. Expansion size and `cargo check` cost (rough)

Generated declaration (`exp/compile/gen.py`): 400 endpoints in 40 scopes, ~10 header contributions
per endpoint (direct, `each`, optional ref, literals, `append`, `override`). Ran 1.97.1, hyperfine,
deps warm, 20 cores, noise ±1–2 %.

| Variant | expanded bytes (nightly `-Zunpretty=expanded`) | `cargo check` 400 endpoints | 50 endpoints |
|---|---|---|---|
| a915c8d, `String` values | 2,786,245 | 674.7 ± 10.4 ms | 140.0 ± 4.0 ms |
| prototype, `String` values | 2,868,592 (+3.0 %) | 685.1 ± 13.7 ms (+1.5 %) | 141.7 ± 4.2 ms |
| prototype, `HeaderValue` values | 2,900,864 (+4.1 %) | 682.3 ± 6.5 ms (+1.1 %) | 138.2 ± 3.2 ms |

Differences are within about two standard deviations; the expansion grows by the literal statics
and `__private::text(..)` wrappers.

## 9. Open risks

- **Break for direct `Display` header placements** (custom enums, `Uuid`, `IpAddr`, floats,
  `u128`, `char`). Compile-time, with a note and a documented two-line migration; still a break
  downstream users will see.
- **Generated `Debug` of a header-only value calls `to_header_value()`** (a clone/allocation; for a
  user impl, arbitrary code). An invalid `String` now shows `<invalid header value>` instead of its
  text. Debug is not hot; a panicking user impl would panic in `Debug`.
- **Per-value rule is deliberately minimal**: an unflagged sibling appended to a natively flagged
  value is shown. Applications wanting key-level protection mark the value or key (`#[sensitive]`).
- **`Text` diagnostic trait**: the Option/Vec errors now carry a `HeaderValue` note that is
  irrelevant to them; path/template primary spans cover the macro invocation (rustc span choice,
  pre-existing for paths).
- `from_static` in generated statics requires `http`'s `const fn from_static` (true since `http` 1.0;
  reqwest 0.13 depends on `http` 1.x).
- `Encode::CONTENT_TYPE` is still parsed per request (body track's area; could become a static).
- Integer conversion cost is `http`'s (2 allocations per value).
- Not run: macOS/Windows; HTTP/2 HPACK effect of the flag (reasoned from h2 source only).
