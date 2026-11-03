# Final semantic and architecture review: Concord V4 modular codecs, candidate 9f8939f (side Opus, frozen)

Snapshot reviewed: `/home/canarit/projects/concord_v4_modular_work/snapshots/final-sem-opus`. Its tracked files are unchanged; `git status` is clean after my builds. Probes are under `/home/canarit/projects/concord_v4_modular_work/proposals/final-sem-opus/{probe,probe-seal}`, built into `.../proposals/final-sem-opus/target` with `cargo +1.99.1 --offline`. I did not read `snapshots/final-exec-sol` or `codex`. This report makes no acceptance claim.

## Blocking

None found. The required behaviour exists in code:
- the four-crate split;
- formats as type paths;
- the three-step `Decode`;
- `Metadata` errors that keep the response unread;
- `Accept` as a fallback added after header composition;
- a presence-free structural engine shared by `HeaderPlan::compose` and `NameCheck`;
- a sealed `NativeKey`;
- `CodecError` built from static labels.

## Should fix

**S1. The H08 sealing regression test proves nothing since the crate split.**
- `crates/concord/tests/ui/native_key_sealed.rs` imports `concord::__private::NativeKey`. That path stopped existing at the split, which left the facade with only `__macro`.
- The expected `.stderr` is now `E0432 unresolved import concord::__private`.
- History, from `git show` in `wt/dev`:
  - at `3a836e7` and `df37383` the expected error was `E0277 ... Shifting: concord::__private::sealed::Sealed is not satisfied`;
  - commit `7ec1f4b` overwrote it with the import error.
- The sealing itself still holds. `probe-seal/src/lib.rs` implements the same counter-based `Shifting` through `concord::__macro::concord_core::__private::NativeKey` and gets `E0277 ... __private::sealed::Sealed is not satisfied`.
- The problem: deleting `: sealed::Sealed` from `__private.rs:737` would no longer fail any test.
- These records cite the broken test as their evidence: TEST_MATRIX H08 "met", DECISIONS MC04 ("rejected now") and REVIEW_TRIAGE R3.
- `to_header_value_sealed.stderr` was re-blessed correctly; this is the only degenerate UI snapshot.
- Fix: import through `concord::__macro::concord_core::__private` (or depend on `concord-core`) and re-bless, so the snapshot shows `Sealed`.

**S2. `validate_header_names()` claims more than it proves about `prepare()`.**
- These places say it applies "the rules `prepare()` applies before any value is converted" and that "a problem reported here is the error that endpoint's `prepare()` returns for every request":
  - `names.rs:9-17`;
  - the generated rustdoc (`emit.rs:2008-2016`);
  - AUTHOR_GUIDE §4.9 (l.563);
  - MC14;
  - the header comment of `validate_names.rs`.
- The generated `prepare()` runs in this order, and SEMANTICS §6 l.243 states the same order:
  1. `let __body = #body_code;` (the encoder runs) at `emit.rs:2297`;
  2. labels, host, segments and query (`Display` runs);
  3. only then `HeaderPlan::compose` (`emit.rs:2183`).
- Reproducer (probe P1): client `V(header(X_A) a: String = ..)` with three endpoints that each set `X_A = "again"`.
  - The validator reports all three.
  - `prepare()` returns `path segment {id} could not be formatted` for a failing `Display`.
  - It returns `body: refused by the probe encoder` for a failing encoder; that encoder ran, which consumes a one-shot payload.
  - Only the plain endpoint returns the header error.
- What is true:
  - the verdict is presence-free and identical to `compose`'s, since both call `structure`;
  - no request of a reported endpoint can ever be built;
  - `Ok(())` means `prepare()` never fails on header names.
- Fix, either:
  - run the structural check first in `prepare()`. It needs only the shapes `static` and the key accessors, it makes the claim true, and it stops encoders from running for requests that can never be built; or
  - narrow the wording to "every request fails to build; this is the error unless an earlier step fails first".

