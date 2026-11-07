# Final report — modular codecs increment

## Identities

| What | Identity |
|---|---|
| Final code commit | `fcf343f` on branch `concord-v4-modular` (not pushed; no upstream) |
| Records HEAD | the head of `concord-v4-modular`: after `fcf343f` come only records commits, the one adding this report and one recording its gate (Verification) |
| Working tree at the final gate | clean: no tracked change, no untracked file in the repository |
| Starting point | `concord-v4-final` at `dfdeed8` (kept, like every historical branch) |
| Frozen candidate reviewed | `9f8939f`; corrections `ee0117c` (audit X-F1/X-F2), `d98e4cf` (semantic review S1–S4, M1–M8), `abdad04` and `2cbac44` (re-review findings N1–N3, RR2-1..RR2-4), `fcf343f` (dead code in `concord-extras` without `json`/`form`, a feature-subset lint step, the benchmark generator; found by the measurement) |
| Toolchain | Rust 1.99.1 is both `rust-version` and the `rust-toolchain.toml` pin; the machine's default toolchain (stable 1.99.0) is unchanged |
| Measurement sources | S2 `686cd24`, SX `dc7f134`, R5 `3a836e7`/`a4b465a`; the candidate `2cbac44` for allocations and Criterion, `fcf343f` for compile time, rebuild and R5 (library code identical in the measured builds); raw data in the work directory's `evidence/perf/run-final-2cbac44/` ([PERF.md](PERF.md)) |

Status words below: **implemented** (in the code commit), **tested** (a discriminating test or
command in the gate, TEST_MATRIX), **reviewed** (examined by an independent reviewer who did not
write it, with executed evidence where stated), **accepted** (the coordinator's acceptance against
the handoff's matrix after every required review returned; the owner's own acceptance is theirs).

## What was required, and where it stands

| Requirement | Implemented | Tested | Reviewed | Accepted |
|---|---|---|---|---|
| Four crates: `concord-core` (runtime, open contracts), `concord-macros` (no codec knowledge), `concord-extras` (standard formats over the public traits), `concord` (re-exports, features, one-line hygienic bridge) | `7ec1f4b`, `dc7f134` | A01–A04, A10; `scripts/check-graph.py`; five isolated consumers on sources and on unpacked packages | arch challengers; final exec audit (graphs, packages, consumers); semantic review (boundaries) | yes |
| Formats are type paths; `Json` and a downstream format resolve the same way; `body x: T`, `-> T` and format statements unchanged | `df37383` | A05–A09 (unknown type = E0425, aliases, renamed facade, cfg-disabled formats never resolved) | final exec audit (hygiene probes); semantic review | yes |
| Rust 1.99.1 minimum and pin; same-compiler baseline kept | `686cd24` | T01–T04; state 1 (`dfdeed8`, 1.97.1) and state 2 (`686cd24`, 1.99.1) gates in STATUS | final exec audit (toolchain evidence) | yes |
| Open, metadata-aware `Decode` (content type → context → read → body) and `Encode` | `ca545f0` | C01–C20 (C19 partial) | two architecture challengers chose the shape; final exec audit (a new downstream format); semantic review (S3 wording fixed) | yes |
| Response `Content-Type` validated by the format, never compared with `Accept` | `ca545f0` | C02–C05, C18 | both final reviews | yes |
| `Accept` suggestion as a fallback after authored composition; suppression and authored values preserved; reqwest client-default boundary documented | `ca545f0` | M01–M06, M11 (truth table on prepared requests and on the wire) | final exec audit (wire); semantic review (S4, M5 docs fixed) | yes |
| Strict `Json` in extras; a deliberate compatibility adapter (`IgnoreContentType<F>`) and an application compat format (`JsonOrEmpty`) | `ca545f0`, `ee0117c`, `d98e4cf` | C03–C05, C10, M10; charset and RFC 9110 parameter regressions | final exec audit (X-F1/X-F2, fixed); targeted re-review; semantic review (M4, fixed) | yes |
| Raw bytes for any media type (`Binary`) | `ca545f0` | C11 | final exec audit | yes |
| Unread response kept on metadata failures; ordinary errors safe; `CodecError` static labels with numeric positions | `ca545f0` | C06, C20 (body-first mutant times out), E01–E06 | final exec audit (confidentiality probes, unread-body proofs); re-review (12 refusals, 0 body bytes, 0 leaks) | yes |
| Riot redirect guard | `93f9b0e` | R01–R02 (second origin receives nothing; default-policy control forwards the dummy key) | final exec audit | yes |
| No-instance validation of native header names, sharing `prepare()`'s structural rules without inventing presence | `25a52f7`; docs narrowed `d98e4cf` (MC19) | H01–H07, H12–H13 (every presence assignment; build order pinned) | final exec audit (presence comparisons); semantic review (S2) | yes |
| `NativeKey` sealed | `3a836e7`; test repaired `d98e4cf` | H08 (E0277; a mutant without the seal fails the test) | final exec audit (external impl refused); semantic review (S1) | yes |
| Codec-error construction hardened, numeric context kept | `ca545f0` | E01 (`compile_fail,E0308` on a `String`), E02–E03 | both final reviews | yes |
| Native-key occurrence provenance indexed | `a4b465a` | H09 | semantic review | yes |
| One verification inventory for local and CI | `013de15` | T03 (`scripts/check-ci.py --workflow`) | final exec audit (ran it) | yes |
| Hierarchical paths: bounded prototype, adopted or deferred | prototype (Terra), recipe `1f26e91` | P01–P07 **deferred** (MC16); native recipe tested | semantic review (deferral sound) | deferred |
| Fresh users complete a custom metadata codec and all-endpoint validation from docs | docs `d5c93f0` and later | R06–R07 (Terra and Sonnet trials) | — | yes |

