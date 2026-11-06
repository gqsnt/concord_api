# SDK05 author evidence

Baseline: Concord `e04236ca`. Role: SDK author only; no consumer `tests/first_use.rs`
was created. Facts below describe attempts, diagnostics, changes and observed results.
Paths/line numbers are relative to this SDK unless prefixed with the repository path.

1. **Path/query declaration stayed small; no execution reconstruction.**
   `src/lib.rs:19` declares both routes, date and optional provider filter. The shared
   adapter at `src/lib.rs:78` accepts public `Endpoint<Rate>`, calls `prepare().receive()`
   once and uses the generated operation identity. No native `execute`, private module,
   manual URL composition or handwritten operation name appears in SDK implementation.
   Reproduce with `tests/protocol.rs:22`, `:43`, `:63`, and `tests/workflows.rs:50`:
   latest omission, historical query order, path/query injection encoding and added optional
   filters/unknown fields pass. **Cause: Concord, positive.**

2. **Inline date parsing needed an annotation at the generated call boundary.**
   The first integration-test compile failed with E0283, “cannot infer type of the type
   parameter `F` declared on the method `parse`”; the diagnostic cited `IntoValue<Date>`.
   At `tests/protocol.rs:46`, replacing `.parse::<Date>()` with `.parse()` reproduces it;
   the same occurred in two workflow calls. Supplying `::<Date>` fixed all three. The
   README already used `let date: Date`, which compiled. No wrapper or library change was
   made. **Cause: Concord's generic argument ergonomics interacting with Rust inference.**

3. **Typed provider errors and owned metadata still need an SDK adapter.**
   `src/lib.rs:78` branches on status, captures the head, checks error media, bounds the
   body and parses the problem. `src/error.rs:9` owns metadata; `src/error.rs:115` preserves
   a typed outer error; custom Debug implementations omit response text/header values.
   Public `Endpoint` shares the work across both operations, but the two inherent
   forwarding methods at `src/lib.rs:59` and `:69` make `.fetch()` discoverable without a
   trait import. Generated `.send()` remains present with different error/result semantics.
   Reproduce through `tests/protocol.rs:178`, `:219`, `:236`, `:318`, `:420` and the README
   error example. All pass, including malformed error documents and success-body failures.
   **Cause: SDK/provider policy for error interpretation; Concord leaves owned metadata
   and typed-error adaptation to SDKs.** The adapter boilerplate is the closest this small
   slice came to reconstructing shared library work; HTTP execution was not rebuilt.

4. **Bounded error reads were public, but the recipe points toward older plumbing.**
   Repository `docs/RECIPES.md:19` describes `into_response` plus body-only parsing and
   `response.chunk()` for a limit. The generated `Received` rustdoc documents
   `bytes_limited` instead. The SDK uses it at `src/lib.rs:85`; no chunk loop or native
   escape was needed. `tests/protocol.rs:279` proves exactly 16 KiB is accepted and one byte
   beyond is a typed Limit failure preserving 429 and headers; `:339` proves read failure
   preserves 503. **Cause: docs discoverability; Concord primitive positive.** Success
   uses finite `decode()` and remains unbounded in bytes, explicitly documented in
   `README.md`; this slice did not add a second body-consumption framework.

5. **Exact JSON numbers required a field adapter, not a new HTTP codec.**
   `src/model.rs:25` receives RawValue, requires a numeric token, invokes BigDecimal's
   parser and checks positivity. The standard Concord Json codec remains in the declaration.
   `tests/protocol.rs:91` passed ten exact-value cases, including >2^53, a long decimal
   mantissa and 1e±400; `:119` rejected sixteen invalid/nonpositive cases. No decimal
   truncation or float intermediary was introduced. **Cause: SDK numeric policy and JSON
   representation; mature external parser is appropriate.** The small adapter enforces
   the provider's number type, rather than accepting permissive quoted decimal strings.
   Clippy initially flagged `value <= Decimal::from(0)` with `cmp_owned` at
   `src/model.rs:37`; using BigDecimal's direct integer comparison (`value <= 0`) removed
   the unnecessary allocation and the warning.

6. **The website and OpenAPI express errors differently.**
   Website Errors shows `status` plus `message`; OpenAPI 2.1.1 error schemas describe
   `message`. `src/error.rs:43` makes body status optional, while HTTP status is retained
   separately. `tests/protocol.rs:178` passes both documented forms; `:219` keeps HTTP 422
   even with synthetic body status 200. **Cause: provider/docs variation**, handled in the
   SDK without requiring callers to interpret raw JSON.

7. **Build-environment interventions, unrelated to the provider contract.**
   The initial unlocked package test updated `Cargo.lock` with three dependency packages,
   then failed writing `/home/canarit/.cargo/registry/cache/.../num-bigint-0.4.8.crate`:
   “Read-only file system (os error 30)”. A writable registry home was created at
   `/tmp/frankfurter-sdk-cargo-home`: its index was copied, existing cache/source entries
   linked for reads, and new crates downloaded there. The required shared `target-a`
   directory and five build jobs were retained.

   A subsequent build compiled the library but both doctests failed with E0460,
   “found possibly newer version of crate `stable_deref_trait`”, listing multiple shared
   target metadata files. No source error was reported for the library. Setting
   `RUSTFLAGS='-C metadata=frankfurter_sdk05'` separated this SDK's compiler artifacts;
   subsequent tests and both doctests passed. No shared cache files were deleted and
   no Concord sources were changed. The precise source of the metadata collision was not
   established. **Cause: application/build environment.** Reproduction inputs are the
   dependency additions in `Cargo.toml:14` and the commands below; the cache failure is
   environment-dependent, not claimed to reproduce on an ordinary writable Cargo home.

## Verification commands and results

Every Cargo invocation used the required `CARGO_BUILD_JOBS=5` and
`CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a`.
After the cache interventions, package verification also used the following per-command
environment (no environment/config files in the repository were changed):

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a CARGO_HOME=/tmp/frankfurter-sdk-cargo-home RUSTFLAGS='-C metadata=frankfurter_sdk05' cargo test --locked -p concord-sdk-frankfurter
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a CARGO_HOME=/tmp/frankfurter-sdk-cargo-home RUSTFLAGS='-C metadata=frankfurter_sdk05' cargo clippy --locked -p concord-sdk-frankfurter --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a CARGO_HOME=/tmp/frankfurter-sdk-cargo-home RUSTFLAGS='-C metadata=frankfurter_sdk05' cargo fmt -p concord-sdk-frankfurter
```

Observed tests: **17 protocol + 4 workflow + 2 compiled README doctests passed**; zero
unit tests, failures or ignored tests. Loopback tests explicitly shut down their servers.
Clippy with `--all-targets -- -D warnings` passed, and package formatting completed.
All fixture/protocol scenarios are offline evidence, not live provider verification.
