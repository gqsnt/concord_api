# Track C — Core/boundary specialist report (D11, D12, D13, D15, R12/U30/U31)

## Source identity and model

- Snapshot: `snapshots/core-8f7adc2` (SNAP). `.concord-review.json` records `original_commit`
  `8f7adc2019f3ee31af087c69286d559bca1c173a`, tree `26e6a14b…`; SNAP HEAD `6d05554` ("Review
  snapshot of 8f7adc2…"). Verified before work. rustc/cargo 1.97.1 (pinned toolchain). Locked
  natives confirmed with `cargo tree -i`: http 1.5.0, reqwest 0.13.5, url 2.5.8, hyper 1.12.0
  (also hyper-util 0.1.21, idna 1.1.0, idna_adapter 1.2.1, ICU4X 2.1.x).
- Model: Claude Opus 5.5 (`claude-opus-5-5`). Requested effort: xhigh. Effort not observable from
  inside the session.
- Interrupted once by the host agent restart (~00:43); all builds were re-run afterwards. The run
  in flight at the restart was the 24,577-header real-macro compile (see D12).
- Writes: SNAP gained three untracked test files only (`crates/concord/tests/{header_capacity,
  fmt_error,d15_surfaces}.rs`); SNAP `src/` is unchanged (a mutation of `error.rs` was applied and
  reverted with `git checkout`). Everything else is under EXP =
  `exp/core`: a scratch workspace (`Cargo.toml`, lock copied from SNAP), two `git worktree`s of
  SNAP (`snap-b` = D11 option (b), `snap-c` = D12/D13 prototypes; registered in SNAP's `.git`),
  and standalone crates `d11/consumer_{control,b}`, `d11/static_check_b`, `d12_macro`.
- Evidence labels: **[ran]** = command executed with the quoted result; **[reasoned]** = argument
  from source reading only. Timings are **noisy (concurrent builds on the host)**; no benchmark
  was attempted.

## Recommendations at a glance

| ID | Recommendation | Smallest implementation |
|---|---|---|
| D11 base validation | **KEEP** the strict static subset (a); REJECT (b) url-in-proc-macro and (c) fallible `new` | Add a subset ⇒ `Base::parse` regression test in `concord-macros` with `url` as a **dev**-dependency (no consumer cost); document the runtime-only classes and the subdomain+IP-base loopback recipe (`ClientBuilder::resolve`) |
| D12 headers | No ordinary-data panic exists. **CHANGE (tiny)**: `try_append`/`try_insert` in `Parts` (2 sites) so Concord code never panics at capacity; **no** static limit; document the native residual | 13-line diff, prototyped and tested in `snap-c` |
| D12 side | **CHANGE (small, defect)**: handle `fmt::Error` uniformly as a build error | `encode_segment`/`display_into`/`query`/`header` return `Result`; generated `__p.query(..)?` |
| D13 native/typed access | **KEEP** `into_parts` as the one native route; **REJECT** `Prepared::request_mut` (leak shown); **CHANGE (small)**: `BodyError: Display + Error` (no source) and `Error::into_response(self) -> Result<Response, Error>`; **optional** `Prepared::decoder(&self)` | 4 small hunks in `snap-c`, full suite passes unchanged |
| D15 surfaces | **KEEP** the boundary (no `source()`); fix test evidence: ADV-1 has no positive control; add S03 remove/reintroduce, S07, S09, JSON-encode cases (provided) | Move `crates/concord/tests/d15_surfaces.rs` into the suite (TLS-gated) |
| R12/U30/U31 | **No core change needed** — all four compositions compile and run on loopback | `downstream/tests/r12.rs` |

---

## D11 / L3 — Base validation (U13/U33, C04/C05/C13)

### Problem and smallest counterexample
Declared bases are checked by a hand-written subset so `Base::from_static` (which panics) is
unreachable; runtime `with_base_url` uses `url::Url` via `Base::parse`. The question is whether the
two disagree in a way that panics, changes meaning, or blocks real users.
Smallest counterexamples: `base "http://bücher.example"` (compile error; runtime OK →
`xn--bcher-kva.example`), `base "http://example.test/a%20b"` (compile error; runtime OK).

### Control behavior — evidence [ran]
Method: every corpus input was compiled through the **real `api!` macro** (one module per line,
`cargo check --message-format=json`, diagnostics mapped back to inputs), and the accepted ones were
constructed and prepared at runtime under `catch_unwind`; every input was also passed to
`with_base_url`. A verbatim copy of `validate_base` (only `LitStr`/`syn::Error` plumbing replaced;
cross-checked against the real macro on all 98 inputs: 0 disagreements) drove a 300,000-input
randomized differential search against `concord::__private::Base::parse`/`from_static`.

| Result | Corpus (98 inputs, 12 groups) | Random search (300k) |
|---|---|---|
| static accepts and runtime rejects | **0** | **0** |
| `from_static` panics after macro accepted | **0** | **0** |
| same meaning when both accept | 24/24 identical (`from_static` *is* `parse`) | — |
| both reject | 20 | — |
| runtime only | 54 | 100,950 |

Full table: `exp/core/d11/probe_output.md`. Canonicalization on shared inputs (same in both
because the same parser runs): host lowercased (`EXAMPLE.Test`→`example.test`), default ports
dropped (`:80`, `:443`, `[::1]:80`), leading-zero ports (`:08080`→`:8080`), IPv6 compressed
(`[0:0:0:0:0:0:0:1]`→`[::1]`, `[::FFFF:127.0.0.1]`→`[::ffff:7f00:1]`), empty path → `/`.

Runtime-only classes (static rejects, `with_base_url` accepts), by U33 category:

| Category | Examples (input → runtime result) |
|---|---|
| uppercase scheme | `HTTP://example.test` → ok |
| trailing/leading dots | `example.test.` (kept, FQDN), `example.test..`, `.example.test`, `127.0.0.1.` → `127.0.0.1` |
| empty port | `example.test:` → no port |
| non-dotted-quad IPv4 | `127.1`, `2130706433`, `0x7f.0.0.1` → `127.0.0.1`; `01.02.03.04` → `1.2.3.4`; `1.2.3` → `1.2.0.3` |
| IDNA | `bücher`, `xn--bcher-kva`, `XN--BCHER-KVA`, `faß` → `xn--fa-hia`, fullwidth `ｅｘａｍｐｌｅ` → `example`, `a。b` → `a.b`, `例え.テスト` |
| host characters | `x_y`, `-lead`, `trail-`, `ex%41mple` → `example`, `exa\nmple` → `example` |
| DNS lengths | 64-byte label, 254-byte host (url does not enforce DNS lengths) |
| escaped/Unicode paths | `%20`, `%2F`, `caf%C3%A9`, `café` → `caf%C3%A9`, `a b` → `a%20b`, malformed `%zz`, bare `%` |
| dot segments | `/x/%2e%2e/admin` → **`/admin`**, `/x/%2E/y` → `/x/y`, `/a/./b`, `/a/../b`, `/..` |
| non-pchar path chars | `\` → `/`, `{`→`%7B`, `"`, `` ` `` encoded; `|`, `^`, `[x]` kept raw; `\t` removed |
| structure | leading/trailing space stripped, `http:\\example.test`, `http:example.test`, `http:///example.test` → `http://example.test/`, empty userinfo `http://@example.test` / `http://:@…` silently dropped |

Failure stage [ran] (`downstream/tests/d11_send_stage.rs`, loopback): every runtime-only form
above that was sent (`[x]`, `|`, `^`, `%zz`, `%`, `caf%C3%A9`, `%2e%2e`, `x_y`, `-lead`,
`example.test.`, `bücher`, 64-byte label) is **sent without error**; the server sees e.g.
`/[x]/x`, `/a%/x`, `/admin/x`, `Host: xn--bcher-kva.example:PORT`. There is no send-time failure
stage for bases: a base is either rejected by `with_base_url` or sent as canonicalized.

Fresh-use trial (coordinator note) folded in: `with_base_url` accepts an IP base, but every endpoint
with `subdomain` labels then fails at **prepare** with "subdomain requires a base URL with a domain
name". The participant's working route was a DNS-named base plus
`reqwest::ClientBuilder::resolve("eu.catalog.test", addr)` — exactly what `downstream/src/lib.rs`
`catalog_for` does for all D13/R12 tests. `eu.localhost` is not a portable alternative (on this
host it resolves to `::1` only).

### Alternative (b): proc macro depends on `url` and runs exactly `Base::parse`'s rules [ran]
Prototype: `exp/core/snap-b` (diff `exp/core/d11/option_b_url_macro.diff`: `url.workspace = true`
in `concord-macros`, `validate_base` replaced by the six `Base::parse` checks; 18 insertions,
105 deletions).

- Behavior: the same 98-input corpus through the (b) macro → rejects exactly the 20 "both reject"
  inputs and accepts the other 78, i.e. identical to runtime. Diagnostics carry url's static
  `ParseError` text, e.g. `invalid base URL "http://xn--a.example": it is not an absolute URL
  (invalid international domain name)`.
- Dependency footprint (exact, `cargo tree -p concord-macros -e normal`): **5 → 34 crates (+29)**:
  url, form_urlencoded, percent-encoding, idna, idna_adapter, utf8_iter, smallvec, the ICU4X 2.1
  stack (icu_collections, icu_locale_core, icu_normalizer(+_data), icu_properties(+_data),
  icu_provider, litemap, potential_utf, tinystr, writeable, yoke, zerofrom, zerotrie, zerovec,
  utf_types, stable_deref_trait) and five derive proc-macros (displaydoc, synstructure,
  yoke-derive, zerofrom-derive, zerovec-derive; syn 2 alongside the macro's syn 3). All 29 are
  already in the runtime graph, but on the host side they become extra units.
- Clean consumer build units (one `api!`, `exp/core/d11/consumer_{control,b}`, rustc invocations
  from `cargo -v`): `cargo check` **120 → 144 (+24)** — the host must fully compile url/idna/ICU
  that the target side only checks; `cargo build` **119 → 126 (+7)** (most ICU units are shared
  host/target there). `--timings` sum of the 25 extra check units: ≈3.9 s CPU; wall clock
  10.99 s vs 10.43 s check, 14.4 s vs 15.0 s build — **noisy, concurrent builds; no measurable wall
  difference at this size**.
- Other costs [reasoned]: the macro's accepted set then depends on the application's
  `idna_adapter` choice (unified in the lockfile, so host and target still agree); the declaration
  would accept forms that url silently rewrites (`http:\\x`, `exa\tmple`, `/a/../b`, `%2e%2e`,
  empty userinfo), so source text would no longer say what is sent unless a further "must already
  be canonical" rule is added.

### Alternative (c): fallible generated constructor [ran]
Hand-written equivalent (`downstream/src/d11_fallible.rs`, test `downstream/tests/d11_fallible.rs`):
`Idn::new(http) -> Result<Self, concord::Error>` parses the declared base at runtime. It accepts
`bücher.example` (`http://xn--bcher-kva.example/v1/ping`), but every caller and every application
constructor must now propagate an error only a wrong declaration can produce
(`App::new(..) -> Result<Self, concord::Error> { Ok(Self { api: Idn::new(http)? }) }`), and a
wrong declaration (`http://xn--a.example`) moves from a compile error to a runtime `Build` error
with `endpoint() == None` on every construction.

### Trade-offs
| | (a) subset (control) | (b) url in macro | (c) fallible `new` |
|---|---|---|---|
| static ⇒ runtime, no panic | yes (corpus + 300k random) | yes by construction | n/a |
| accepted declared set | strict, what-you-write-is-sent (modulo case/default port) | everything `with_base_url` accepts, incl. silent rewrites | everything |
| IDN / escaped base paths | runtime only | yes | yes |
| proc-macro deps | 5 | 34 | 5 |
| diagnostics | precise custom messages at the literal | url's static texts | none at compile time |
| caller ergonomics | infallible `new` | infallible `new` | `?` everywhere |
| docs burden | must list runtime-only classes | "same as with_base_url" | must explain why `new` fails |

### Recommendation: **KEEP (a)**
Smallest implementation:
1. Regression: a test in `concord-macros` with `url` as a **dev-dependency** (no consumer impact)
   asserting subset-accepted ⇒ the six `Base::parse` checks pass, over the corpus and a seeded
   random generator (`exp/core/d11_probe` is a ready template; it found 0 violations in 300k).
2. SEMANTICS §4 / `with_base_url` rustdoc: "Runtime bases follow WHATWG URL parsing (`url`
   crate) including its rewrites: IDNA, `\`→`/`, tab/newline removal, `%2e%2e` resolution,
   shorthand IPv4, empty userinfo dropped. Declared bases are a strict subset that parses without
   such rewriting." Generated `with_base_url` docs for clients with `subdomain` should say that
   labelled endpoints need a domain-name base and give the `ClientBuilder::resolve` loopback recipe.
3. Optional diagnostic polish [reasoned]: `Base::parse` collapses all url errors into "is not an
   absolute URL"; url's `ParseError` variants have static, content-free texts ("invalid port
   number", "invalid international domain name"), which could be mapped into the `&'static str`
   reason.

Intentionally unsupported: declaring an IDN host, an `xn--` host, a percent-escaped or Unicode base
path, an underscore host or a trailing-dot FQDN statically. Route: declare any subset base and
call `with_base_url` (fallible) — the declared base then acts as a placeholder.

---

## D12 — Header construction beyond L1 (C06/C07/C15)

(L1 literal-name validation and L2 wire order are the coordinator's and are not covered here.)

### Native facts [reasoned from http 1.5.0 source, confirmed by tests]
`HeaderMap` index slots ≤ `MAX_SIZE` = 2^15 → usable capacity **24,576 distinct names**. Extra
values of one name live in an unbounded `Vec<ExtraValue>` with `usize` links (memory-bound
only). `try_append2`/`try_entry2` call `try_reserve_one()` **before** the vacant/occupied check,
so a map holding 24,576 names panics on *any* further `append`/`insert`/`entry`, even for an
existing name.

### Control behavior — reproduced in SNAP [ran]
`crates/concord/tests/header_capacity.rs` (7 tests pass, 1 ignored probe) drives the exact helpers
generated code calls (`concord::__private::Parts`):

| Case | Result |
|---|---|
| 200,000 values of one declared name via `header("x-tag") each` | builds; `keys_len()==1`; with a body the content-type insert is fine |
| 20,000 values of one name on the wire (loopback) | all received in order |
| 24,577th distinct name in `Parts::header` | **panic** "size overflows MAX_SIZE" (`HeaderMap::append`) |
| 24,576 names, then append to an existing name | **panic** |
| 24,576 names, then body content-type `insert` in `Parts::finish` | **panic** |
| 24,575 names, `send()` | **panic in native code** at send: `hyper-util 0.1.21 client.rs:300` `entry(HOST)`, after reqwest merged `accept` (probe: 24,573 and 24,574 send fine) |
| 500 distinct names, send | fine |

Reachability: distinct names are literal declaration keys, so only a declaration with ≥ 24,575
distinct header names on one endpoint chain can get there; caller data cannot add names.
Real-macro attempt (`exp/core/d12_macro`, `MEASUREMENTS.md`): compiling one owner with N distinct
literal headers needs 0.36 GB RSS @2,000, 1.46 GB @6,000, **4.70 GB @12,000** (12.8 s, noisy);
extrapolated ≥ 16 GB for 24,577. The full-size attempt was not completed (an unbounded attempt
coincided with the host restart; a `ulimit -v 16 GB` retry aborted with "memory allocation
failed"). So: **no ordinary-data panic is reachable**; the capacity panic is unreachable in
practice through the macro.

### Alternative: fallible native operations [ran]
`snap-c` prototype: `Parts::header` uses `try_append`, `finish` uses `try_insert`, mapping
`MaxSizeReached` to `Error::build(meta, name, "exceeds the header map capacity")` (13 changed
lines). `tests/header_capacity_try.rs` (adapted copy): the Concord-side cases now return
`GET Cap.many: request not built: x-h24576 exceeds the header map capacity`; the native send
case still panics (hyper-util) — no Concord-side change can prevent that, and the application's
own `default_headers` lower the native threshold. Whole `snap-c` suite passes. Runtime cost: none
[reasoned: `append` is `try_append(..).expect(..)`].

A static macro limit was considered and not recommended: it would hard-code a dependency-derived
number into the DSL ("do not hardcode a dependency limit…"), and the compiler already cannot
build such declarations.

### Recommendation: **KEEP** semantics, **CHANGE (tiny)** to `try_append`/`try_insert`
Document in SEMANTICS §5: "One name may repeat without limit (memory only). A request carries at
most ~24,500 distinct header names (http's HeaderMap); native layers add their own headers at
send time." Unsupported counterexample: a declaration with tens of thousands of distinct names
(not compilable in practice; native send would still panic).

### Side finding (defect, low): `fmt::Error` handling differs by placement [ran]
`crates/concord/tests/fmt_error.rs` (5 tests pin current behavior) with a `Display` that writes
`abc` then returns `Err`: path segment → URL `…/items/abc` built and sendable (partial text, error
ignored by `encode_segment`); host label → `abc.f.example.test` (same via `display_into`); query
and header → **panic** (`ToString::to_string`); generated `Debug` → panic (`DebugDisplay`). Such a
`Display` violates the `fmt` contract (user code), but the silent partial path/host can address a
different resource. Smallest fix [reasoned]: helpers write through `fmt::write` and return
`Err(Error::build(meta, what, "could not be formatted"))` for every placement (generated query
calls gain `?`); generated `Debug` writes `<unformattable>`.

---

## D13 — Native and typed response access (U14/U15/U16/U28/U29)

All downstream code is in `exp/core/downstream` (separate crate, real macro, the USER_TASKS
"regional catalog" fixture with a region label resolved to loopback, tenant scope, optional
category, default page size, repeated tag, sensitive authorization header). The crate's 17 tests
(D11 2, D13 11, R12 4) pass [ran].

### U14 — unread non-2xx + error document [ran]
`tests/d13.rs::update_name` (control): get the error, read `status()` **before**
`into_response()` (it consumes the error), `.expect()` the `Option`, `bytes()`, then
`<Json<ApiError> as Decode>::decode(bytes)` — the public `Decode` trait already is the "decode
bytes with a representation regardless of status" route. Friction, captured as rustc output in
`exp/core/d13_compile_fail/rustc_output.txt`:
- `E0277 ` `?` couldn't convert the error: `BodyError: StdError` is not satisfied`
- `E0277 `BodyError` doesn't implement `std::fmt::Display``
- `E0382 borrow of moved value: e` (status after `into_response`)
- `into_response()` on a non-status error returns `None` and drops the error.
- `response.json()` needs reqwest's `json` feature, which `reqwest-defaults` does not enable
  (matches the fresh-use trial: they used `bytes()` + serde_json).

With the `snap-c` prototypes (`tests/d13_prototype.rs::error_code`) the same task is:
```rust
let response = error.into_response()?;          // Result<Response, Error>: other errors propagate
let status = response.status();
let doc = <Json<ApiError> as Decode>::decode(response.bytes().await?)?;  // BodyError: Error
```

### U15 — inspect, change timeout + header, send, reuse decoder [ran]
`tests/d13.rs::u15_*`: `format!("{prepared:?}")` is redacted; `prepared.request()` exposes
URL/host; `into_parts()` → `timeout_mut()`, `headers_mut().insert(..)` (caller sets
`set_sensitive`), `client.execute(request)`, `decoder.decode(response)`; the server sees the new
header and token; a 200 ms native timeout trips against a silent server. Friction: two error
types meet, and the native execute error's `Display` includes the URL (asserted: contains
`eu.catalog.test`) unless the caller calls `without_url()`. The trial found U15 worked as designed.

### U28 — raw streaming, early stop, application timeout, REAL chunked fixture [ran]
`tests/u28_streaming.rs` with a hand-written `Transfer-Encoding: chunked` server (asserted on the
response headers): reading 3 chunks and dropping the response makes the server's writes fail long
before its 2,000 chunks (not finished; `written < 2000`); `tokio::time::timeout` around
`response.chunk()` abandons a stalled stream; a native `timeout_mut` set through `into_parts`
yields `is_timeout()` during body reading; the declared `Binary` buffers the whole body (contrast).
`chunk()` needs no reqwest feature; `bytes_stream()` would need reqwest `stream`. The repo's R08
test uses a content-length body, so these tests are the first chunked/early-stop evidence.

### U29 — form and multipart [ran]
- Form via custom `Encode` (`Form<T>`, serde_urlencoded, ~10 lines): wire body
  `sku=A+1%2F2&note=x%26y%3Dz`, content type `application/x-www-form-urlencoded`.
- Multipart via custom `Encode`: works, but `Encode::CONTENT_TYPE` is a `&'static str`, so the
  boundary must be constant; the encoder must reject content containing it (tested: build error
  "file content contains the multipart boundary").
- Multipart via the native route: endpoint declared without a body; `into_parts()` →
  `RequestBuilder::from_parts(client, request).multipart(form).build()` → `execute` →
  declared `decoder.decode` (needs the application's own reqwest `multipart` feature). Works; the
  declared authorization header is preserved.

### U16 — custom Encode/Decode downstream [ran]
`Csv` decoder with a content-free description: `GET Catalog.region.tenant.export: response not
decoded: CSV row 2 has 1 fields, expected 2`. Small, no transport reimplementation. Same
`BodyError` Display/Error friction as U14 when used outside Concord.

### Conveniences considered
| Candidate | Evidence | Decision |
|---|---|---|
| `Prepared::request_mut()` | `snap-c/tests/d13_prototype.rs`: one `path_segments_mut().push("rotate")` makes `Debug` print `/v1/keys/S3CR3T/<redacted>`; `set_host("edge.t0p…")` prints `<redacted>.t0p…`; an unflagged native `authorization` prints in clear. Redaction is by generated segment index (from the end), label index (from the left), query key and `is_sensitive` flag; native edits invalidate all of them. | **REJECT** (D2 reasoning confirmed with a reproduction) |
| `BodyError: Display + Error` (Display = description, `source()` = None) | prototype; positive control: serde's own message quotes `42` while `BodyError` Display does not | **CHANGE** (+11 lines) |
| `Error::into_response(self) -> Result<Response, Error>` | prototype; non-status errors come back intact (`GET S.item: transport error (connect)`); every existing `.unwrap()` caller and doc test compiles unchanged | **CHANGE** (signature) |
| `Prepared::decoder(&self) -> Decoder<R>` | prototype: `let d = prepared.decoder(); let r = prepared.send_raw().await?; r.headers()…; d.decode(r).await?` — 2xx headers (ETag) + typed body with Concord errors only | **optional CHANGE** (+9 lines); covers "inspect status/headers while retaining typed decoding" without the native error type |
| `Decoder::send(client, request)` / `Prepared::from_parts` | [reasoned] a second send verb; `from_parts` would re-attach stale redaction metadata | **REJECT** |
| Public "decode regardless of status" helper | the public `Decode` trait already does it | **no new surface**; show it in `into_response` docs |
| Value-dependent `Encode` content type (multipart boundary) | fixed boundary + native route both work | **DEFER** |

Prototype diff for all `snap-c` changes: `exp/core/snap-c.prototypes.diff` (4 files, +43/−5);
full `snap-c` suite passes (108 tests) [ran].

Intentionally unsupported: mutating a prepared request while keeping Concord's redacted `Debug`
and `send()`; once native, the request is the application's (`into_parts` is the one-way door).

---

## D15 — Error/sensitivity surfaces (S01–S09)

### Surface map
| Surface | Guarantee (control, as stated) | Evidence (existing / **added**) | Positive control |
|---|---|---|---|
| Generated client/scope/endpoint `Debug` | effective-sensitive values `<redacted>`; others via `Display`; base shown | `generated_debug_redacts`, `c4_r_001`; **`d15_doc`**: sensitive literal default `<redacted>` | wire in `s01_s02` |
| `Prepared` `Debug` | sensitive segments/labels by index, query by key, headers by `is_sensitive` flag; body `...`; base shown | `s02`, `s03`, `s04`, `adv_2`, `safety_list_*`; **`s03_remove_and_reintroduce_stays_sensitive`** | native URL/headers (`s04`, added test) |
| `Error` Display/Debug | static text only (endpoint, method, stage, template text, header name, status + reason, decoder description) | `s05_*`, `r03`, `r05`, `tls.rs` ×2, `adv_1` | via `transport()` in `tls.rs`; **none in `adv_1`** (see below) |
| `Error::source()` | always `None` | `build_errors_have_no_source`, `adv_1`, `tls.rs` | mutation [ran]: restoring `source()` for Transport fails `tls.rs` ×2 (leak asserts), `adv_1` (structural assert only) and the added TLS test (leak assert) |
| `transport()` | unredacted native error, top-level URL removed | `s05` baseline, `tls.rs` | yes |
| `body_source()` | unredacted, may quote content | `r03`; **`json_encode_error_is_content_free`** | yes (source quotes `S3CR3T-BODY`, Display does not) |
| Custom `BodyError` description | trusted user text, shown verbatim | **`s09_custom_encoder_description_is_trusted_text`** | yes (value appears) |
| Status response (`into_response`) | unread, never printed by Concord | `r05` | yes |
| Generated rustdoc | user doc comments + static generated lines; no values, defaults or literals | **`d15_doc`**: `cargo doc` HTML for items contains none of 5 literals; only rustdoc's `[src]` page (author's own source) does | — |
| Declaration diagnostics | may quote declaration source (base literal, template, offending char); compile-time only | `tests/ui/*.stderr` | n/a |
| Native escape (`request()`, `into_parts`, `send_raw`, native errors) | outside; sensitive header flags are a courtesy | `s06`; `u15_native_timeout_is_effective` (native Display has host) | yes |
| Caller-supplied base URL | never sensitive: shown in client and `Prepared` `Debug`; `with_base_url` errors never echo it; transport `Display` has no URL | `c13` (no echo); **`s07_runtime_base_is_shown_but_not_echoed_by_errors`** | yes |
| Panic messages | `from_static` panic text includes the declared base (unreachable per D11); capacity/`to_string` panics carry no values | D11 probe, `header_capacity.rs`, `fmt_error.rs` | — |

Added tests: `crates/concord/tests/d15_surfaces.rs` (7 pass). It uses rustls/TLS dev-deps, so in the
suite it needs `required-features = ["reqwest-defaults"]` like `tls.rs`.

### S-series coverage
S01 ✓ (wire + Debug). S02 ✓ (Debug; positive via native URL). S03 override ✓, **remove +
reintroduce was untested → added**. S04/S05 ✓ with positive controls. S06 ✓. **S07 policy was
stated but unpinned → added**. **S08 had no explicit test → added** (`s08_classification_without_
printing`). **S09 untested → added**. ADV-1: `adv_1_sensitive_host_hides_transport_source` has
**no positive control** — over plain HTTP with a refused connection the native chain never
contains the label (`adv_1_existing_scenario_has_no_positive_control` [ran]), so its leak assertion
is vacuous and only `source().is_none()` discriminates. Added
`adv_1_tls_name_mismatch_positive_control`: a certificate for `localhost` and a request to
`t0psecret.localhost` puts `t0psecret` in the native chain while Concord's output omits it; the
`source()` mutation makes it fail on the leak assertion itself.

### Is `ErrorKind` enough? [ran + reasoned]
Yes for branching without printing: `kind()`, `status()`, and native predicates on `transport()`
(`is_connect`, `is_timeout`, …) cover build/transport/status/decode; Concord's own `Display` is
static and safe to log. The one split not available as data is encode vs construction (both
`Build`); `body_source().is_some()` is a heuristic that misses source-less custom encoders.
**DEFER** an `ErrorKind::Encode` (enum is `#[non_exhaustive]`, additive) until a task needs it.

### Recommendation: **KEEP** the boundary
No `source()`; keep the `BodyError` trust contract and document it next to the S09 test. Fix the
evidence (ADV-1 positive control; S03/S07/S08/S09 tests).

---

## R12 / U30 / U31 — External composition proofs [ran]
`downstream/tests/r12.rs`, all against loopback, **zero core changes**:
- **Bounded retry** around a replayable factory: `retry(3, || t.product("p1").send(), |e| e.kind()
  == ErrorKind::Transport)`; a server dropping the first 2 connections → success on the 3rd
  (3 connections seen); with 3 failures and a bound of 2 → the Transport error after 2. Endpoints
  share no trait, so the helper takes a closure returning a future; requests with bodies are not
  `Clone`, so a factory is the general shape. Replay safety is the application's stated assumption.
- **Finite cursor loop**: `search.clone().maybe_cursor(cursor.take()).send()` up to an application
  bound; targets on the wire exactly `…?page_size=2&tag=new`, `…&cursor=c2`, `…&cursor=c3`.
- **Admission before execution**: prepare (no I/O) → `Semaphore::acquire_owned` → `send`; 8 calls
  with 2 permits → server peak concurrency ≤ 2.
- **Caller-updated authorization**: the application holds the token and rebuilds the handle
  (`Catalog::new(http.clone(), token)`; required root values have no setter, and `new` is cheap);
  the wire shows `Bearer old` (401), `Bearer new` (200), and an endpoint value captured before the
  update still sends `Bearer old` (snapshot semantics).

None needs a core change; native parts were sufficient in every case.

---

## DEFECTS found in the control (separate from design opinions)
1. **fmt::Error inconsistency** (low): path/host keep partial text silently; query/header/generated
   `Debug` panic. Repro: `SNAP/crates/concord/tests/fmt_error.rs`.
2. **ADV-1 test lacks a positive control** (evidence quality): leak assertion vacuous over plain
   HTTP. Repro and replacement: `SNAP/crates/concord/tests/d15_surfaces.rs`.
3. **Latent panic sites at HeaderMap capacity** in `Parts::header`/`finish` (not reachable by
   caller data; declaration size makes it uncompilable in practice). Repro:
   `SNAP/crates/concord/tests/header_capacity.rs`; fix prototyped in `snap-c`.
4. **`Error::into_response` drops non-status errors** (`Option`), forcing status-first ordering
   (also reported by the fresh-use trial). Repro: `exp/core/d13_compile_fail/rustc_output.txt`.
5. Matrix claim weaker than wording: TEST_MATRIX R08 "raw response streamable natively" is
   backed by a content-length body; chunked/early-drop evidence now in `u28_streaming.rs`.

Observations for other tracks (not investigated further): the `Sticky` fixture in
`d15_surfaces.rs` (client values whose only placements are removed by every endpoint) produces a
rustc `dead_code` warning "fields `k` and `q` are never read" pointing into generated code; macro
compile memory grows superlinearly with header statements in one owner (0.36 → 1.46 → 4.70 GB at
2k/6k/12k) — C16 territory.

---

## Commands actually run (rerunnable)
Paths: `SNAP=/home/canarit/projects/concord_v4_refine_work/snapshots/core-8f7adc2`,
`EXP=/home/canarit/projects/concord_v4_refine_work/exp/core`. All with `CARGO_BUILD_JOBS=6`.

```sh
# D11 control: real-macro corpus, probe and random search
cd $EXP/d11 && python3 -I gen.py static
cd $EXP && CARGO_TARGET_DIR=$EXP/target cargo check --offline -p d11-static-check --message-format=json > d11/static_check.jsonl   # fails by design (74 rejections)
python3 -I $EXP/d11/parse_errors.py && cd $EXP/d11 && python3 -I gen.py probe rejected.json
cd $EXP && CARGO_TARGET_DIR=$EXP/target cargo run --offline -p d11-probe --release -- 300000 > d11/probe_output.md
# D11 option (b): corpus through the url-based macro, dependency trees, consumer units
cd $EXP/d11/static_check_b && CARGO_TARGET_DIR=$EXP/d11/consumer_b/target-check cargo check --offline --message-format=json
cd $SNAP && cargo tree --offline --locked -p concord-macros -e normal --prefix none
cd $EXP/snap-b && cargo tree --offline -p concord-macros -e normal --prefix none
cd $EXP/d11/consumer_{control,b} && CARGO_TARGET_DIR=./target-check cargo check --offline -v   # also `build`, `check --timings`
# D11 send stage and option (c)
cd $EXP && CARGO_TARGET_DIR=$EXP/target cargo test --offline -p core-downstream --test d11_send_stage -- --nocapture
cd $EXP && CARGO_TARGET_DIR=$EXP/target cargo test --offline -p core-downstream --test d11_fallible
# D12 / D15 in SNAP (control)
cd $SNAP && CARGO_TARGET_DIR=$SNAP/target cargo test --locked -p concord --all-features --test header_capacity --test fmt_error --test d15_surfaces
cd $SNAP && CARGO_TARGET_DIR=$SNAP/target cargo test --locked -p concord --all-features --test header_capacity probe_native_threshold -- --ignored --nocapture
# D15 mutation (apply source() for Transport in src/error.rs, run, then `git checkout crates/concord/src/error.rs`)
cd $SNAP && CARGO_TARGET_DIR=$SNAP/target cargo test --no-fail-fast --locked -p concord --all-features --test d15_surfaces --test sensitivity --test tls
# D12 real-macro memory (N = 2000/6000/12000)
cd $EXP/d12_macro && python3 -I gen.py 12000 && CARGO_TARGET_DIR=./target CARGO_BUILD_JOBS=1 /usr/bin/time -f "%MKB %es" cargo build --offline --lib
# D13 / R12 downstream, friction capture, rustdoc check
cd $EXP && CARGO_TARGET_DIR=$EXP/target cargo test --offline -p core-downstream
cd $EXP && CARGO_TARGET_DIR=$EXP/target cargo check --offline -p d13-compile-fail      # fails by design
cd $EXP && CARGO_TARGET_DIR=$EXP/target cargo doc --offline --no-deps -p d15-doc && grep -rl dflt-S3CR3T target/doc/
cd $EXP && CARGO_TARGET_DIR=$EXP/target cargo test --offline -p d15-doc -- --nocapture
# Prototypes (D12 try_*, D13 conveniences): full suite
cd $EXP/snap-c && CARGO_TARGET_DIR=$EXP/snap-c/target cargo test --locked -p concord --all-features
```
Full SNAP suite with the three added files passes (`cargo test --locked -p concord
--all-features`), as does the full `snap-c` suite.

## Reasoned only (not executed)
- Why `cargo check` pays more than `cargo build` for (b) (host must codegen what the target only
  checks); the effect of `idna_adapter` unification; MSRV of the added stack (icu_normalizer
  declares rust-version 1.83 ≤ 1.85).
- The uniform fmt::Error fix (not prototyped); url `ParseError` message mapping; an
  `ErrorKind::Encode` split.
- Native-layer header additions other than `accept`/`host` (proxy auth, application default
  headers) lower the send-time threshold further.
