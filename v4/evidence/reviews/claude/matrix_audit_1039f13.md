<!-- Verbatim final report of the Claude test-cartographer subagent (Opus 5.5 medium), extracted from its transcript. -->

## Concord V4 acceptance-evidence audit (snapshot `1039f13`, branch `concord-v4`)

The evidence is mostly honest, with one exception: **L01's "PASS on 1.97.1" did not reproduce, and that also knocks out part of V05.** Every test named in the matrix exists under its exact name. Most runtime tests check concrete wire or Debug strings and would fail if the behaviour were broken. The problems are a failing UI suite, a few weak or masked assertions, and some unpinned spec rules.

### Matrix rows with a problem

| matrix row | named tests exist? | discriminating? | problem |
|---|---|---|---|
| **L01** | yes | n/a | **Does not reproduce on rustc 1.97.1 (8bab26f4f 2026-07-14).** `tests/ui.rs::ui` FAILS: the output of `option_has_no_wire_spelling.rs` and `vec_needs_each.rs` no longer matches the `.stderr` files. Only the rustc `help:` wording changed ("is implemented" became "is conditionally implemented", plus an "unsatisfied requirement" label). The error code is still E0277. The row also says "34 cases", but there are **36** `.rs` files. |
| **V05** | yes | runtime part yes | It cites `ui/option_has_no_wire_spelling.rs` as PASS, but that case currently mismatches (see L01). The values test is fine. |
| **V11** | yes | partly | `duplicate_names.rs` declares `list(a: u32, a: u32)`, but its `.stderr` has no duplicate-value error because other errors hide it. Compiling the same snippet alone in a scratch crate does give `duplicate value \`a\` in \`Api.list\``, so the check exists but this fixture doesn't prove it. The `scope s { GET send(); }` line in `method_collisions.rs` raises no error, so it tests nothing. Never tested: clashes with `with_base_url` / `send` / `prepare` / `clone`, a client value named `http`, and the `__` prefix. |
| **V09** (via `v03_initializer_evaluation_points`) | yes | **no** for "keeps values" | The test only counts initializer runs and checks the endpoint stamp `e`. It never checks that a cloned scope still carries `r`/`s` (for example after `set_root(9)`), so a clone that reset values would still pass. |
| **V03** | yes | partly | It shows initializers are not re-run by `prepare`, but `send` is never checked. "Declaration order" and "evaluated even when immediately overridden" are not tested. |
| **M05** | yes | half | `m05_failure_is_not_retried` counts server requests, which is good. `bodies.rs::transport_error` only checks the error kind and Display. It cannot count connection attempts, so a retrying implementation would still pass the transport half. |
| **C03** | yes | mostly | `path_errors.rs` covers literal `..` but not a lone literal `.` segment. The value-side test is good. |
| **C04** | yes | mostly | 64 bytes is rejected, but no test shows 63 is accepted, and the 253-byte host limit is untested. The IP-base test only checks the error kind. |
| **C13** | yes | partly | The compile-time base checks only cover: query (which also hides the user-info error in `base_errors.rs`), a non-numeric port, a bad host, bad IPv6, `%` in the path, a numeric last label, and punycode. Not tested at compile time: user-info alone, fragment, a scheme other than http/https, port `0` or `65536`. The trailing-`/` rule has no test for a base with no path owners (see gap 3). |
| "query keys case-sensitive; form encoding; no `?` when empty" | yes | `no_query_string_when_nothing_survives` is weak | Its fixture (`Paths…here`) has no query contributions at all, so nothing is removed. A real "removed, so no `?`" case is only covered indirectly, by `c08_suppressed_contributions_are_not_formatted` (the `removed` endpoint yields `<none>`). |
| **R04** | yes | **no** for "not buffered" | `c4_r_005_unit_drains_large_body` only checks success and one request, so a buffering `()` would pass. `adv_6` (not cited in the matrix) covers a custom `DISCARD` type receiving 0 bytes. Nothing measures memory or allocation. |
| **S05** | yes | yes | Small gap: for build errors, nothing asserts that `source()` is `None`, which the spec requires. |
| **L05 / L07** | n/a | not checked | Their evidence is in `evidence/GATES.md`, which I did not audit. L03, L04 and L06 say "see GATES" rather than PASS, so I did not count them. |

**Clean rows: 43** of 55 PASS-marked rows (12 rows above have problems). This includes C05, C06, C08, C09, C10, S02–S04, S06, R01–R03 and R05–R08. R08 passes trivially because the native type guarantees it, which fits its claim.

### Most important gaps (with the test I would add)

1. **`override "k" = each opt` with a required `Option` that is `None` clears the key (§5).** Not tested anywhere. Fixture: client `query { "b"="blk"; "k"="a"; }`, `GET clr(o: Option<String>) { query { override "k" = each o; } }`, with client param `query("p") p: u32 = 1`.
   - `clr(None)` → query `p=1&b=blk`
   - `clr(Some("z"))` → `p=1&b=blk&k=z`

   My scratch probe gave exactly these results.
