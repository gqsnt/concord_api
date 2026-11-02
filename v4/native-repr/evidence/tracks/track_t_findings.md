# Track T — two localized review findings (multipart recipe, conditional `Clone`)

- **Model:** Claude Opus 5.5 (`claude-opus-5-5`). The reasoning-effort setting is not visible to me
  from inside the session, so it is not reported.
- **Worktree / branch:** `/home/canarit/projects/concord_v4_native_work/wt/findings`, `native-repr-findings`
  (base `01774a9`). Not pushed. `crates/concord` and `crates/concord-macros` are unchanged.

## Commits

| Commit | Summary |
|---|---|
| `03e5671` | feat(examples): new `examples/multipart` crate (reqwest `multipart`, independent-parser wire tests); hand-written MIME writer, `FilePart`, `upload` endpoint and their 3 tests removed from `examples/native`; RECIPES / native README updated |
| `21e8481` | test(checks): new `checks/clone-compat` crate: 4 runtime tests + 6 compile-fail doctests, each with a compiling twin |
| `a0ce0ef` | ci: `verify.sh` feature-isolation step, multipart crate tested alone (1.97.1 and `--msrv` 1.85), clone-compat in the MSRV list, opt-in `--nightly`; CI: the same checks plus a `nightly-clone` job with `continue-on-error: true`; MAINTAINERS rows |
| `17bace6` | fix(verify): delete old `target/verify-*.log` at start so the summary covers only the current run. Before this, stale `--msrv`/`--nightly` logs were reported. |