**S3. The `Decode` docs say a head-based decision cannot happen after the body is read; it can.**
- The claim appears in:
  - `codec.rs:71-72`: "a decision that depends on the head cannot be made after the body was consumed";
  - CUSTOM_FORMATS l.140: "this way a refusal always can";
  - MIGRATION l.225 and AUTHOR_GUIDE l.920: "`decode_body` never sees the head".
- `Context` carries data taken from the head into `decode_body`.
- Probe P2: the format `Late` (`Context = u64` parsed from `x-version`) refuses in `decode_body`. The result is `ErrorKind::Decode`, `response()` is `None` (body consumed), and `Display` is "response not decoded: unsupported version (version 3)".
- The project's own `JsonOrEmpty` legitimately makes a status-dependent decision in `decode_body`.
- Fix the wording: the split lets refusals placed in `check_content_type`/`context` keep the response unread; it cannot stop a format from moving a head-derived refusal later.

**S4. MIGRATION understates the `Accept` change for clients with a configured default `Accept`.**
- MIGRATION l.104-106 says: "Before, Concord sent none and reqwest added its `*/*`."
- reqwest fills only header names the request lacks. So an application whose `reqwest::Client` has `default_headers(ACCEPT: vendor+json)` silently sends `application/json` on every typed `Json`/`Form` endpoint after upgrading. This matters for APIs that select their version through `Accept`.
- `accept.rs::the_wire_and_the_client_defaults` proves this replacement, and AUTHOR_GUIDE §5.8 states the rule. The migration "What you may notice" section is where it must appear.
- Fix: add the case there with its remedy: declare the `Accept` in `headers`, or use a format wrapper that suggests nothing.

## Minor

- **M1. Some docs still say error output is purely static.**
  - `error.rs:11-13` says `Display`/`Debug` have "only static information ... never ... header values", and CALLER_GUIDE §6 says "only static text".
  - Numeric positions are dynamic. The project's own `Versioned` example (`.at("version", v)` from `x-api-version`) and `checks/consumers/core-only` (`.at("record", x-record)`) put response header values, as numbers, into `Error` `Display`.
  - SEMANTICS §8 is accurate.
  - `error.rs:13` also points to "the crate documentation for the exact scope", but `concord_core`'s crate docs have no such section.
- **M2. `IgnoreContentType<F>` is only as narrow as documented when `context` never refuses on Content-Type.**
  - `codec.rs:124-127` suggests a charset as `Context`; `codec.rs:138-141` puts every Content-Type check in `check_content_type`.
  - A charset-aware format must therefore either refuse a missing or malformed field in `context`, which the adapter does not skip, or parse the field twice.
  - Fix: state that `context` may read Content-Type for state but must not refuse on it.
- **M3. The RECIPES error-document text is stale.** RECIPES l.27-29 says "as written, the recipe would report [a Metadata error] as a Problem…". `examples/native/src/errors.rs` already guards with `error.status().is_none()`. The "How" bullet also says `into_response()?` lets any other error propagate unchanged, but it returns the 2xx response of a `Metadata` error.
- **M4. Strict `Json` refuses some media types RFC 9110 allows.** These are refused because the `mime` crate rejects them (probe P6), not because they are malformed:
  - `application/json ; charset=utf-8` (whitespace before `;`);
  - `application/json;;charset=utf-8` (empty parameter).

  The label is "not a media type". The Json doc says "malformed"; the truth is "rejected by `mime`". Document this, or trim whitespace before parsing.
- **M5. A native `Accept` edit now produces two `Accept` lines.** After `into_parts`, `RequestBuilder::from_parts(..).header(ACCEPT, v)` appends to the inserted suggestion (probe P5). MIGRATION does not mention this for native-exit users.
- **M6. Small stale or incomplete docs:**
  - `concord-macros/src/lib.rs:1-2` says the first token is "the path of the `concord` crate"; it is `$crate::__macro::concord_core`;
  - MAINTAINERS l.187 still calls `v4/refinement/` "the current refinement campaign";
  - core `lib.rs` says core depends on "reqwest and `bytes`"; it also depends on `url` and `percent-encoding`;
  - the facade's `extras` feature bullet (l.202) omits `IgnoreContentType`;
  - the facade does not re-export `CODEC_ERROR_CONTEXT`, which `CodecError::at` refers to.
