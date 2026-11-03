Two **should-fix** metadata-policy defects remain. All 33 repository gate steps passed; the independently written built-in-codec suite finished **3 passed, 2 failed**. Both failures remain as regressions under `audit/`. No candidate library source was changed. Acceptance remains undetermined.

Snapshot HEAD is `f08a914805603413e3760b8f1ca15936c30999a5`, initially clean. Its commit message identifies original source `9f8939f2c7114a928c70bb32dd34c5ee3b3316f4`. That original object is unavailable locally, so I verified snapshot identity but could not independently compare its tree with the original commit.

The current session’s local `turn_context` confirms **`gpt-6.1-sol`, effort `xhigh`**, approval policy `never`, and sandbox `workspace-write`. Selected evidence is in `audit/effective-settings.json`. Executed toolchain evidence:

```text
rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo 1.99.1 (5f94df478 2026-08-27)
host: x86_64-unknown-linux-gnu
```

Rustup reports the repository’s exact 1.99.1 override. No global settings were changed.

**Should fix F1 — Valid quoted UTF-8 charset rejected when another parameter follows.**

Location: [media.rs:28](/home/canarit/projects/concord_v4_modular_work/snapshots/final-exec-sol/crates/concord-extras/src/media.rs:28), shared by Json, Text and Form.

A response containing:

```http
Content-Type: application/json; charset="utf-8"; audit=x
```

with body `7` fails with `Metadata`. Moving charset to the last parameter succeeds. Form reproduces the refusal with its own media type and body `a=7`.

Reproducer, from the snapshot root:

```sh
CARGO_TARGET_DIR="$PWD/audit/target" cargo test --offline \
  --manifest-path audit/probe/Cargo.toml --features builtin --test builtin \
  quoted_utf8_before_another_parameter_must_decode -- --exact --nocapture
```

Exit: **101**. Exact output excerpts:

```text
multiple parameters "content-type: application/json; audit=x; charset=\"UTF-8\"\r\n": Json=Ok(7); Text=Ok("7")
quoted charset before parameter Form: Err(Error { kind: Metadata, endpoint: Some("Builtin.form_response"), message: GET Builtin.form_response: response metadata not accepted: the form response declares a charset other than utf-8 })
test quoted_utf8_before_another_parameter_must_decode ... FAILED
```

The rejected JSON ordering reports:

```text
GET Builtin.json: response metadata not accepted: the JSON response declares a charset other than utf-8
```

I separately compiled and executed `audit/media_parser.rs` against pinned `mime 0.3.17`. Exact output:

```text
"application/json; charset=\"utf-8\"; audit=x": charset=Some("\"utf-"); params=[("charset", "\"utf-"), ("audit", "x")]
"application/json; audit=x; charset=\"utf-8\"": charset=Some("utf-8"); params=[("audit", "x"), ("charset", "utf-8")]
```

This establishes a pinned parser defect reaching the public codec policy. Resolve that behavior and retain wire regressions across all three codecs.

**Should fix F2 — Conflicting duplicate charset parameters bypass refusal.**

The same helper calls `get_param(CHARSET)`, selecting only the first parameter. A single field containing `charset=utf-8; charset=iso-8859-1` is accepted by Json, Text and Form. That permits ambiguous metadata containing an explicitly foreign charset.

Reproducer:

```sh
CARGO_TARGET_DIR="$PWD/audit/target" cargo test --offline \
  --manifest-path audit/probe/Cargo.toml --features builtin --test builtin \
  duplicate_charset_must_not_hide_a_declared_foreign_charset -- --exact --nocapture
```

Exit: **101**. Exact output:

```text
duplicate charset Json: Ok(7); Text: Ok("7")
duplicate charset Form: Ok([("a", "7")])
test duplicate_charset_must_not_hide_a_declared_foreign_charset ... FAILED
```

The assertion observed:

```text
  left: (None, None, None)
 right: (Some(Metadata), Some(Metadata), Some(Metadata))
```

The parser probe independently printed:

```text
"application/json; charset=utf-8; charset=iso-8859-1": charset=Some("utf-8"); params=[("charset", "utf-8"), ("charset", "iso-8859-1")]
```

Reject conflicting charset parameters. This finding demonstrates a metadata-policy failure, not a confidentiality leak.

**Personally executed — repository gate.**

I ran:

```sh
env -u CARGO_TARGET_DIR CARGO_NET_OFFLINE=true scripts/verify.sh --package --bench
```

This removed the inherited target override and let the gate use its default `target/`. Logs show dependency compilation. Independent builds used newly created `audit/target`.

Every gate PASS/FAIL line, in order:

```text
PASS  toolchain
PASS  ci-inventory
PASS  graph
PASS  fmt
PASS  records
PASS  generated
PASS  clippy
PASS  test
PASS  test-no-default
PASS  test-json-only
PASS  test-reqwest-defaults-only
PASS  test-form-only
PASS  test-form-json
PASS  test-extras-only
PASS  test-core
PASS  test-extras-none
PASS  test-extras-json
PASS  test-extras-form
PASS  consumers
PASS  feature-isolation
PASS  test-multipart-alone
PASS  doc
PASS  doc-no-default
PASS  doc-json-only
PASS  doc-form-only
PASS  doc-form
PASS  doc-extras-only
PASS  doc-core
PASS  doc-extras
PASS  doc-extras-none
PASS  bench-build
PASS  package-check
PASS  consumers-packaged
```

No gate FAIL lines occurred. The workspace test summary reports **644 passed, 0 failed**; 13 quickstart/migration doctests were ignored. Feature runs repeat tests and should not be summed as unique coverage. Full output: `audit/verify.stdout`; detailed gate logs: `target/verify-*.log`.