## The four crates

`cargo tree -e normal --depth 1` (all features; facade also with `default-features = false`):

```text
concord-core v4.0.0-alpha.1
├── bytes v1.12.1
├── percent-encoding v2.3.2
├── reqwest v0.13.5
└── url v2.5.8

concord-macros v4.0.0-alpha.1 (proc-macro)
├── http v1.5.0          (literal header names and values checked at expansion)
├── mime v0.3.17         (declared media types checked at expansion)
├── proc-macro2, quote, syn

concord-extras v4.0.0-alpha.1
├── concord-core v4.0.0-alpha.1
├── mime v0.3.17
├── serde, serde_json (feature json), serde_urlencoded (feature form)

concord v4.0.0-alpha.1
├── concord-core v4.0.0-alpha.1
├── concord-extras v4.0.0-alpha.1   (features json, form, extras)
└── concord-macros v4.0.0-alpha.1

concord v4.0.0-alpha.1 (default-features = false)
├── concord-core v4.0.0-alpha.1
└── concord-macros v4.0.0-alpha.1
```

The inter-crate requirements are exact (`=4.0.0-alpha.1`). The facade's `api!` is one
`macro_rules!` line passing `$crate::__macro::concord_core` to the procedural macro; `__macro`
and `__private` are documented as unstable. No crate has a build script.

## A complete downstream format

