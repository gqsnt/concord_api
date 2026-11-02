Original source SHA: 1174d5d8c894fa768546f6600cf0415faf4690c5
Independent snapshot HEAD: bed9fd29d336c30be4735a6d545d2c7ee19b6355
Review mode: execute; requested model: gpt-6.1-sol; requested effort: xhigh.
Verify effective settings; report personally executed versus inspected-only evidence.
Never push, publish, modify global settings, bypass sandbox, or claim acceptance before triage.

## Supplied document: prompt.md

# Codex task — focused executable re-review of `1174d5d` (your findings F1–F4 on `66a4478`)

Your executable review of `66a4478` (attached below as `final_66a4478_report.md`) returned REVISE with
F1 (P1) to F4 (P3). The local team's fixes are on `1174d5d`; its triage is in
`v4/native-repr/REVIEW_TRIAGE.md` ("Codex executable review of the candidate `66a4478`") and the
rule change in `v4/native-repr/DECISIONS.md` (N02 amendment) and `CONTRACT.md` row H10b. The code
delta since `66a4478` is `git diff 66a4478..1174d5d` in spirit; in this snapshot read the changed
files: `crates/concord/src/__private.rs` (`TextHeader`, `header_text*`, removal of
`DebugTextHeader`), `crates/concord-macros/src/emit.rs` and `model.rs` (routing), the new tests
`crates/concord/tests/ui/header_text_dual_use.rs` and
`crates/concord/tests/native_headers.rs::dual_use_values_are_withheld_everywhere_or_are_text`,
`scripts/package-check.sh`, `scripts/migrate_via.py`, `docs/MIGRATION.md`, `v4/native-repr/PERF.md`.

Execute (fresh build, `--locked`, private `CARGO_HOME` prefetched):

1. **F1** — your reproducer (a custom `ToHeaderValue + Display` value whose header value is
   flagged, in a direct header, a template, a path and a query) must now fail to compile with
   Concord's diagnostic; then try hard to **bypass** the rule: `Option<_>` / `x?:` values, `each`
   over collections, `Box`/`Arc`/`Cow`/`&` wrappers, aliases, values of enclosing owners used by
   reference (`client.x`), different endpoints using the value differently, `override`/`append`,
   subdomain labels, sensitive key names instead of the marker, `#[cfg]`, renamed dependency. Any
   accepted declaration in which a natively flagged value reaches a Concord-produced output
   (owner `Debug`, `Prepared` `Debug`, `Error`) unredacted is a finding. Also check that ordinary
   `String`/integer/`bool` dual use still compiles and behaves as before, and that a `#[sensitive]`
   custom dual-use value is withheld everywhere.
2. **F2** — `scripts/package-check.sh` with your sandbox's `CARGO_TARGET_DIR`.
3. **F3** — your migration reproducer and the script's `--self-test`.
4. **F4** — the corrected `PERF.md` statements against the archived data (no re-measurement).
5. Run `scripts/verify.sh --msrv --package` once and report its PASS/FAIL lines; look for any
   regression the delta introduced.

Final message = the report: effective settings as observed, what you executed, per-finding
RESOLVED / NOT RESOLVED with evidence, any new finding (P0–P3) with a reproduction, and a verdict
(ACCEPT / ACCEPT WITH FIXES / REVISE). No push, publish, global-setting change or sandbox bypass;
stop every server you start before finishing.


## Supplied document: final_66a4478_report.md

**Verdict: REVISE.** Triage found one P1, two P2s and one P3. No P0 identified.

Effective settings were **`gpt-6.1-sol`, `xhigh`**, observed in the session’s `turn_context`. Execution used workspace-write sandboxing, approval policy `never`, Rust 1.97.1, MSRV 1.85.1 and nightly 1.101.0. [Environment receipt](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/environment-receipt.json).

Reviewed snapshot HEAD: `e374f20a7f59b8a0e0e1144a2008e55429fb5966`. Its commit identifies source `66a44781b159be97b7d50f0392fae42ef350a8b2`; that original object is unavailable locally. All **654 tracked files** still match the snapshot byte-for-byte. Production sources were never modified.

1. **P1 — `Prepared::Debug` leaks a natively sensitive dual-use value.**

   A custom `ToHeaderValue + Display` value returns a flagged `HeaderValue` and feeds a direct header, template, path and query. Request-owner `Debug` redacts it, but `Prepared` prints:

   ```text
   url: "http://127.0.0.1/NATIVE-SECRET?q=NATIVE-SECRET",
   headers: [x-dual: <redacted>, x-copy: NATIVE-SECRET]
   ```

   This uses the same declared value without an explicit application conversion into another value. The declared-sensitive control passes.

   [Reproducer](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/acceptance/tests/contract.rs:333), [observed output](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/acceptance-stable.txt), affected [Prepared formatting](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/crates/concord/src/prepared.rs:76).

   ```bash
   CARGO_TARGET_DIR=target/probe-build cargo test --locked --offline \
     --manifest-path target/probes/acceptance/Cargo.toml \
     --test contract dual_use_prepared_must_withhold_native_flag -- --exact
   ```

   Fails on stable and MSRV. Protect these same-value placements consistently, or reject the unsupported combination.