**Personally executed — independent downstream probes.**

I wrote `ScaledRecords` in standalone workspace `audit/probe`, against renamed facade `sdk` with default features disabled. It validates a vendor media type, requires a single `x-audit-scale` response header, retains owned context, decodes numbered records, and consumes a non-Clone payload for encoding.

- The no-extras run passed **2 tests**. A loopback call decoded `2\n5` with scale 3 into `[6, 15]`. Check/context/decode counters totaled 111.
- Head refusal returned while server body bytes written remained **0**. Explicit response retrieval followed by body release returned the complete unread body. Non-success status ran no codec step.
- Renamed facade, `macro_rules!` forwarding, lowercase format alias, disabled nonexistent format/key, unused nonexistent default, and two crates’ identically printed `$crate::KEY` paths worked. The latter produced distinct public and sensitive headers.
- Validator sentinels ran no defaults, Display or encoding. All four optional-presence assignments for aliases, `else inherit`, and remove/reintroduce matched preparation. Optional header/body combinations with declared media also matched, including declaration labels.
- A parser source deliberately echoed `BODY_SECRET`. Ordinary Error/CodecError Display, Debug and source chains omitted it; `source_unredacted()` retrieved it. Refused-response header secrets and sensitive Accept values stayed out of tested Concord errors, clients, endpoints and Prepared rendering.
- Three numeric positions were retained; a fourth was discarded.

The independent built-in suite passed three tests and failed F1/F2. Passing wire cases covered:

- Missing, equal-duplicate, conflicting-duplicate, comma-joined and malformed Content-Type; `+json`; ordinary case/parameter handling; foreign charset; HTML-labelled JSON.
- Strict versus compatibility decoding, empty/whitespace/schema failures, null/unit/Option body-only decoding, Text UTF-8, Binary PNG, and Form input/Json output.
- Withheld-body refusal, unread response access, raw 302, standalone Decoder, native execution with `Decoder::finish`, unchanged decoder after Accept mutation, status ordering, and compatibility retaining custom context checks.
- Client-default Accept, suppression, reintroduction, authored empty values and repeated values. Exact wire examples:

```text
accept suggested: prepared=["application/json"]; wire=["application/json"]
accept removed: prepared=[]; wire=["application/x-client"]
accept again absent: prepared=["application/json"]; wire=["application/json"]
accept empty: prepared=[""]; wire=[""]
```

I also executed three external negative checks. Each returned **101 for the intended reason**:

```text
native_key:
error[E0277]: the trait bound `Changing: concord::__macro::concord_core::__private::sealed::Sealed` is not satisfied

dynamic_error:
error[E0308]: mismatched types
expected `&str`, found `String`

reserved_validator:
error: `Collision` would have two methods named `validate_header_names`: the navigation method for `Collision.validate_header_names` and the built-in `validate_header_names`; rename one of them
```

The NativeKey implementation attempted to alternate names through an atomic counter. Sources and logs remain under `audit/probe/` and `audit/negative-*`.

**Personally executed — isolation, packages and Riot.**

I separately ran core-only and renamed facade-without-extras consumers: **3 tests passed each**. The gate ran all five isolated consumers against workspace sources and unpacked packages. Their graph exclusions passed.

I executed separate normal dependency trees for core, macros, extras with none/json/form/all features, and facade with none/default/extras/json/form features. The new codec’s no-feature tree contained core and macros, with no extras, serde_json or serde_urlencoded. Trees are saved under `audit/graphs/`.

I enumerated and inspected all four `.crate` archives: facade **8 files**, core **15**, extras **14**, macros **11**. None contained audit/build/history/test/fixture material. File inventories and archive hashes are in `audit/package-contents.json`. Packaged-source consumers actually compiled and ran.

I separately executed Riot’s redirect test binary: **4 passed**. The shared application policy preserved typed Status/302 and raw 302; the second origin received zero requests. The default-policy control forwarded the dummy key. The live builder refused plain HTTP. These were personally executed existing tests, not a newly authored Riot fixture. No real credential or live mode was used.

**Personally measured.**

Linux `7.0.0-42-generic`, x86-64, Intel i5-13600K, 20 logical CPUs, Rust 1.99.1; optimized Criterion executable. Ten samples, 100 ms warm-up, 200 ms requested measurement per case:

| Distinct contributions | Compose interval | Validate interval |
|---|---|---|
| 8 | 338.09–339.42 ns | 77.610–78.724 ns |
| 512 | 899.78–903.25 µs | 228.35–229.11 µs |

Criterion reported two high-severe outliers for 8-contribution validation. This short run establishes no before/after speedup. Exact command and output: `audit/measurement.log`.

**Inspection-only evidence and limits.**

Source inspection found the four dependency roles, exact alpha dependency coordination, feature forwarding, small hygienic facade bridge, no production codec dispatch/media/feature table in macros, indexed occurrence ownership, and one structural function shared by validator and preparation. CI calls the common gate and configures Windows execution; I did not execute that platform.

The hierarchical-path surface remains deferred. Its maintained native recipe tests ran through the gate.

I did **not** run native Windows, optional nightly Clone checks, historical baseline/compiler controls, 100/1,000/4,000-endpoint expansion comparisons, allocation measurements, or the complete timing matrix. Existing architecture-review and performance claims were not independently rerun. Decompression-enabled reqwest was not run; compression dependencies were absent from the offline cache. The gate exercised its decompression-disabled Content-Encoding regression. The 13 ignored doctests remain unexecuted.

No external release verification, new review-provider sessions, live services, push or publication occurred. Final tracked diff is empty; only `audit/` is untracked. F1 and F2 remain open for integrator triage and correction.