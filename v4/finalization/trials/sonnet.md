# Fresh-use trial report: Field Notes client with Concord (participant: sonnet)

Note: writing `trial/REPORT.md` was refused by the environment ("Subagents should return findings as text, not write report files"), so this message is the report. I did not try another route to write it. Predictions as written before each run ARE on disk in `trial/PREDICTIONS.md`.

Snapshot: `/home/canarit/projects/concord_v4_finish_work/snapshots/fu-sonnet` (toolchain 1.97.1).
Crates: `trial/` (`notes-trial`, tasks 1-6, 8, 9), `trial/renamed/` (`notes-renamed`, task 7), task 10 in `examples/riot/tests/trial_sonnet.rs`.
Diagnostic probes (expected-to-fail compiles, behind a `probes` feature): `trial/probes/` (`run.sh`, output captured in `OUTPUT.txt`) and `trial/renamed/probes/`.
All runnable tests pass with `--locked --offline` once the lock file is pruned (see Setup). No tracked file of the snapshot was modified (`git status`: only `trial/` and `examples/riot/tests/trial_sonnet.rs`, both untracked). No server was left running (all loopback servers live inside test processes).

## Transitions to library source / macro expansion / `v4/` design records

None deliberate. I never opened `crates/concord/src`, `crates/concord-macros/src`, the library's own tests (`crates/concord/tests`, nor `examples/*/tests` other than `examples/riot/tests/support/mod.rs`, which the brief says to read), never ran `cargo expand`, never opened `v4/`. Docs reference `v4/SEMANTICS.md` and `crates/concord/tests/ui/*.stderr`: not followed.
Incidental exposure: rustc's own E0277 output printed lines of library source (tasks 4 and 7: `crates/concord/src/repr.rs:171`, `repr.rs:121-194`, `__private.rs:831`, `__private.rs:183`). I did not go and read those files.
Example code I did read (allowed as "examples' code"): `examples/quickstart/src/lib.rs` (partial), `examples/renamed/{Cargo.toml,src/lib.rs}`, `examples/native/src/repr.rs` (Ndjson) BEFORE writing my own codec in task 7 (so task 7's codec is not an independent first guess), `examples/riot/src/{lib,api,routing}.rs`, riot fixtures and rustdoc.

## Pages and sections opened, in order (whole trial)

1. `docs/QUICKSTART.md` (whole). 2. rustdoc `concord/index.html` (whole). 3. rustdoc `concord/macro.api.html` (one sentence, points to the crate page).
4. `docs/AUTHOR_GUIDE.md` (whole, sections 1-10). 5. `docs/CALLER_GUIDE.md` (whole). 6. `examples/riot/tests/support/mod.rs` (copied, see Setup).
7. After the task 1 run: own rustdoc `Notes`, `NotesNotebook`, `NotesCreateNotebook`, `NotesConfig`.
8. After the task 3 run: own rustdoc `NotesUpdateNotebook` and `Notes::update_notebook`. 9. After task 4: own rustdoc `NotesNotebookPage`.
10. Before task 7: `docs/RECIPES.md` (whole), `docs/MIGRATION.md` lines 680-718 (renamed dependency), `examples/renamed/Cargo.toml` + `src/lib.rs`, `examples/native/src/repr.rs`. After the task 7 run: rustdoc of the renamed crate's `Notes`.
11. After the task 8 run: rustdoc `concord/struct.Error.html`. After task 9: `struct.Prepared.html`, `struct.Decoder.html`.
12. Task 10: `examples/riot/README.md`, `fixtures/README.md`, `src/lib.rs`, `src/api.rs`, `src/routing.rs`; Riot rustdoc (`types::TournamentCodeParameters`, `TournamentId`, `TournamentCode`, `PickType`, `MapType`, `SpectatorType`, `api::RiotTournamentStub`); fixtures `tournament_id.json`, `codes.json`, `code_info.json`, `account.json`, `summoner.json`, `match_ids.json`, `provider_id.json`, `error_403.json`, `match_EUW1_7000000001.json`, `masteries_top.json`.
13. At the end, AFTER tasks 4-7 had been run: `docs/DIAGNOSTICS.md` (headings; "An invalid header name or value", "Setting an inherited key again", "Known rough edges"), `README.md`. So the diagnostics of tasks 4-7 were first met from the compiler, not from DIAGNOSTICS.md.

## Setup (harness-level friction, not Concord defects)

- `cp Cargo.lock trial/Cargo.lock` then `cargo test --locked` fails at once:
  ```
  error: cannot update the lock file .../trial/Cargo.lock because --locked was passed to prevent this
  help: to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
  ```
  Reason: the copied lock lists the other workspace members and lacks `notes-trial`. Workaround: `cargo fetch --offline` once (the lock then differs from the original by 54 removed package entries and one added `notes-trial`; I diffed name/version pairs: no third-party version changed), then `--locked --offline` works. Same for `trial/renamed`. Smallest change: say so in the brief ("copy the lock, run `cargo fetch --offline` once").
- `examples/riot/tests/support/mod.rs` ends with Riot helpers that `use concord_example_riot::...`; copied into another crate it does not compile. I cut everything from `/// Riot's routed hosts` on, for my crates (left intact for task 10). Smallest change: split the generic server from the Riot helpers, or say it in the brief.
- `cargo test` builds `examples/`; probes that must fail to compile are therefore registered behind `required-features = ["probes"]` (`cargo check --features probes --example probe_t4a`).

---

## Task 1: describe read and create; predict signatures and wire; loopback test

**Author.** Declaration (`trial/src/t1.rs`):
```rust
concord::api! {
    pub client Notes(#[sensitive] token: String, tenant: String) {
        base "https://notes.example.test/v1";
        headers { AUTHORIZATION = "Bearer {token}"; "x-notes-tenant" = tenant; }
        format json;
        GET notebook(notebook: String) -> Notebook { path "notebooks/{notebook}"; }
        POST create_notebook(body new_notebook: NewNotebook) -> Notebook { path "notebooks"; }
    }
}
```
- Predicted: `Notes::new(reqwest::Client, NotesConfig{token,tenant})`; `notebook(&self, impl IntoValue<String>) -> NotesNotebook`; `create_notebook(&self, impl IntoValue<NewNotebook>) -> NotesCreateNotebook`; `send() -> Result<Notebook, Error>`; wire: GET `/v1/notebooks/nb1` with `authorization: Bearer tok`, `x-notes-tenant: acme`, no content-type, **no accept**; POST with `content-type: application/json`, body `{"title":"Ideas"}`; Debug shows `token: <redacted>`.
- Outcome: compiled first time, no warnings; signatures, paths, bodies and Debug (`Notes { base: "http://127.0.0.1:41089/v1", token: <redacted>, tenant: "acme" }`) as predicted.
- Wrong prediction: the wire has `accept: */*` (added by reqwest/hyper at execute). Failure text: `left: Some("*/*") right: None`. `prepared.request().headers()` does not contain it (checked in `tests/task_extra.rs`), so the prepared view and the wire differ.
- Docs that mattered: QUICKSTART section 3, crate docs quick start, AUTHOR_GUIDE 1-2, 4.2, 5. Own rustdoc confirmed the signatures; the per-endpoint text ("Request: GET https://notes.example.test/v1/notebooks/{notebook}. Output: Notebook, decoded from a 2xx response with the format json (the response default of the client Notes)") was the most useful single thing.
- Compiler messages: none. Surprise: "Concord adds no Accept header" (QUICKSTART, AUTHOR_GUIDE 5.3, crate docs) is literally true, but a reader will conclude the request carries no Accept. Smallest change: add "(reqwest itself sends `accept: */*`)" to the QUICKSTART bullet.

**Caller.** `api.notebook("nb1").send().await`, `api.create_notebook(NewNotebook{..}).send().await`; `&str` accepted for `String`. `with_base_url` replaces the whole base including `/v1`, so the loopback URL had to include `/v1` (shown in the quickstart's example, easy to miss). No problem.
Ambiguity kept: what `token` means. The brief says `Authorization: Bearer <token>`; with the template above the caller supplies the bare token; with `header(AUTHORIZATION) token` the caller supplies `"Bearer ..."` (task 5 shows both). Both are legitimate readings.

## Task 2: raw text (read, replace) and bytes as exceptions

**Author.**
```rust
GET read_page_text(notebook: String, page: String) -> String { path "notebooks/{notebook}/pages/{page}/text"; response text; }
PUT replace_page_text(notebook: String, page: String, body text: String) { path ".../text"; request text "text/markdown"; }
GET attachment(notebook: String, id: String) -> Bytes { path ".../attachments/{id}"; response bytes; }   // concord::bytes::Bytes
```
- Predicted and observed: raw text returned verbatim; PUT sends `content-type: text/markdown` (no charset) with the raw body, 204 gives `Ok(())` (no `->`); attachment bytes `ff 00 42` unchanged (an id `a/b` goes out as `/attachments/a%2Fb`); invalid UTF-8 text gives `ErrorKind::Decode` ("GET Notes.read_page_text: response not decoded: response body is not valid UTF-8 (valid up to byte 0)"). No wrong predictions.
- Probe (`tests/task02b.rs`): forgetting the exception compiles and misbehaves silently. `GET .. -> String` without `response text;` gives `Decode / ... JSON decode error (syntax) at line 1, column 1`; `PUT .. body text: String` without `request text` sends `content-type: application/json` and body `"# New\n"` **JSON-quoted**, with no error at all. AUTHOR_GUIDE 5.2 and the QUICKSTART describe this hazard ("the format, not the Rust type, decides the wire"); the PUT case is the dangerous one. Smallest change: no API change; a one-line warning next to the QUICKSTART `PUT image` example that a `String`/`Vec<u8>` body under `format json;` is JSON-encoded without any error.
- `-> ()` vs no `->` (extra): `PUT .. -> () { request text "text/markdown"; }` with a 204 under `format json;` is `Decode` ("JSON decode error (unexpected end of input) at line 1, column 0"), as AUTHOR_GUIDE 5.5 says; without `->` it succeeds.

**Caller.** `send().await` returns `String`, `()`, `Bytes`. Nothing surprising.
Ambiguity kept: the brief says the PUT returns 204; it does not say whether a caller expects `()`; I read it as "no typed output".

## Task 3: PATCH with an optional, nullable body

**Author.** The brief gives no PATCH response; I declared none (ambiguity kept; any 2xx accepted, body drained).
```rust
PATCH update_notebook(notebook: String, body patch?: Option<NotebookPatch>) { path "notebooks/{notebook}"; }
```
(`NotebookPatch { title: Option<String>, archived: Option<bool> }`, both `skip_serializing_if = "Option::is_none"`.)
- Predicted and observed (`tests/task03.rs`), all on the wire:

  | call | body | content-type |
  |---|---|---|
  | `update_notebook("nb1")` or `.maybe_patch(None)` (touch) | none (no content-length either) | none |
  | `.set_patch(None)` (reset) | `null` | `application/json` |
  | `.set_patch(Some(NotebookPatch::default()))` | `{}` | `application/json` |
  | `.set_patch(Some(NotebookPatch{title:Some("T"),..}))` | `{"title":"T"}` | `application/json` |
  | `archived: Some(false)` only | `{"archived":false}` | `application/json` |
  | `.maybe_patch(Some(None))` | `null` | `application/json` |

  Signatures: `update_notebook(&self, impl IntoValue<String>)`, `set_patch(self, impl IntoValue<Option<NotebookPatch>>)`, `maybe_patch(self, Option<Option<NotebookPatch>>)`. `set_patch(None)` infers without annotation. No wrong predictions (I had hedged "maybe content-length: 0": absent).
- Docs: AUTHOR_GUIDE 5.4 states the three states in one sentence; I found it only because I read the whole guide. The generated rustdoc of `set_patch`/`maybe_patch` says "Sets the request body. Initially absent: no body is sent." / "Sets the request body to exactly value: None sends no body." and does **not** say that `set_patch(None)` sends JSON `null`. Smallest change: for `body x?: Option<T>` the generated docs of `set_x` should say "`set_x(None)` sends JSON `null`; `maybe_x(None)` sends no body".

**Caller.** The pair `set_patch(None)` (JSON null = reset) / `maybe_patch(None)` (no body = touch) is easy to swap; `.set_patch(None)` reads like "no patch". Wire-checked, but the names do not say it. Surprise kept, not a defect. Smallest change: the rustdoc sentence above (or an example in CALLER_GUIDE section 4, whose three-state example uses a header, not a body).
Ambiguity kept: `NotebookPatch { title?, archived? }`: I read `?` as "key may be absent", not "nullable".

## Task 4: scope, then change the group default, then change one exception

**Author.** `trial/src/t4.rs`: `create_notebook` and `list_notebooks(query archived?: bool, query limit?: u32)` on the client; `scope notebook(notebook: String) { path "notebooks/{notebook}"; get, update, attachment, scope page(page: String) { path "pages/{page}"; request text "text/markdown"; response text; text, set_text, meta (response json;) } }`.
- Predicted and observed: `api.notebook("nb1").page("p1").text().send()`, types `NotesNotebook`, `NotesNotebookPage`, `NotesNotebookPageText`, `NotesNotebookPageSetText`; wire identical to tasks 1-3 for every operation; `list_notebooks()` has no query string, `.set_limit(5).set_archived(true)` gives `?archived=true&limit=5` (declaration order, not call order). My own test indexing was off by one (test bug). Own rustdoc of the scope states each endpoint's format AND its origin ("the request default of scope Notes.notebook.page", "the endpoint statement").
- Changing the group's default: moving the page group to text while an endpoint keeps a typed non-String output and no exception is a compile error (predicted; `probes/probe_t4a.rs`):
  ```
  error[E0277]: the format `Text` cannot decode a `PageMeta` response
   16 |   GET meta() -> PageMeta { path "meta"; }          // forgot `response json;`
      |                 ^^^^^^^^ this output's format is `Text` (the nearest `response` statement: on the endpoint, a scope or the client); it needs `Text: Decode<PageMeta>`
      = note: built in: `json` (any `DeserializeOwned`, feature `json`), `text` (`String`), `bytes` (`Bytes`, `Vec<u8>`), `form` ...; the words name `Json`, `Text`, `Binary` and `Form`. Another format is a type implementing `concord::Decode<PageMeta>`, named by its path: `response Csv;`; an endpoint without typed output omits `->`
  help: the trait `Decode<PageMeta>` is not implemented for `Text`
        but trait `Decode<String>` is implemented for it
     --> .../crates/concord/src/repr.rs:171:1
  note: required by a bound in `concord::__private::decoder`
     --> .../crates/concord/src/__private.rs:831:19
  ```
  A good message (names the statement chain and the fix). The same move is SILENT for `String`/`()`/`Vec<u8>` payloads (task 2 probe): a `-> String` endpoint that was a JSON string would now be raw text.
- Changing one exception without altering siblings (`tests/task04b.rs`): `meta` from `response json;` + `-> PageMeta` to `-> Bytes { response bytes; }`: only that endpoint's signature and wire change; `text` and `set_text` (`text/markdown`) are unchanged. As predicted.

**Caller.** Navigation is `api.notebook("nb1")`, then `.page("p1")`, then the endpoint; scope handles also get `set_notebook`/`set_page`. Nothing surprising. One naming trap I noticed but did not fall into: my endpoint `set_text` on scope `page` would collide with a value setter if that scope had a value `text` (AUTHOR_GUIDE 9 lists such collisions).
Ambiguity kept: "change the group's default" does not say which default (format, path, headers); I chose formats.

## Task 5: native `AUTHORIZATION`, own `HeaderName` constant, rotation

**Author.** `trial/src/t5.rs`:
```rust
pub const X_NOTES_TENANT: HeaderName = HeaderName::from_static("x-notes-tenant");
headers { AUTHORIZATION = "Bearer {token}"; X_NOTES_TENANT = tenant; }
// placement form (module `placed`): #[sensitive] header(AUTHORIZATION) token: String, header(X_NOTES_TENANT) tenant: String
```
- Predicted and observed: same wire as task 1; `Prepared` Debug `Prepared { endpoint: "Notes.notebook.get", method: "GET", url: "...", headers: [authorization: <redacted>, x-notes-tenant: acme], body: None }`; `headers["authorization"].is_sensitive()` true, tenant false. With the placement form the caller must supply `"Bearer ptok"` (the prefix is part of the value).
- Unexpected (not mispredicted, but not foreseen in detail): the Build error for a tenant containing a line break names the header by the Rust constant's spelling: ``Build / GET Notes.notebook.get: request not built: header `X_NOTES_TENANT` value is not a valid header value (rejected by `http::HeaderValue`)``, while Debug shows wire names, and rustdoc "Headers (...): AUTHORIZATION, x-notes-tenant" mixes both. AUTHOR_GUIDE 4.8 says Concord "never reads a constant's identifier": true for semantics, not for labels. Smallest change: a sentence in 4.8 ("diagnostics and rustdoc name a native key as you spelled it"), or append the wire name in the Build message.
- Probe `probes/probe_t5a.rs`: the brief spells the header `X-Notes-Tenant`; `HeaderName::from_static("X-Notes-Tenant")` fails in `http`'s const evaluation (predicted):
  ```
  error[E0080]: evaluation panicked: HeaderName::from_static with invalid bytes
   --> probes/probe_t5a.rs:3:40
    | pub const X_NOTES_TENANT: HeaderName = HeaderName::from_static("X-Notes-Tenant");
  note: erroneous constant encountered
   --> probes/probe_t5a.rs:7:53        (the placement inside the Concord declaration)
  ```
  Not Concord's message; DIAGNOSTICS "Known rough edges" mentions the same `E0080` only for over-long literal names. Smallest change: one sentence in AUTHOR_GUIDE 4.8 ("`from_static` needs lowercase").

**Caller (rotation).** `set_token`/`set_tenant` consume the handle and return a new one (`tests/task05.rs`): `let rotated = api.clone().set_token("tok2")`; handles/scopes/requests created earlier keep sending the old token (`old_scope.get()` sent `Bearer tok` after rotation), new ones send the new; `with_base_url` keeps both. A caller-side session holds `Notes` and rotates with `self.api = self.api.clone().set_token(t)`. Surprise kept: a scope handle cached across a rotation silently keeps the stale token (documented in CALLER_GUIDE 3 and RECIPES `Session`, but only there).
Ambiguity kept: "rotate supplied data as a caller would" does not say whether cached handles should follow; Concord answers "no, new handles only".

## Task 6: two names for one header; invalid template fragment

**Author (collisions; `tests/task06.rs`, `task06b.rs`).** Client has `X_NOTES_TENANT = tenant;`.

| Endpoint declaration | Result |
|---|---|
| `headers { "x-notes-tenant" = "other"; }` | compiles; `prepare()`/`send()` is `ErrorKind::Build`, nothing sent |
| `headers { X_TENANT_ALIAS = "other"; }` (second const, same name) | same Build error |
| `override "x-notes-tenant" = "other";` or `override X_TENANT_ALIAS = ..` | works; one line `other` |
| `append "x-notes-tenant" = "other";` | two lines `acme`, `other` |
| `remove "x-notes-tenant";` | no tenant line |
| `headers { X_NOTES_TENANT = "other"; }` (same constant) | **compile** error |
| client literal `"x-notes-tenant"` + endpoint literal `"X-Notes-Tenant"` | **compile** error |

Run-time message: ``GET Notes.a_literal_below_constant: request not built: `x-notes-tenant` on `Notes.a_literal_below_constant` sets a header that an enclosing owner already sets (possibly with another spelling of the same name); use `override` or `append`, or `remove` it first`` (for the alias: `` `X_TENANT_ALIAS` on `Notes.b_second_constant` ... ``). Compile messages:
```
error: header `X_NOTES_TENANT` is already set by `Notes`; use `override` or `append` to change an inherited header, or add `remove X_NOTES_TENANT;` to this owner's block to replace it exactly
error: header `x-notes-tenant` is already set by `Notes`; use `override` or `append` to change an inherited header, or add `remove "x-notes-tenant";` to this owner's block to replace it exactly
```
- Wrong predictions: (a) with the second spelling as an OPTIONAL placement `header(X_TENANT_ALIAS) alias?: String` I expected an error only when `alias` is set; the endpoint is a Build error even when `alias` is absent (names are compared before values, AUTHOR_GUIDE 4.8 says so; I did not connect it to optionals). (b) A same-owner duplicate (`X_NOTES_TENANT = tenant; "x-notes-tenant" = "other";` on the client) is a Build error whose text says "an enclosing owner already sets" although both are in one owner (`on Same`). Smallest change: "this owner or an enclosing one".
- Surprise kept: the native-vs-literal collision is found only when the endpoint is first prepared; a declaration with such an endpoint compiles cleanly and the endpoint is permanently unusable. An author needs a test that prepares every endpoint. DIAGNOSTICS.md covers the compile-time variant only ("Setting an inherited key again"), not this run-time one. Smallest change: add the run-time Build message to DIAGNOSTICS.
- Extra (predicted, confirmed): a `reqwest::Client::builder().default_headers(x-notes-tenant: default)` plus Concord's `acme` gives one line `acme` (Concord's wins); with `remove "x-notes-tenant";` on the endpoint reqwest's `default` is still sent (the docs say so).

**Caller.** The caller of such an endpoint sees `ErrorKind::Build` with that message on every call; nothing marks it as an authoring bug rather than bad input except the wording.

**Author (invalid template fragment; `probes/probe_t6t.rs`, `probe_x2.rs`).** All compile errors at the literal, as predicted:
```
error: this header value contains a byte that `http::HeaderValue::from_bytes` rejects (at byte 3 of literal fragment 0; no value can repair it)   // "bad\nvalue {tenant}"
error: unclosed `{` in "tenant={tenant"
error: `{}` in "tenant={}" is not a value reference; holes are `{name}` or `{owner.name}`
error: `nope` is not a value of `Notes`                                                                                                           // "tenant={nope}"
error: this header value contains a byte that `http::HeaderValue::from_bytes` rejects (at byte 1 of literal fragment 0; no value can repair it)   // "t\u{7f}{tenant}"
error: this header value contains a byte that `http::HeaderValue::from_bytes` rejects (at byte 1)                                                // plain "a\nb" (matches DIAGNOSTICS.md)
```
Small gaps: the line-break message gives a byte index but not which byte or class (control character U+000A) and does not name the header; the span is the whole literal. The templated message shape differs from the DIAGNOSTICS.md excerpt (which shows only the plain-literal form). Smallest change: "(a control character, U+000A, at byte 3 of literal fragment 0 ...)".
Ambiguity kept: "make two names collide and find out what Concord does": the answer depends on whether any key in the chain is a native constant (compile error vs run-time Build error) and on override/append/remove.

## Task 7: renamed dependency + downstream codec

**Author.** `trial/renamed`: `notes_http = { package = "concord", path = "../../crates/concord" }`; my own NDJSON codec `Ndjson` implementing `notes_http::Encode<Vec<T>>`/`Decode<Vec<T>>`; `notes_http::api! { ... POST import_notebooks(body notebooks: Vec<NewNotebook>) -> Vec<Notebook> { path "notebooks/import"; format Ndjson; } }` (client stays `format json;`). I invented the operation (the Field Notes table has no NDJSON/CSV operation).
- Predicted and observed: compiles first time, no import of the format words, the type path `Ndjson` is found by rustc; wire `content-type: application/x-ndjson`, body `{"title":"a"}\n{"title":"b"}\n`, response lines decoded; the JSON sibling is unchanged. Rustdoc of the renamed client: "encoded with the format Ndjson (the endpoint statement). Content-Type: the format's Encode::CONTENT_TYPE, sent with the body." (does not show the value; it cannot).
- Probes (`trial/renamed/probes/probe_t7.rs`):
  ```
  error: `ndjson` is not a format: the built-in formats are `json`, `text`, `bytes` or `form`; a format type is written as a path (`self::ndjson`)
  error[E0277]: the format `DecodeOnly` cannot encode a `Vec<NewNotebook>` request body
    = note: ... Another format is a type implementing `concord::Encode<Vec<NewNotebook>>`, named by its path: `request Ndjson;`
  help: the following other types implement trait `Encode<T>`
    --> .../crates/concord/src/repr.rs:121:1      (library source lines printed by rustc)
  ```
  Uncertain prediction: I did not know whether the note would spell the trait `concord::` or `notes_http::`; it says `concord::Encode<..>` (the real crate name), which a renamed-dependency user cannot paste. Smallest change: a note in MIGRATION's renamed section ("messages say `concord::`").

**Caller.** Same as any client; `notes_http::reqwest::Client` builds the HTTP client. Nothing else surprising.

## Task 8: read a non-success response without assuming JSON

**Caller** (`tests/task08.rs`). Helper:
```rust
async fn failure(error: concord::Error) -> Result<Failure, Box<dyn std::error::Error>> {
    let response = error.into_response()?;           // non-Status errors propagate unchanged
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.bytes().await?;            // reqwest::Error -> Box<dyn Error>
    // classify by the content-type header: json / text / html / bytes / empty
}
```
- Predicted and observed: 404 JSON, 429 `text/plain` + `retry-after: 7`, 502 `text/html; charset=utf-8`, and 500 binary without content-type are all `ErrorKind::Status`; the typed JSON decoder never runs on a non-2xx body; `Display` is `GET Notes.notebook: HTTP status 404 Not Found`; a 200 with invalid JSON is `Decode` ("JSON decode error (syntax) at line 1, column 2") and `into_response()` hands it back as `Err`; `response.text()` exists with the default features. No wrong predictions.
- Surprises kept: `reqwest::Error` from `response.bytes()` has no conversion into `concord::Error`, so a helper that wants to return `concord::Error` cannot (I used `Box<dyn Error>`, as the QUICKSTART excerpt does); `status()`/`endpoint()` must be read before `into_response()` consumes the error. Smallest change: one sentence in CALLER_GUIDE 6 about both.

**Author.** Nothing to declare; the endpoint's typed output is irrelevant on this path (good).

## Task 9: native customisation, typed decode kept

**Caller** (`tests/task09.rs`). Route 1: `let (client, mut request, decoder) = api.notebook("nb1").prepare()?.into_parts();` then `*request.timeout_mut() = ...`, `request.headers_mut().insert("x-request-id", ..)`, `decoder.finish(client.execute(request).await).await?` gives a typed `Notebook`. Route 2: `reqwest::RequestBuilder::from_parts(client, request).timeout(..).header(..).send().await` passed to `decoder.finish(..)`.
- Predicted and observed: both routes decode; the extra header arrives next to `authorization` and `x-notes-tenant`; a 100 ms timeout against a 600 ms reply is `ErrorKind::Transport`, `transport().unwrap().is_timeout()` true ("GET Notes.notebook: transport error (timeout)"); a 404 through `finish` is `Status` with a readable response; `headers_mut().insert("x-notes-tenant", ..)` REPLACES the declared value, `RequestBuilder::header("x-notes-tenant", ..)` APPENDS (wire: `acme` then `other`). A header marked `set_sensitive(true)` is sent normally. No wrong predictions.
- Surprise kept: the builder's `.header()` appending is reqwest's, but the docs steer callers there (`RequestBuilder::from_parts`) and only show `insert`; replacing a declared header natively needs `headers_mut().insert`. Smallest change: a sentence in CALLER_GUIDE 7.

**Author.** Nothing; no declaration needed for this.

## Task 10: Riot

`examples/riot/tests/trial_sonnet.rs` (`mod support;`, `support::riot(&server)`, fixtures only; Riot never contacted).
- `player_summary(&riot, "Fixture Player", "EUW", Europe, Euw1, Europe, 3)`: predicted and observed 6 requests (`4 + 2`, because the fixture lists two ids, not `4 + 3`), all GET, hosts `europe`/`euw1`/`euw1`/`europe`/`europe`/`europe` `.api.riotgames.com:PORT` asserted through `Host`; paths `/riot/account/v1/accounts/by-riot-id/Fixture%20Player/EUW`, `.../summoners/by-puuid/<PUUID>`, `.../top?count=3`, `.../ids?count=3`, `/lol/match/v5/matches/EUW1_7000000002` then `..._7000000001`; every request `x-riot-token: <KEY>`, `accept: application/json`, no `authorization`, no content-type, empty body; `{riot:?}` never contains the key. Summary: two `MatchLine`s (Ahri CLASSIC win, LeeSin ARAM loss).
- Request-body call: `tournament_codes(&riot, Europe, TournamentId(2002), params, 2)`: `POST /lol/tournament-stub/v5/codes?tournamentId=2002&count=2`, host `europe.api.riotgames.com:PORT`, `content-type: application/json`, body exactly `{"metadata":"synthetic","teamSize":5,"pickType":"TOURNAMENT_DRAFT","mapType":"SUMMONERS_RIFT","spectatorType":"ALL","enoughPlayers":true}`; reply `codes.json` gives two codes. As predicted.
- Wrong prediction (example type, not Concord): I assumed `Account.game_name: String`: ``error[E0308]: mismatched types ... expected `Option<String>`, found `&str` ``. Fixed with `.as_deref()`.
- Ambiguity kept: "the documented request-body call": the README documents `tournament_codes`; I used it rather than the endpoint directly. I did not read the existing riot tests (they would contain the answer).

**Author.** The Riot declaration was read, not written. Rustdoc lists header keys as `X_RIOT_TOKEN, ACCEPT` (constant spellings), same note as task 5.
**Caller.** Pleasant: routing values are typed enums, so a wrong route is a compile error.

---

## Consolidated list: wrong predictions
1. T1: `accept: */*` is on the wire (reqwest) but absent from `prepared.request().headers()`.
2. T6: an absent optional placement of a second spelling still makes the endpoint a Build error; same-owner duplicates are reported as "enclosing owner".
3. T7: diagnostics spell the trait `concord::Encode`, not the renamed path (uncertain prediction).
4. T10: `Account.game_name` is `Option<String>` (example type).
5. Own test bug: off-by-one index in task 4's assertions.

## Consolidated surprises and smallest public changes (none was a blocker; every task finished)
- Silent wire change when a group default moves and the payload is `String`/`()`/`Vec<u8>` (T2, T4): docs warn (AUTHOR_GUIDE 5.2); a warning next to the QUICKSTART exceptions would help. Typed payloads give a good compile error.
- `set_x(None)` vs `maybe_x(None)` on `body x?: Option<T>` (T3): the generated rustdoc should say which sends `null`.
- Native header collisions are run-time Build errors (T6): add to DIAGNOSTICS; wording "this owner or an enclosing one"; mention that an absent optional placement still collides.
- Native key labels in messages and rustdoc use the constant's spelling (T5, T10).
- `from_static` needs lowercase (T5): one sentence.
- `accept: */*` (T1), builder `.header` appends (T9), `reqwest::Error` has no `concord::Error` conversion and `status()` must precede `into_response()` (T8): one-sentence doc notes.
- The line-break / control-character fragment message does not say which character (T6).
- Harness: lock-file pruning and the Riot tail of `support/mod.rs` (Setup).

## Ambiguities in the tasks, preserved
T1/T5 meaning of `<token>` (bare vs prefixed); T2 whether the PUT's 204 means "no output" or `()`; T3 PATCH response unspecified, `?` fields absent vs nullable; T4 which "default" and which "exception"; T5 whether rotation should reach cached handles; T6 which kind of collision and at which stage; T7 no NDJSON/CSV operation exists in the API; T8 what "without assuming JSON" should return; T10 which "documented" call.
