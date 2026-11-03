# Concord fresh-use trial (fresh-sonnet): report

Both tasks are done and all 17 tests pass, plus fmt and `clippy -D warnings`. I read only README.md, docs/*.md, examples/*/src, examples/*/README.md, the `//!` crate docs and the generated rustdoc, until both tasks were finished. After that I read the implementation only through compiler error output (paths shown in notes) and one grep of `v4/` (see "Disclosures").

## What I built

Crate: `/home/canarit/projects/concord_v4_modular_work/proposals/fresh-sonnet/` (own `[workspace]`, edition 2024, `rust-version = "1.99.1"`, concord by path, `Cargo.lock` copied from the snapshot and then pruned by cargo to this crate's tree).

- `Cargo.toml`: dependencies are concord, serde and serde_json. tokio is a dev-dependency.
- `src/lib.rs` contains:
  - `AcmeV2` implements `Decode<Versioned<T>>`:
    - `check_content_type` requires exactly one `Content-Type` whose essence is `application/vnd.acme.v2+json` (any case) and whose charset, if given, is UTF-8.
    - `context` requires exactly one `Acme-Version` equal to `2` and returns it as `Context = SchemaVersion`.
    - `decode_body(version, bytes)` delegates to `<Json as Decode<T>>::decode_body` and wraps the result with the version.
    - `ACCEPT = Some("application/vnd.acme.v2+json")`.
  - `Ndjson` implements `Decode<Vec<T>>`. Errors are `CodecError::new("an NDJSON record is not valid").at("record", n).at("line", n).at("column", n).with_source(serde_err)`.
  - `X_ACME_KEY` (the sensitive credential `HeaderName`) and `KEY`, an alias constant of it.
  - The client `Acme(#[sensitive] header(X_ACME_KEY) key)` has `response AcmeV2;` on the client and four endpoints:
    - `widget` inherits the client's `response`.
    - `gadget` has its own `response AcmeV2;`.
    - `audit` has `headers { override KEY = admin_key; }`.
    - `export` has `response Ndjson;`.
  - A unit test calls `Acme::validate_header_names()`.
  - A `HEAD_CHECKS` counter exists only as test scaffolding.
- `tests/support/mod.rs`: a tiny tokio TCP server that records requests, sends scripted replies, and can withhold the body after the head.
- `tests/format.rs` (8 tests): good decode on both endpoints with `Accept` and path asserted on the wire, wrong media type, wrong versions (3, 1, "two", missing, repeated), refusal before the body, a control test, NDJSON, and `IgnoreContentType<AcmeV2>`.
- `tests/status_first.rs` (1 test): a 500 is a `Status` error and the format is never consulted.
- `tests/header_names.rs` (5 tests): a client `Broken` with four mistakes, a comparison of the check with `prepare()`, and the wire override.
- `tests/head_unit.rs` (2 tests): head checks through `ResponseHead::new`, with no server.

## Commands and results

All commands were run with `CARGO_TARGET_DIR=$PWD/target` in the crate directory.

- `cargo +1.99.1 --offline doc -p concord --no-deps` finished in 8 s.
- `cargo +1.99.1 --offline build` succeeded on the first attempt, with no compiler errors.
- `cargo +1.99.1 --offline test` gives 1 unit + 8 + 2 + 5 + 1 integration tests, all ok.
- `cargo +1.99.1 --offline fmt --check` is OK and `cargo +1.99.1 --offline clippy --all-targets -- -D warnings` is clean.

Observed Task 1 behaviour:

- **Wrong media type:** `GET Acme.widget: response metadata not accepted: the response Content-Type is not application/vnd.acme.v2+json`.
  - `kind()` is `Metadata` and `status()` is `None`.
  - `error.response().unwrap().headers()["content-type"]` is readable.
  - `into_response().text()` returns the full body.
- **Wrong version:** the same shape, ending in `unsupported Acme-Version (version 3)`.
  - Non-numeric: `Acme-Version is not a number`.
  - Missing or repeated: `the response needs exactly one Acme-Version`.
- **Refusal before the body is read:**
  - The server sent a head announcing a 1000-byte body and then nothing. With `Acme-Version: 3` the call returned `Metadata` immediately.
  - With a good head, the same server made the call hang. I used that as a control (`control_a_good_head_with_a_withheld_body_does_wait`).
- **500 with a wrong media type and a wrong version:**
  - It gives `GET Acme.widget: HTTP status 500 Internal Server Error`, `kind()` `Status`, and the body is still readable.
  - `HEAD_CHECKS` stayed 0, and it went to 1 after a 2xx control. This test is in its own file so parallel tests cannot disturb the counter.
- **NDJSON:**
  - A failing record 2 on line 3, with TOPSECRET inside, shows `GET Acme.export: response not decoded: an NDJSON record is not valid (record 2, line 3, column 37)`.
  - The secret is absent from `Display` and `Debug`. `codec_error().get("record")` is `Some(2)`, and `source_unredacted()` is available.
- **`IgnoreContentType<AcmeV2>`:** a mislabelled `text/plain` response with version 2 decodes, and the `Accept` suggestion is kept. With version 3 it is still `Metadata`. This matches the docs.

Observed Task 2 behaviour. The correct client's `validate_header_names()` is `Ok`. I then broke `audit` in place, changing `override KEY = admin_key;` to `KEY = admin_key;`. The unit test failed with this (the `unwrap()` version printed the Debug struct dump, so I switched to `panic!("{errors}")`):

```
Acme.audit: `KEY` on `Acme.audit` sets a header that an earlier declaration already sets (both name the same header); use `override` or `append`, or `remove` it first (earlier: `X_ACME_KEY` on `Acme`)
```

I restored it afterwards, and `tests/header_names.rs` keeps a permanent `Broken` client.

- `problems()` has one entry per broken endpoint, in declaration order. `declaration()` is `` `KEY` on `Broken.set_again` `` and `earlier()` is `` Some("`X_ACME_KEY` on `Broken`") ``.
- The same set of mistakes also reports:
  - ``Broken.override_nothing: `override X_OTHER` ... changes a header that no enclosing owner sets``
  - ``Broken.managed: `HOST` ... names a header field that reqwest/hyper manage; it cannot be declared``
- `prepare()` on the broken endpoint gives `ErrorKind::Build` with the same text, and it also fails when the value is optional and absent (names are compared before values).
- On the wire, `audit` sends `x-acme-key` once with the admin value. Neither key appears in `Debug` output, and the prepared header is flagged sensitive.

## Documentation: unclear, missing or wrong

1. **A payload-generic format cannot be called as `AcmeV2::check_content_type(&head)`.**
   - I hit E0283: `cannot infer type for type parameter T ... required for AcmeV2 to implement Decode<Versioned<_>>`. It does not name the fix, which is `<AcmeV2 as Decode<Versioned<Widget>>>::…`.
   - CUSTOM_FORMATS §9 shows a helper `verdict<F: Decode<T>, T>` that avoids the problem silently, but never says this.
   - I expected one sentence there: "if your format is generic over the payload, name the payload in the trait path".
2. **There is no help for parsing a vendor media type.**
   - §4 says "parsing a media type is the format's job (`concord-core` has no parser)".
   - §6 says in passing that the standard formats use the `mime` crate, but it is not re-exported.
   - I hand-rolled essence, case and charset handling. A short snippet in §5, or a re-export or helper, would help.
3. **`HeaderNameErrors` is hard to read when unwrapped.**
   - The one-liner in AUTHOR_GUIDE §4.9 (`Vault::validate_header_names().unwrap()`) panics with the long single-line Debug struct dump.
   - The guide's own `main` uses `panic!("{errors}")`, which is readable.
   - §4.9 never shows the Display text, so I had to produce it myself.
   - The guide lists `problems()`, `endpoint()`, `declaration()` and `earlier()` but not `reason()`, which exists.
4. **`with_base_url` replaces the declared base including its path.**
   - My first test run failed with `/widgets/7` instead of `/v2/widgets/7`. The generated rustdoc ("Declared paths are appended to its path") and the QUICKSTART `format!("{}/api", …)` imply this.
   - Nothing says outright "include the base path". One sentence in QUICKSTART §4/§5 or CALLER_GUIDE §1 would do it.
5. **No guidance on exposing head state to the caller.**
   - A `Decode<T>` returns only `T`, so keeping the version visible needs a wrapper output type. I used `Versioned<T>`.
   - The docs' example hides the version inside an id offset. A sentence about the wrapper pattern would help. I think this is the intended pattern, but that is a guess.
6. **`validate_header_names` covers less than its name suggests.**
   - It only covers endpoints whose chain writes a native constant.
   - `zz_f`, a provoked test with a literal `"x-k"` below a native `X_K`, compiled fine, so only this check or `prepare()` catches it.
   - This is documented in §4.8/§4.9 and in the generated rustdoc, which lists the checked endpoints.

Everything else I relied on held up. The Decode order table in CUSTOM_FORMATS §4 matched what I saw: Metadata for steps 1 and 2 with the response unread, Decode for step 4, and Status before the format. The doc claims about `CodecError` and `IgnoreContentType` also held. The §5 `Versioned` example and the Accept table were the right templates.

## Compiler errors

I hit none unprovoked in the library. The one I hit in a test is the E0283 above. I then provoked these to judge message quality:

- `CodecError::new(format!(..))`: E0308 `expected &str, found String`. Clear.
- `HeaderName::from_static("Acme-Version")`: E0080 `HeaderName::from_static with invalid bytes`, pointing at the line. It does not say "lowercase", but §4.8 does.
- `-> Widget` with `response AcmeV2;` when the format implements `Decode<Versioned<_>>`: E0277 `the format AcmeV2 cannot decode a Widget response`, with a pointer to the `Decode<Versioned<_>>` impl. Excellent.
- `-> Versioned<Widget>` with no format: `output Versioned<Widget> has no format: say response F; ...`. Excellent.
- `Decode` impl missing `context`: E0046 with a suggested stub. Fine.

## Time and attempts

Timings are by the shell clock, which probably undercounts: it started at 05:36:58 and ended at 05:43:05, about 6 minutes including reading the docs. A human would need much longer.

| Part | Shell-clock time | Attempts |
|---|---|---|
| Reading the docs | about 1 min | – |
| Task 1 | about 3 min | library compiled first time. Tests ran twice: one failure from the base-path mistake, one fix split out for the 500 test. |
| Task 2 | about 2 min | passed first time. Break-and-restore was done on purpose. |
| Extras | – | head-level tests: 1 compile error (E0283), then fixed. |

## Names

- **`ErrorKind::Metadata`: misleading.** It means "a 2xx whose head the format refused", with nothing to do with decoding. "Metadata" reads like request or body metadata. `Head` or `Rejected` would be clearer. The docs explain it well, but the name does not. Note that `status()` is `None` while `response()` carries a status.
- **`Decode::context`: the least self-describing method.** It is really "check the rest of the head and extract per-response state". Something like `check_head` returning `Context` would say more. `check_content_type` and `decode_body` read fine.
- **`validate_header_names`: slightly misleading.** It checks the composition of header chains (duplicates, override or remove with nothing to act on, managed fields), not whether names are valid. It is also limited to native-constant chains.
- **`IgnoreContentType`: accurate, but the contract is by convention.** It skips only `check_content_type`. A format whose author puts a media check in `context` will not have it skipped, and a non-media check in `check_content_type` will be skipped. It worked as documented for `AcmeV2`.
- **`CodecError`: mildly odd for a head refusal.** No codec runs in that case, yet `error.codec_error()` returns it for `Metadata`. `FormatError` would fit better.

## What I would change first

1. Make `HeaderNameErrors`' Debug print the Display text, or change the §4.9 one-liner to `panic!("{e}")`. Add a sample Display line and mention `reason()`.
2. Add a "payload-generic formats need `<F as Decode<T>>::`" note and a small media-type-parsing helper or snippet to CUSTOM_FORMATS.
3. Rename or alias `ErrorKind::Metadata` and `validate_header_names`, and document the wrapper-output pattern for head state.
4. Add one QUICKSTART sentence that `with_base_url` replaces the base path too.

## Disclosures

- A post-task grep of `v4/` for naming rationale surfaced two lines from `v4/finalization/trials/sonnet.md` and `terra-xhigh.md` in its output. I did not open those files or use them.
- The compiler notes showed paths into `concord-core/src/__private.rs` and `http-1.5.0`. I did not read them.