- **M7. A stray file.** An empty file named `A` sits at the repo root, tracked since `df37383`.
- **M8. `concord-macros` has no README or `readme` field.** The handoff's packaging section asks all four crates for README/docs links.

## Notes

- **Boundaries hold.** `cargo tree -e normal` per package:
  - core → `http`, `reqwest` only;
  - macros → `http`, `mime`;
  - extras (all features) → core, `mime`, `serde*`;
  - facade with no default features has no extras and no `serde_json`.

  No build scripts. The macro has no codec table; removed spellings are parsed only as generic paths for diagnostics. Core mentions extras types only in prose (the `Decode` docs name `IgnoreContentType`; an `error.rs` example names `Json`).
- **The hidden API is honest about itself.** `__private` and `__macro` are documented as unstable, and the exact `=` pins are explained in the facade `Cargo.toml` and MAINTAINERS. The extras→core exact pin is stricter than needed (extras uses public API only) but deliberate.
- **The `Accept` behaviour matches the docs in every row I traced:**
  - the suggestion is added in `Parts::finish` after composition;
  - literal chains use `slot.removed`; native chains use `HeaderPlan.accept_removed` (decided by the last operation);
  - the suggestion never enters `HeaderPlan`, the literal-chain slots or `NameCheck`;
  - `remove` without a target is still an error.

  `each` over an empty list counts as no authored value, so the suggestion is sent (probe P4). That follows the rule but deserves a row in the §5.8 table.
- **The Json charset policy is a deliberate, documented choice.** Strict Json refuses `charset=utf8` and any non-UTF-8 charset, although RFC 8259 §11 says a charset parameter has no effect on compliant recipients. The only escape is `IgnoreContentType`, which also drops the media check.
- **`ContentTypeField`'s Debug prints its value** (probe P3b), unlike `ResponseHead`'s Debug. Harmless for Content-Type.
- **Teachability:**
  - `context` is required even for stateless formats, while `check_content_type` accepts by default. So wrappers that only forward `decode_body` silently lose `Json`'s strictness (documented in CUSTOM_FORMATS §3/§8).
  - The body-only route needs a `Context` the caller can construct.
- **Ownership is sound:**
  - `Decoder` stays `Copy` through a function pointer;
  - there is a `Box` only for non-zero-sized contexts;
  - `Context: Send + 'static` keeps futures `Send`;
  - the response is borrowed only for the head step, then moved.
- **The path deferral is sound:**
  - ordinary interpolation stays one segment;
  - the maintained native recipe is honest about losing redaction and about the `url` crate's different encoding;
  - the deferral is recorded in REVIEW_TRIAGE and TEST_MATRIX, though there is no MC decision row.
- **No scope drift:** one adapter, no codec registry, no second interpreter (the validator reuses `structure`). The structural rules still exist twice (the macro composes literal-name chains, core's `structure` composes native ones); that predates this increment and is cross-checked only at the `HeaderPlan` level.

## What I executed and what I reasoned

**Executed:**
- `cargo tree` on five package/feature combinations;
- probe crate `probe/tests/probes.rs`, 7 tests passing:
  - P1: validator problem vs `prepare()` error;
  - P2: head-derived refusal after the body is read;
  - P3: Json charset spellings;
  - P3b: `ContentTypeField` Debug;
  - P4: `each` over an empty `accept`;
  - P5: native `.header(ACCEPT)` duplicates;
  - P6: RFC 9110 media-type forms;
- `probe-seal`: compile-fail, confirming the seal holds;
- `git show` of `native_key_sealed.stderr` across `3a836e7`, `df37383`, `7ec1f4b`, `dc7f134`, `9f8939f`.

**Reasoned from source and docs (not run):**
- every other statement in this report;
- the crate graph beyond `cargo tree`;
- the Accept truth table beyond P4/P5 (relying on the candidate's own `accept.rs`);
- M2, the RFC 8259 note, the migration and docs consistency findings;
- the path deferral.

I did not run the candidate's gate (`scripts/verify.sh`), the UI suite, or any Windows or packaging checks.