2. **P2 — the package gate fails with the supplied target directory.**

   `scripts/verify.sh --msrv --package --nightly` exits **1** because [package-check.sh:25](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/scripts/package-check.sh:25) reads `target/package/…`, although Cargo writes archives beneath `target/review-build/package/…`.

   ```text
   tar (child): target/package/concord-macros-4.0.0-alpha.1.crate:
   Cannot open: No such file or directory
   ```

   [Verification receipt](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/verify-review.txt). Using `CARGO_TARGET_DIR="$PWD/target"` makes the packaged-source consumer pass on stable and MSRV; a subsequent locked consumer build also passes. Resolve the archive location from Cargo’s effective target directory.

3. **P2 — migration silently changes a downstream codec.**

   [migrate_via.py:29](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/scripts/migrate_via.py:29) maps the final identifier regardless of namespace:

   ```text
   body b:String via codec::Text
   → text b: String
   ```

   Both resulting format choices compile, but the custom codec produces `CUSTOM:value`; the migrated declaration produces `value`. This contradicts the migration guide’s semantic-preservation claim.

   [Original input](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/migration-original.rs), [reproducer](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/acceptance/tests/migration.rs), [observed output](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/migration-stable.txt).

   ```bash
   python3 scripts/migrate_via.py target/probes/migration-output.rs
   CARGO_TARGET_DIR=target/probe-build cargo test --locked --offline \
     --manifest-path target/probes/acceptance/Cargo.toml --test migration
   ```

   Reproduced on stable and MSRV. Preserve unconfirmed downstream paths.

4. **P3 — `PERF.md` overstates precision and size stability.**

   Archived data contradicts two statements:

   - Optional-absent run 3: mean **900.802 ns**, 95% CI **860.574–977.947 ns**, exceeding the claimed ±1% without a noted exception.
   - Release rlibs: `scale_shared` grows **0.5172%** and `scale_typed` **1.1543%**, exceeding “within 0.5%.”

   Reproduction: `python3 target/probes/check_perf.py`. [Reconciliation](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/perf-reconciliation.txt). Correct the record; no remeasurement is needed.

I personally executed the following:

| Check | Result |
|---|---|
| Fresh verification script, including MSRV/package/nightly | All gates passed except package-check above |
| Workspace tests | 535 passed |
| MSRV runtime/examples/multipart lanes | 416 / 96 / 11 passed |
| Nightly Clone compatibility | 16 passed |
| Independent compiler probes | 31 expected outcomes passed on each of stable and MSRV |
| Independent contract/runtime probes | 7 passed, sensitivity reproducer failed on each toolchain |
| Independent migration probe | Failed as described on each toolchain |
| Four isolated mutations | Baseline pass → intended runtime failure → restored pass, all four |
| Rustdoc builds | All five feature configurations passed |

The independent passing probes confirm native bytes on the raw wire, flag preservation and classification OR-ing, borrowed statics, aliases, snapshots, request Clone with bodies, repeated-value order, owner Debug, body absence/null/empty distinctions, Content-Type precedence and sensitive insertion, drain versus typed unit, independent defaults, custom codecs, form input/output, unread status errors and retained decoding after native execution.

I also reconciled H/F test references against executed logs, inspected expanded generated code, and checked **531 archived Criterion means**, paired compile measurements and allocation records. **I did not rerun benchmarks.** Prior campaign reviews, measurements and fresh-use records remain inspected historical evidence.

README, Quickstart, Author Guide, Migration and generated HTML were inspected; repository documentation examples ran through verification. Visual rustdoc inspection remains unverified: browser access was unavailable and Chrome aborted on its sandbox setup. No sandbox protection was disabled. The requested prior receipt, `v4/native-repr/evidence/verify_66a4478.txt`, is absent, so comparison with it was impossible.

No push, publish or global-setting change occurred. **Review cleanup remains unresolved:** the documentation server on **`127.0.0.1:44975`** survived its exec-session interruption. The session is now unavailable, and its process is invisible to sandbox-local `ss`/`fuser`; it requires host-side termination. All probe-test servers terminated. [Integrity and cleanup receipt](/home/canarit/projects/concord_v4_native_work/snapshots/final-66a4478/target/probes/source-integrity.json).

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