2. **`Decoder::decode` applies status handling (§6).** Only a 200 response is tested. Test: `into_parts()`, execute against a 500 reply, then `decoder.decode(resp)` → `kind()==Status`, `status()==500`, and `into_response()` returns the body unread.
3. **Base trailing slash when no owner has a path (§4).** The spec contradicts itself here: "at most one trailing `/` removed" versus "the base path is used unchanged". Probe: `with_base_url("http://h.test/root//")` gives `http://h.test/root//?…`, and `http://h.test/root/` with no path owners is not pinned either. **The coordinator must decide**, then add an exact-URL test. Related: a runtime base with port `:0` is accepted (`with_base_url("http://h.test:0")` returns Ok). The spec only restricts ports for the declared base, so this needs an explicit decision too.
4. **Order of build checks (§6).** Test: an invalid body content type plus an invalid label returns the content-type error. Also `GET e(bad: String){ subdomain "{bad}"; path "{bad}"; }` with `".."` → Display `GET Api.order: request not built: subdomain label \`{bad}\` is not a DNS label (1-63 ASCII letters, digits or interior hyphens)` (observed in my probe).
5. **All 8 reserved headers (§5).** One UI case per name and casing (`Host`, `TE`, `Transfer-Encoding`, …) → "is managed by the HTTP transport". Also a runtime test that `Cookie` and `Authorization` are sent unchanged.
6. **Header value policy at build time (§5).** A non-sensitive header with value `"café"` → `Build` error whose Display contains the header name and not `café`. A value `"a\tb"` → sent byte-for-byte as `a\tb`.
7. **`IntoValue` strictness (§3), as UI tests.**
   - `set_limit(5u8)` for a `u32` value → E0277 "is not accepted for a value of type `u32`".
   - `maybe_cursor("x")` (no `Some`) → E0277.
   - `set_cursor(&String::from("x"))` compiles and sends `cursor=x`.
8. **Initializer timing and clone retention (V03/V09).** Use initializers that append `"a"` and `"b"` to a log → after `new` the log is `["a","b"]` (declaration order). `send()` adds nothing. Then `Timing::new(h).set_root(9).inner().clone().leaf().prepare()` → query `r=9&s=200&e=300`.
9. **Name normalisation and exact-match safety list (§9).** Unmarked `query { "Client_Secret" = v; "secret_id" = w; }` → `Prepared` Debug contains `Client_Secret=<redacted>` and `secret_id=<w>`. Unmarked `header("X-API-KEY")` → `x-api-key: <redacted>`.
10. **Generated-name and DNS limits (§2, §4).**
    - UI: client value named `http`, value `__x`, and endpoint `with_base_url` each give a collision error naming both origins.
    - Runtime: a 63-byte label is accepted; a host longer than 253 bytes gives a `Build` error mentioning `subdomain label`.
    - UI: `scope s { subdomain "api"; }` under a declared IP base (`http://127.0.0.1`) is a compile error.

Smaller items: sensitivity on the Debug of `Decode`/`Status` errors when the host is sensitive; `Accept` never being added is only shown by reqwest's `*/*` default; streaming request bodies are honestly marked NOT_RUN.

### What I ran

All runs were on Linux x86_64, rustc/cargo 1.97.1 (rustc 8bab26f4f 2026-07-14), snapshot HEAD `1039f13cedc18a8c91a0b354a56a4463aeac9e03`, with `CARGO_TARGET_DIR=/home/canarit/projects/concord_api_v4/target/cartographer`.

1. `cargo test --manifest-path <snapshot>/Cargo.toml --workspace`
   - bodies 16/16 ok
   - composition 31/31 ok
   - hygiene 2/2 ok
   - sensitivity 11/11 ok
   - slice 7/7 ok
   - **ui FAILED** (mismatches in `option_has_no_wire_spelling.rs` and `vec_needs_each.rs`; the other 34 ok)
2. `cargo test … -p concord --test ui`, to see the full mismatch diff described under L01.
3. `cargo test … --workspace --no-fail-fast --exclude concord`: renamed `tests::builds_a_request` ok; shapes `wire.rs` 3/3 ok.
4. `cargo test … -p concord --no-default-features --features reqwest-defaults --test values --test composition --test sensitivity`: 11, 31 and 11 passed.
5. Scratch probe crate at `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/probe`, using a path dependency on the snapshot and built `--offline`. It produced the duplicate-value diagnostic (V11) and the outputs A–F quoted in gaps 1, 3 and 4.

`git status` in the snapshot was clean after the runs, so nothing was modified. I did not read `v4/evidence/reviews/`.

Files referenced: `<snapshot>/v4/TEST_MATRIX.md`, `<snapshot>/v4/SEMANTICS.md`, `<snapshot>/crates/concord/tests/{slice,values,composition,sensitivity,bodies,hygiene,ui}.rs`, `<snapshot>/crates/concord/tests/ui/{option_has_no_wire_spelling,vec_needs_each,duplicate_names,method_collisions,base_errors,path_errors}.{rs,stderr}`, where `<snapshot>` is `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/review_matrix`.