Compiled and run in `crates/concord/tests/metadata.rs` (excerpt: the test's decode counter and its other endpoints omitted; taught in `docs/CUSTOM_FORMATS.md` §5):
`Json`'s strict media check, a required `x-api-version: 2` checked before the body is read, and the
version handed to the body decoder as a typed context.

```rust
pub const API_VERSION: HeaderName = HeaderName::from_static("x-api-version");
pub struct Versioned;

impl Decode<Item> for Versioned {
    type Context = u32;
    const ACCEPT: Option<&'static str> = Some("application/json");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<Item>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<u32, CodecError> {
        let mut values = head.headers().get_all(&API_VERSION).iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return Err(CodecError::new("the response has no single x-api-version"));
        };
        match value.to_str().ok().and_then(|v| v.parse().ok()) {
            Some(2) => Ok(2),
            Some(v) => Err(CodecError::new("unsupported x-api-version").at("version", v)),
            None => Err(CodecError::new("x-api-version is not a number")),
        }
    }

    fn decode_body(version: u32, body: Bytes) -> Result<Item, CodecError> {
        let item: Item = <Json as Decode<Item>>::decode_body((), body)?;
        Ok(Item { id: item.id + u64::from(version) * 1000 })
    }
}

concord::api! {
    pub client Meta {
        base "http://meta.invalid";
        response Json;
        GET versioned() -> Item { path "item"; response Versioned; }
        GET versioned_any_type() -> Item { path "item"; response IgnoreContentType<Versioned>; }
    }
}
```

A refusal in `check_content_type` or `context` is `ErrorKind::Metadata` with the response unread
(`Error::response()`, `Error::into_response()`); `IgnoreContentType<Versioned>` skips only the
`Content-Type` check, so the version is still checked. The same contract compiles against
`concord-core` alone (`checks/consumers/core-only`) and against the facade without extras
(`checks/consumers/facade-no-extras`, renamed `sdk`, a lowercase alias `type json = MyJson;`).

## Validating native header names

```rust
#[test]
fn header_names() {
    Riot::validate_header_names().unwrap();       // examples/riot/tests/wire.rs
    DataDragon::validate_header_names().unwrap();
}
```

No client, configuration, value, network or runtime: only the native `HeaderName` constants are
read, with the structural rules `prepare()` applies when it composes headers. A reported endpoint
can build no request; `Ok(())` means no `prepare()` fails on header names. `unwrap_err()` lists one
problem per endpoint (`endpoint()`, `declaration()`, `earlier()`, `reason()`); `Debug` and
`Display` show the same lines, never values.

## Strict and compatible responses

| Format | Response | Outcome | Test |
|---|---|---|---|
| `Json` | 200, `text/html`, JSON bytes (server withholds the body) | `Metadata`; zero body bytes read; `into_response()` returns it unread | `metadata.rs::the_head_is_refused_before_the_body_exists` |
| `IgnoreContentType<Json>` | 200, `text/html`, JSON bytes | decoded | `bodies.rs::r03_content_type_is_checked_unless_deliberately_ignored` |
| `IgnoreContentType<Json>` | 200, `text/html`, `<html>` | `Decode` (only the media check is skipped) | same |
| `Json` | 200, `application/json; charset=iso-8859-1`, or `charset` twice | `Metadata` | `crates/concord-codecs/tests/media.rs`, `metadata.rs::charset_parameters_are_read_exactly` |
| `Json` | 204 without `Content-Type` | `Metadata` ("the response has no Content-Type") | `error_inspection.rs::empty_responses_are_not_null` |
| `JsonOrEmpty` | 204; 200 `application/json` `{"track":..}`; 200 `null` | `Ok(None)`; `Ok(Some(..))`; `Ok(None)` | `examples/compat/tests/json_or_empty.rs` |
| `JsonOrEmpty` | 200 without `Content-Type`; 200 `application/json` empty or `{` | `Metadata`; `Decode` | same |
| any | 404 | `Status`, response unread; the format is not consulted | `metadata.rs::status_comes_before_the_format` |

`JsonOrEmpty` is the application's own format (a `204` decides through `context`), not a Concord
mode; `Json` stays strict. Bare test names are in `crates/concord/tests/`.

## The Riot application builder

```rust
pub fn http_policy(builder: ClientBuilder) -> ClientBuilder {
    builder
        .redirect(redirect::Policy::none())
        .timeout(Duration::from_secs(10))
}

pub fn live_http_client() -> concord::reqwest::Result<concord::reqwest::Client> {
    http_policy(concord::reqwest::Client::builder())
        .https_only(true)
        .build()
}

// examples/riot/src/main.rs (bounded live-read mode, opt-in, key from the environment)
let http = live_http_client().expect("HTTP client");
let riot = Riot::new(http, RiotConfig { key });
```

reqwest strips only a fixed list of credential headers on a cross-origin redirect; `X-Riot-Token`
is not on it. With this policy a `3xx` reaches the caller as a `Status` error (or raw from
`send_raw`). The tests run offline against loopback fixtures with a dummy key; no live call was
made in this increment, and unresolved remote facts stay open questions in `examples/riot/SOURCES.md`.

## Reviews and dispositions

| Review | Reviewer (observed) | Source | Outcome |
|---|---|---|---|
| Architecture and codec challenge, two independent prototypes | `gpt-6.1-sol` xhigh; `claude-opus-5-5` | `dfdeed8` | converged on the three-step `Decode` (MC08–MC12) |
| Author-journey probes (docs only) | `gpt-6.1-terra` xhigh; `claude-sonnet-5-5` | `dfdeed8` | documentation plan |
| Bounded path prototype | `gpt-6.1-terra` xhigh | `25a52f7` | deferred (MC16): a static-index mutant mis-redacts |
| Fresh-use trials (R07) | `gpt-6.1-terra` xhigh; `claude-sonnet-5-5` | `9f7eb83`, `9f8939f` | both completed both tasks from the docs; notes applied or recorded (MC18) |
| **Final executable audit** of the frozen candidate | `gpt-6.1-sol` xhigh (fresh) | `9f8939f` | no blocking; X-F1, X-F2 (charset parsing) should fix → fixed `ee0117c` |
| **Final semantic review** | `claude-opus-5-5` (side) | `9f8939f` | no blocking; S1–S4 should fix, M1–M8 minor → fixed `d98e4cf` |
| Targeted re-review of the audit corrections | `gpt-6.1-sol` xhigh (fresh) | `4ce3d01` | X-F1/X-F2 resolved; RR-1, RR-2 (should fix), RR-3 → already fixed by `d98e4cf`, checked with its probe |
| Re-review of the semantic corrections | `claude-opus-5-5` (same side session) | `530b6c1` | S1–S4, M1–M8 closed; N1–N3 minor → fixed `abdad04`; MC19 rationale disagreement recorded |
| Second targeted re-review of all corrections | `gpt-6.1-sol` xhigh (fresh) | `3fa14b7` | all earlier findings resolved; RR2-1 (should fix), RR2-2..4 (minor) → fixed `2cbac44` |
| Narrow re-review of the last delta | `gpt-6.1-sol` xhigh (fresh) | `d28ad34` | no finding |
| Static review of the final lint/tooling delta | `gpt-6.1-sol` medium (fresh, read-only) | `fcf343f` | no finding |

No reviewer certified code it wrote: every implementation line of this increment is the coordinator's
(`claude-opus-5-5`), except the isolated consumers (`gpt-6.1-sol` medium, under exclusive write
ownership), the native path recipe in `examples/compat` (from the `gpt-6.1-terra` prototype) and
the documentation rewrite (`claude-sonnet-5-5`); the final reviews examined all three. No finding is open.

Details: [REVIEW_TRIAGE.md](REVIEW_TRIAGE.md); models and frozen reports: [MODEL_RUNS.md](MODEL_RUNS.md).

## Verification

Final gate: `scripts/verify.sh --package --bench --nightly` on the commit that adds this report:
recorded in STATUS ("Final gate") by the records-only commit that follows it. The final code
commit `fcf343f` passed the same gate (35/35, 650 tests in the facade's default lane; every other lane listed by `scripts/verify.sh`), as did each correction
commit before it (STATUS). Each independent reviewer also ran the gate on its own snapshot
(`9f8939f`: 33 steps, 644; `4ce3d01`: 648; `3fa14b7`: 650), except the narrow and static
re-reviews of the last two deltas.

| Coverage | Outcome |
|---|---|
| Linux x86_64, Rust 1.99.1 | every gate step (`scripts/verify.sh --package --bench --nightly`), including lints of the feature subsets (`clippy-features`) |
| Feature lanes | facade default, no-default, json-only, reqwest-defaults-only, form-only, form+json, extras-only, multipart; core alone; extras none/json/form; docs for each |
| Isolated consumers | core-only, facade-no-extras, extras-core, facade-full, facade-json-only: on workspace sources and on the four unpacked `.crate` files |
| Packages | four `.crate` archives extracted and built by a consumer; crates.io publication not tested (local patches stand in for the registry) |
| Nightly | non-blocking `Clone` compatibility lane, run locally |
| Windows, macOS | **NOT_RUN**: native CI needs a push, not authorized. `ci.yml` runs the same `scripts/verify.sh` on `windows-latest` |
| Decompression-enabled reqwest (C19) | **NOT_RUN**: no compression dependencies in the offline cache; the decompression-disabled `Content-Encoding` case is tested |
| Rendered docs | rustdoc HTML of the facade, core, extras and the Riot example's generated client inspected as extracted text: every changed passage present, no unrendered intra-doc link (the only missing link targets are in `tracing`'s blanket-impl docs). Visual rendering NOT_RUN: the in-app browser cannot open local files |

## Measured trade-offs

Against the same-compiler baseline (S2), the final candidate: allocations identical in 84 of 92
cases, the others explained by the new `Accept` header (Concord's overhead over hand-written
reqwest unchanged: 5 allocations, 129 bytes) and the new response-head checks (1 allocation for
`Json`, 3 for `Text` with a charset, fixed per response); runtime +11 ns per decode for the head
step, +67 ns (`Json`) and +105 ns (`Text`) per small response for the metadata checks, request
building and composition within noise; compile time within 3% for `check` and release builds of
large APIs, +7% for the typed suite and +19% (+0.37 s) for the debug build of 1,000 endpoints, the
crate split alone 0–3% (+8% for that debug build); rebuilding the Concord crates +1.1%; release
binary +0.26%. Header-plan composition grows quadratically with the length of one native chain
(sub-microsecond for realistic chains, under 1 ms at 512 contributions). Full tables: [PERF.md](PERF.md).

## Migration

`docs/MIGRATION.md`: format words become types (`format json;` → `format Json;`, compiler-guided,
no rewriter); `Decode` gains `check_content_type`/`context`/`decode_body`; `CodecError` takes
`&'static str` labels; strict `Json` may refuse metadata it used to ignore (and how to select
`IgnoreContentType<Json>` deliberately or inspect the unread response); typed endpoints send an
`Accept`, which replaces a client-wide default `Accept`; `into_response()` also returns the 2xx
response of a `Metadata` error; facade features (`json`, `form`, `extras`, `reqwest-defaults`).

## Limitations and open items

- Windows/macOS execution and decompression-enabled reqwest: NOT_RUN (above).
- Hierarchical typed paths are deferred (MC16); `examples/compat` keeps a native recipe.
- Strict `Json`'s charset rule is stricter than RFC 8259 requires (a deliberate default;
  `IgnoreContentType<Json>` is the escape). `Text` treats a field it cannot read — malformed, or
  with bytes outside visible ASCII — as no field.
- The structural header rules exist twice, as before this increment: the macro checks literal-name
  chains at compile time, core checks native chains; they are cross-checked at the `HeaderPlan`
  level only.
- Naming feedback from one trial (`Metadata`, `context`, `CodecError`, `validate_header_names`) was
  recorded and not acted on (MC18).
- 13 doctests are marked `ignore` on purpose: the "before" code of MIGRATION, which no longer
  compiles, and excerpts that a test checks verbatim against compiled sources; every other code
  block is compiled.
- Process: two commit messages lost backticked words to the shell (`ca545f0`, `df37383`; errata in
  the work directory, history not rewritten); two gate runs were invalidated by the coordinator
  editing files during the run and were rerun on clean heads.
