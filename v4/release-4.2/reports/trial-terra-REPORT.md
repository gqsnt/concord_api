# Concord documentation-first trial

Environment: standalone edition-2024 Cargo project, toolchain 1.99.1, path dependency on the candidate snapshot. Model and reasoning effort are not observable to me. Source snapshot was treated as read-only. All network calls in the fixture target 127.0.0.1 and use dummy data and finite timeouts.

## Task 1 — root value

Prediction recorded before reading Quick start. Lookups: `README.md` § Example, Installation and features; `docs/QUICKSTART.md` §§ 1–4 (particularly §3 Declare the API). I wrote `Trial(#[sensitive] token: String)`, an Authorization header template, and `Trial::new(http, TrialConfig { token: ... })`. The guide says required client values become generated config fields; a client without required values has no config. The config gathers the required API configuration separately from the caller-owned reqwest transport. Compile/run result pending.

Task 1 result: `heavy.sh cargo run` succeeded; printed `Trial { base: "http://127.0.0.1/", token: <redacted> }`. No repair or remaining confusion.

## Task 2 — formats and outputs

Prediction recorded after Quick start, before looking at Author guide. Lookup: `docs/AUTHOR_GUIDE.md` §§ 5, 5.1–5.2, 5.5. Wrote `use concord::Json`, `format Json;`, serde `Item`, `GET item() -> Item`, `DELETE clear()` and `GET unit() -> ()`. `heavy.sh cargo run` passed; prepared typed request has `Accept: application/json`, untyped request does not suggest JSON, and both untyped and unit prepare as `Prepared<()>`. The guide specifies no output drains the body without buffering and accepts any 2xx; JSON `-> ()` checks the head and parses JSON `null`; an empty JSON-labelled body gives Decode and an unlabelled 204 gives Metadata. No repair or confusion.

## Task 3 — arguments and inference

Prediction recorded before additional lookup. Lookups: `docs/CALLER_GUIDE.md` § 2 (generated signatures and `IntoValue`); `docs/DIAGNOSTICS.md` inference diagnostic; `docs/AUTHOR_GUIDE.md` § 2. Added `search(name: String, query count: u32)`, then passed a borrowed `&str` plus `"42".parse().unwrap()`. `cargo check` failed E0283 because parse's output type was ambiguous under `impl IntoValue<u32>`, exactly as the caller guide warns. Rustc incidentally printed implementation spans from `crates/concord-core/src/value.rs` in its diagnostic; I did not open or read that file. Repaired with `parse::<u32>()`; `cargo run` succeeded and asserted the path `/search/hello%20world` and query `count=42`. No remaining confusion.

## Task 4 — unread responses

Prediction recorded before Caller guide lookup. Lookup: `docs/CALLER_GUIDE.md` § 6 and “A 2xx response the format refuses.” Implemented a finite four-connection HTTP server on `127.0.0.1:0`, with three-second reqwest, accept, read, write and join timeouts. `heavy.sh cargo run` passed: a 200 `text/plain` for JSON produced Metadata, `status() == None`, and `into_response().bytes()` returned `refused body`; a 422 produced Status, reported 422, and returned `status body`. The server also confirmed no-output succeeds on 200 text with a body and JSON unit succeeds on JSON `null`. Intentionally wrong Content-Type is the requested fixture, not a library defect. No repairs or confusion.

## Task 5 — maintenance edit

Prediction recorded before this task's lookup. Lookup: `docs/AUTHOR_GUIDE.md` §§ 4.1–4.3, 4.7 and 11.1. `src/maintenance.rs` keeps separately compiled before and after declarations: both `issues` and `pulls` originally set `per_page=30` and `x-cache: shared`; the edit moves those to `repo`. The third sibling `commits` changes to `override` for `per_page=100` and `x-cache: private`. The first two additionally append their own second `x-cache` value. Prepared requests were checked before and after, without network I/O. `heavy.sh cargo run --offline` passed with these exact results:

| Sibling | Before query | After query | Header values before and after, in order |
|---|---|---|---|
| issues | `v=2&state=open&per_page=30` | `v=2&per_page=30&state=open` | `shared`, `issues-tail` |
| pulls | `v=2&state=open&per_page=30` | `v=2&per_page=30&state=open` | `shared`, `pulls-tail` |
| commits | `v=2&state=open&per_page=100` | `v=2&per_page=100&state=open` | `private` |

The ordering prediction was partly wrong: overrides keep the inherited position, but moving the first appearance to a scope moves the key before endpoint keys. Header order across distinct names is not specified; the fixture checks same-name values in contribution order, not an unsupported cross-name order. No repairs or remaining confusion.

## Generated reference and documentation issues

`heavy.sh cargo doc -p concord --no-deps --offline` generated the reference in this project's `target/doc/concord/`. Looked up `IntoValue` implementations and `ErrorKind::Metadata`. The rustdoc lists `&HeaderValue` in addition to the string conversions. I verified that a generated endpoint accepting `HeaderValue` also accepts `&HeaderValue` and prepares the header successfully (`src/main.rs`, `header_test`; `cargo run --offline` passed).

- `docs/CALLER_GUIDE.md`, “The declaration used here”, generated signatures: says `IntoValue<T>` accepts exactly `T` or borrowed strings and “Nothing else converts.” This omits `&HeaderValue` for `HeaderValue`; I would wrongly conclude a native header value must be cloned by the caller. Add the borrowed `HeaderValue` case and link the trait reference. `docs/AUTHOR_GUIDE.md` § 2 and `docs/QUICKSTART.md` § 3 similarly summarize only borrowed strings and should be updated for consistency.

No implementation source, tests, `v4/` records, or review reports were opened. Rustc's E0283 diagnostic automatically included a few implementation-span lines from `crates/concord-core/src/value.rs` as noted above; that was not a deliberate file lookup. The initial inference failure was an intentional experiment, not a library or fixture defect. The 200 with a non-JSON media type was intentionally constructed for the requested error test. No other documentation defects were established in this trial.