Cargo.lock gains only `concord-example-multipart`, `concord-clone-compat`, `mime_guess 2.0.5` and
`unicase` (the last two come from reqwest's `multipart` feature). Both were already in the local
registry, so everything was resolved `--offline` and nothing was fetched from crates.io.

## Finding 1: multipart recipe

### Design
- **Crate:** `examples/multipart` (`concord-example-multipart`) depends on `reqwest = { workspace = true, features = ["multipart"] }`. It is the only crate in the workspace that names reqwest's `multipart` feature. Concord has no multipart code or policy.
- **Declaration** (kept minimal so a later `via` migration is mechanical): `POST upload(folder: String) -> Stored via Json { path "folders/{folder}/files"; }` on a client with a `#[sensitive] header("authorization")` root value.
- **Route** (`upload`):
  ```rust
  let (client, request, decoder) = api.upload(folder).prepare()?.into_parts();
  let result = reqwest::RequestBuilder::from_parts(client, request).multipart(form).send().await;
  decoder.finish(result).await
  ```
  I chose `from_parts(..).multipart(form)` because reqwest's builder then writes the boundary `Content-Type` and, when every part has a known length, the `Content-Length`, using public API only. The alternative is to put the body on the `Request` yourself. That needs reqwest's `stream` feature (`Form::into_stream`), and the body is sent chunked because `Form::compute_length` is `pub(crate)`.
- **Helper** `file_form(text, files)` builds the form with `Form::percent_encode_attr_chars()`. reqwest's default name encoding (`percent_encode_path_segment`) leaves `\` raw inside `name="..."`, so a recipient reads `file\name` as `filename`. This is the same bug class as milestone finding F-01, and a test pins the default behaviour to justify the choice. With attr-char encoding, plain names stay `name="x"` and every other name is sent as `name*=utf-8''<pct>`.
- **Errors:** a non-2xx answer becomes `ErrorKind::Status` and `into_response()` still returns the unread body. Transport failures go through `finish` and become `ErrorKind::Transport`. Bad JSON becomes `Decode`. An invalid declared header becomes `Build` before any I/O.
- **`examples/native`:** `upload_file`, `multipart_body`, `header_safe`, `FilePart`, the `upload` endpoint and the 3 multipart tests are gone. In their place is a short comment in `bodies.rs` pointing to `examples/multipart`.

### Tests (`examples/multipart/tests/upload.rs`, 11 tests)
The loopback server and multipart parser in `tests/support/mod.rs` use no HTTP or MIME library. The parser implements RFC 2046/7578 framing (no preamble, `CRLF--boundary` delimiters, `--` close followed only by CRLF), RFC 9110 quoted strings and RFC 8187 `ext-value` decoding. The server also requires a `Content-Length`.

The tests cover:
- several parts in order: text fields, a repeated `tag` field, csv and png files; the declared path `/v1/folders/a%20b%2Fc/files`, the authorization header and the `Content-Length` are all checked
- field names: `"`, `\`, a trailing `\`, CR/LF, `/`, `%`, non-ASCII, and the wire forms of plain names
- file names: `"`, `\`, a trailing `\`, `two\r\nlines\n.txt`, non-ASCII; plus the exact wire form of the CR/LF name, `filename="two\<CR>\<LF>lines\<LF>.txt"` with a backslash before each CR and LF, so there is never a CRLF pair
- bytes: all 256 byte values, a boundary-shaped line and an empty part; the real boundary appears in no part
- part content type, including parameters; an invalid media type is rejected before I/O
- native `Part`s used directly
- the ambiguity of the default name encoding
- error cases: Status (413, unread body readable), Decode, Transport (server hangs up), Build

### Feature isolation evidence
`cargo tree --locked -e features -i reqwest <args> | grep -c multipart`:
```
-p concord                          0
-p concord --no-default-features    0
-p concord-example-native           0
-p concord-example-multipart        4   (├── reqwest feature "multipart" └── concord-example-multipart)
--workspace                         4
```
These are the reqwest features actually compiled (`cargo <cmd> --no-run --message-format=json`):
```
test -p concord                         [... 'default','default-tls','http2','rustls','system-proxy']   (no multipart)
test -p concord --no-default-features   []
test -p concord-example-multipart       [..., 'multipart', ...]
test --workspace                        [..., 'json', 'multipart', 'query', ...]   ← unification
```
The workspace build already unifies `json`, `query` and `multipart` into reqwest. That is why the
check has to run on separate consumers. `verify.sh` now has a permanent `feature-isolation` step
(cargo tree only, no build). It fails if multipart reaches `concord`, with or without default
features, or `concord-example-native`. It also asserts that multipart does reach the multipart
crate, so a broken grep pattern cannot pass silently. `test-multipart-alone` (1.97.1) and
`msrv-multipart-alone` (1.85) build and test that crate with `-p` alone. In CI, the isolation
check runs in the `test` job and the 1.85 test runs in the `msrv` job. That costs one small extra
build of reqwest and concord.

## Finding 2: conditional `Clone` compatibility

### Design (`checks/clone-compat`, depends on `concord` with `default-features = false`; no I/O)
- **Runtime tests** (`tests/clone.rs`, 4 tests):
  - one endpoint with a required path value, an optional query value (`note?`), an initialized header value (`= fresh_key()`), an `each` collection (`Vec<C>`) and a body (`via Raw`, a downstream `Encode`), all of them `Clone`:
    - `.clone()` compiles
    - the initializer runs once at the call and never again on clone or prepare (thread-local counter)
    - both copies `prepare()` to the same method, URL, headers and body
    - clones are independent: `set_*`/`maybe_*` on the clone leave the original unchanged
  - a bodyless request is `Clone`
  - **canary:** the same shape with move-only data still declares and prepares. If rustc started rejecting the binder-wrapped false bounds at the impl itself, this test would stop compiling.
- **Negative tests:** `compile_fail` doctests in `src/lib.rs`. There are 6 cases: a required path value, an optional query value, an initialized header value, an `each` `Vec<MoveOnly>`, a move-only body, and one move-only value among `Clone` ones. Each case has a compiling twin. The twin differs only in importing `Cloneable as V` instead of `MoveOnly as V`, and the two types are identical except for `derive(Clone)`. On every toolchain this proves that the failure comes from the missing `Clone`.
- **Error codes, tested by experiment:** I wrote two doctests, `compile_fail,E0599` and a deliberately wrong `compile_fail,E0277`, both on `struct A; A.clone();`.
  - 1.85.1: both pass, so the code is not checked.
  - 1.97.1: both pass, so the code is not checked.
  - nightly (1.101.0-nightly 2026-10-12): the wrong code fails with `Some expected error codes were not found: ["E0277"]`.

  Error codes are therefore checked by nightly rustdoc only. I kept the `E0599` annotation because the nightly lane does check it. The docs say that stable ignores the code and that the twins carry the reason there. Exact-stderr trybuild was not used.

### Why each negative case fails, per toolchain
Each compile-fail snippet was extracted into a scratch crate and built. Full stderr is in `/tmp/claude-1000/-home-canarit-projects-concord-api/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/clonecases/stderr-<toolchain>-case<N>.txt`.

| case | 1.85.1 | 1.97.1 | nightly |
|---|---|---|---|
| 1 required | E0599, note `MoveOnly: Clone` not satisfied | same | same |
| 2 optional | E0599, `MoveOnly: Clone` | same | same |
| 3 initialized | E0599, `MoveOnly: Clone` | same | same |
| 4 `each` Vec | E0599, `Vec<MoveOnly>: Clone` | E0599, `Vec<MoveOnly>: Clone` | E0599, "following trait bounds were not satisfied: `MoveOnly: Clone` which is required by `ApiGet: Clone`" (wording differs) |
| 5 body | E0599, `MoveOnly: Clone` | same | same |
| 6 mixed | E0599, bound points at `nonce: V` | same | same |

Excerpt (1.85, case 6):
```
error[E0599]: the method `clone` exists for struct `ApiPut`, but its trait bounds were not satisfied
note: trait bound `MoveOnly: Clone` was not satisfied
4 | |     POST put(id: C, query note?: C, header("x-nonce") nonce: V, body payload: C via Raw) {
  | |                                                              ^ unsatisfied trait bound introduced here
```

### Toolchain matrix (`cargo +<tc> test --locked -p concord-clone-compat`)
| toolchain | runtime | twin doctests (compile) | compile_fail doctests | E0599 checked |
|---|---|---|---|---|
| 1.85.1 | 4/4 | 6/6 | 6/6 | no |
| 1.97.1 | 4/4 | 6/6 | 6/6 | no |
| nightly 1.101.0 (2026-10-12) | 4/4 | 6/6 | 6/6 | yes |

**Wiring:**
- `cargo test --workspace` (1.97.1) covers the crate.
- `msrv-examples` (1.85) now includes `-p concord-clone-compat`.
- `scripts/verify.sh --nightly` runs the crate on nightly and never changes the exit status.
- CI job `nightly-clone` (`continue-on-error: true`) runs it on nightly. I ran the nightly lane locally and it passed with 16 tests.

### Ordinary alternative
Always deriving `Clone` would not lower the total cost:
- A `#[derive(Clone)]` on the non-generic generated struct needs every field to be `Clone`, so a move-only value or body would become a compile error at the declaration. That is the pre-D08 restriction that D08 removed.
- Never implementing `Clone` loses retries and cloning for every user.
- An opt-in marker or a generic struct would add syntax or change type signatures.

The current mechanism is about 25 lines in the macro, and these checks now cover its risk. Also, rust-lang/rust#48214 (`trivial_bounds`) moves toward *accepting* trivially false bounds, where the impl would simply never apply. That is the direction the canary would catch if it ever broke. So the alternative was not implemented.

## Final verification (HEAD `17bace6`, clean)
```
scripts/verify.sh            → PASS fmt, clippy, test, test-no-default, test-json-only,
                               test-reqwest-defaults-only, feature-isolation, test-multipart-alone,
                               doc, doc-no-default, doc-json-only   (exit 0)
  verify-test: 468 passed · test-no-default: 349 · test-json-only: 365 ·
  test-reqwest-defaults-only: 351 · test-multipart-alone: 11 (0 failed everywhere)
scripts/verify.sh --msrv     → the above + PASS msrv-concord (368), msrv-examples (83),
                               msrv-multipart-alone (11)   (exit 0)
scripts/verify.sh --nightly  → PASS nightly-clone-compat (16 passed)
```

## Limitations
- reqwest's file-name escaping puts a raw CR or LF after a backslash. It is never a line break, but RFC 9110 quoted strings do not allow CR or LF, so a strict recipient may reject such names. The docs advise sanitizing them; the recipe does not do it.
- reqwest does not check that the boundary is absent from the content; it relies on a random 4×64-bit boundary. The tests show that look-alikes survive, not that a collision is impossible.
- Only buffered parts are shown. Streamed parts (`Part::stream_with_length`) are only mentioned.
- `name*=` (RFC 8187) for unusual field names is discouraged by RFC 7578 §4.2. Servers that do not decode it see the encoded form. Plain ASCII names are unaffected.
- The multipart crate and its tests were not run on Windows locally; CI covers Windows through `cargo test --workspace`.
- Error codes in `compile_fail` doctests are not checked on stable. There, the reason is established by the twins and by the per-toolchain stderr above, not by the doctest itself.
