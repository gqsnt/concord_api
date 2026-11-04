# Concord 4.2 candidate — documentation-first trial report (Brief E, agent simulation)

- Participant: Claude Opus 5.5 (`claude-opus-5-5`, as stated by the harness); effort level not
  observable from inside the run. Agent simulation, not human research. No sub-agents spawned.
- Snapshot: `/home/canarit/projects/concord-v42/trial-snap` (read-only). Trial crate:
  `/home/canarit/projects/concord-v42/trials/opus` (standalone `[workspace]`, path dependency on
  `trial-snap/crates/concord`, `rust-toolchain.toml` = 1.99.1, `Cargo.lock` copied from the
  snapshot so that `cargo --offline` works; all cargo runs via `tools/heavy.sh`).
- Run everything: `tools/heavy.sh cargo run --offline -q` in the trial directory (one binary,
  modules `t1`..`t5`, a hand-written localhost server in `src/server.rs`).
- Note: the crate under test reports version `4.1.0` (README "Status", `Cargo.toml`), although
  the brief calls the snapshot the 4.2 candidate. Not a defect for this trial, but a reader of the
  README cannot tell they are on a 4.2 candidate.

## Task 1 — required root value, construction, generated `Config`

Lookups: README (Example), QUICKSTART §3 ("Read it top-down", first bullet; signature list),
then after writing: AUTHOR_GUIDE §2 (table + "Required client values"), CALLER_GUIDE intro and §1.

Wrote (`src/t1.rs`):

```rust
concord::api! {
    pub client Demo(#[sensitive] token: String) {
        base "https://demo.example.test/api";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;
        GET ping() { path "ping"; }
    }
}
let api = Demo::new(http, DemoConfig { token: "dummy-token".into() });
let api = api.set_token("other-dummy");
```

Result: compiled first time; `Debug` prints `Demo { base: "...", token: <redacted> }`; the
prepared request carries `authorization: Sensitive` in reqwest's header `Debug`.

`Config`: yes, `DemoConfig { token }` is generated because the client has a required value;
only required client values become fields (optional/initialized client values are set after
`new` with `set_*`; a client without required values has no config and uses `Client::new(http)`).
Field types are exact (`.into()` needed for a literal), unlike positional arguments which take
`impl IntoValue<T>`. The *why* (named struct for client values vs. positional args for scope and
endpoint values) is not stated anywhere I read; I infer it is about readability of the
construction site and E0063-by-name for missing fields (AUTHOR_GUIDE §2 mentions the E0063).

Surprise (not a library defect, but a doc gap): `use concord::Json;` was reported by rustc as
an **unused import** in this client, because its only endpoint has no body and no output.
AUTHOR_GUIDE §5.2 explains that statements nothing uses "resolve nothing", but neither the
QUICKSTART ("`Json` is an ordinary type, imported in §2") nor §5.2 says that this surfaces as an
`unused_imports` warning. The warning disappeared in task 2 once an endpoint had a typed output.

Prediction accuracy: all predictions held except the unused-import warning, which I had not
anticipated (I had predicted a risk around a client without endpoints instead).

## Task 2 — `format Json;`, typed output, no output, `-> ()`

Lookups: QUICKSTART §3 (`format Json;` bullet), AUTHOR_GUIDE §5 intro, §5.1, §5.2, §5.5.

Wrote (`src/t2.rs`): `use concord::Json;` + `format Json;` on the client;
`GET item(id: u64) -> Item`, `DELETE remove(id: u64)` (no output), `POST touch(id: u64) -> ()`;
a localhost server with canned responses.

Results (all as documented in AUTHOR_GUIDE §5.5 and §5.8):

| Call | Server answer | Result |
|---|---|---|
| `item(1)` | 200 `application/json` `{"id":1,"name":"pen"}` | `Ok(Item{..})` |
| `remove(1)` (no output) | 204, no Content-Type | `Ok(())` |
| `remove(2)` (no output) | 200 `text/html` | `Ok(())` |
| `touch(1)` (`-> ()`) | 204, no Content-Type | `Err(Metadata)` |
| `touch(2)` (`-> ()`) | 200 `application/json` `null` | `Ok(())` |
| `touch(3)` (`-> ()`) | 200 `application/json`, empty | `Err(Decode)` |

On the wire: typed outputs (including `-> ()`) send `accept: application/json`; the no-output
endpoint sends reqwest's `accept: */*` (QUICKSTART claim verified).

Difference, in my words: both methods return `Result<(), Error>`; the difference is entirely at
runtime. No `->` means "no typed output": any 2xx succeeds, the body is drained, no format is
consulted and no `Accept` is suggested. `-> ()` is an ordinary typed output decoded by the
format: under `Json` the response must be labelled JSON and contain `null`, so a 204 is a
`Metadata` error and an empty JSON body a `Decode` error.

Prediction accuracy: all held (including my guess that the no-`->` endpoint gets `accept: */*`
from reqwest). The docs were sufficient; no repairs needed.

## Task 3 — borrowed `&str` and parsed values as arguments

Lookups: CALLER_GUIDE intro (paragraph after the signature list, read during task 1),
QUICKSTART §3 (signature note), afterwards DIAGNOSTICS.md (grep for `IntoValue`/`E0283`, ~l.130–160).

Wrote (`src/t3.rs`): client `Hub` with a sensitive header key, `scope org(org: String)`,
`GET member(user: UserId, query role?: String, query page: u32 = 1)` (`UserId` a `Display`
newtype). Working calls: `api.org(org_name /* &str */)`, `api.org(&owned /* &String */)`,
`.member(UserId("42".parse::<u64>().unwrap()))`, `.set_role(org_name)`,
`.set_page(page_text.parse::<u32>().unwrap())`, and a typed `let page: u32 = ..parse()..`.
Output URL: `.../orgs/acme/members/42?role=acme&page=3`.

Deliberate failures (compiled one at a time via a throwaway `src/bin/t3fail.rs`, since removed):

| Variant | Result | Matches docs? |
|---|---|---|
| `api.org("acme".into())` | E0283 (two errors: `IntoValue<String>` ambiguity, and `From<&str>` impls in `bytes`/`reqwest`) | yes (CALLER_GUIDE intro) |
| `.set_page("3".parse().unwrap())` | E0283 "cannot infer type of the type parameter `F` ... on `parse`" | yes |
| `.set_page(Default::default())` | E0283, names `impl ::concord::__macro::concord_core::IntoValue<u32>` | yes |
| `.set_page(p_u16)` | E0277 "`u16` is not accepted for a value of type `u32`" + Concord note | yes (DIAGNOSTICS) |
| `.member(42u64)` (newtype) | E0277 "`u64` is not accepted for a value of type `UserId`" | yes |
| `HubConfig { key: "k" }` | E0308 expected `String`, found `&str` | yes (Config fields exact) |

Inference trouble: exactly the documented one. `.into()` / un-annotated `.parse()` /
`Default::default()` as *arguments* are E0283 because the parameter is `impl IntoValue<T>`;
annotate (`parse::<u32>()`) or bind to a typed `let`. In the Config struct literal the reverse
holds: `.into()` is needed. The E0277 diagnostics are good (custom `on_unimplemented` note).

Observations:
- The diagnostics' note says `&HeaderValue` is accepted for `HeaderValue` (also DIAGNOSTICS.md
  l.131/146), but QUICKSTART §3, CALLER_GUIDE intro ("Nothing else converts") and AUTHOR_GUIDE §2
  list only `&str`/`&String`. Small doc inconsistency.
- rustc's E0283/E0277 output points into `crates/concord-core/src/value.rs:67` and shows the
  hidden path `::concord::__macro::concord_core::IntoValue<u32>`; noise, not a defect.
- Prediction accuracy: all held (prediction was informed by the CALLER_GUIDE paragraph). The
  only detail I did not predict was the second E0283 for `.into()` (the `From<&str>` ambiguity).

## Task 4 — recovering the unread response (2xx refused, non-2xx)

Lookups: QUICKSTART §4 ("Errors" bullet), README "Formats", then CALLER_GUIDE §6 (table,
"A status error keeps the unread response", "A 2xx response the format refuses").

Wrote (`src/t4.rs`): `Shop` with `format Json;` and `GET item(id: u64) -> Item`; local server
routes: `items/1` 200 `text/html; charset=utf-8`, `items/2` 404 `application/problem+json`
`{"title":"no such item"}`, `items/3` 200 `application/json` `{not json`.

Results:

```text
t4: 2xx kind=Metadata status=None display=GET Shop.item: response metadata not accepted: the response Content-Type is not application/json or an application/*+json type
t4: 2xx recovered status=200 OK ct="text/html; charset=utf-8"
t4: 2xx body="<p>maintenance</p>"
t4: 404 kind=Status status=Some(404) display=GET Shop.item: HTTP status 404 Not Found
t4: 404 recovered 404 Not Found: Problem { title: "no such item" }
t4: decode kind=Decode -> into_response Err(kind=Decode): GET Shop.item: response not decoded: the body is not valid JSON (line 1, column 2)
```

Recovery code: `error.into_response()?` then `response.text().await?` (2xx) or
`<Json as Decode<Problem>>::decode_body((), response.bytes().await?)?` (non-2xx).
Compiled and ran first time.

Prediction accuracy: one prediction was wrong, and the docs were right: I expected
`error.status()` to be `Some(200)` for a `Metadata` error; CALLER_GUIDE §6 says (and the run
confirms) `status()` is `Status`-only and `None` for `Metadata`. In hindsight the QUICKSTART
match arm `Err(error) if error.status().is_some()` is therefore *correct* for "only non-2xx", but
nothing in the QUICKSTART says `status()` is `None` for `Metadata`; its §4 "Errors" bullet
instead warns "look at `error.kind()` first if a 2xx must not be taken for an error document",
which made me believe the shown arm was the risky one. Suggest one clause in QUICKSTART §4:
"(`status()` is `Some` only for `Status` errors, so the arm above never sees a `Metadata` error)".

Addendum to task 1 (rustdoc, `cargo doc --no-deps` on the trial crate): the generated
`DemoConfig` page says "The required values of `Demo`, passed to `Demo::new`. Optional and
initialized values are set afterwards with the client's `set_*` and `maybe_*` methods", and the
field doc says "Sent as: header AUTHORIZATION. Sensitive: ...". `DemoConfig`'s own `Debug`
prints `DemoConfig { token: <redacted> }` (checked at runtime). Rustdoc also does not give a
rationale for a struct vs. positional arguments.

## Task 5 — moving a shared default into the enclosing scope

Lookups: AUTHOR_GUIDE §4.1–§4.7 (placements, blocks, inherited-key operations, `else inherit`,
wire order), §11 table and §11.1 "Moving a default into a scope" (the worked example is almost
exactly this task).

Before (`src/t5.rs`, `mod before`): in `scope repo(repo: String)`, `issues` and `pulls` each
declare `query per_page: u32 = 50` and `header("x-api-version") ver: String = "2024-01"`;
`stats` declares `query per_page: u32 = 10` and `ver = "2023-06"`. Client adds `query { "v" = "2"; }`
and a sensitive `x-api-key`.

After (`mod after`): the scope gets `query { "per_page" = "50"; }` and
`headers { "x-api-version" = "2024-01"; }`; `issues`/`pulls` turn their values into optional
`per_page?: u32`, `ver?: String` with `override "per_page" = per_page else inherit;` (and the same
for the header), so `set_per_page`/`set_ver` keep compiling; `stats` keeps initialized
`per_page: u32 = 10`, `ver: String = "2023-06"` with `override ..;` (always present).

Prepared requests (query | headers in `HeaderMap` iteration order):

```text
before issues()            v=2&per_page=50 | x-api-key=<s>, x-api-version=2024-01, x-trace=on
after  issues()            v=2&per_page=50 | x-api-key=<s>, x-api-version=2024-01, x-trace=on
before issues+state        v=2&state=open&per_page=50 | ...
after  issues+state        v=2&per_page=50&state=open | ...          <- order changed
before pulls+per_page(5)   v=2&per_page=5 | x-api-key=<s>, x-api-version=x
after  pulls+per_page(5)   v=2&per_page=5 | x-api-key=<s>, x-api-version=x
before stats+window        v=2&window=7d&per_page=10 | ..., x-api-version=2023-06
after  stats+window        v=2&per_page=10&window=7d | ..., x-api-version=2023-06   <- order changed
before stats+per_page(3)   v=2&per_page=3 | ..., x-api-version=2023-06
after  stats+per_page(3)   v=2&per_page=3 | ..., x-api-version=2023-06
```

Same pairs and header values everywhere; `per_page` moves to the scope's position (ahead of the
endpoint's own `state`/`window`), also for `stats`, which overrides it — exactly as §4.7/§11.1
say. Header presence and values are unchanged; header order between names is unspecified
(§4.7), so I compared presence/values only. Calling code compiled unchanged; `maybe_per_page` /
`maybe_ver` are new on `issues`/`pulls`.

Compiled first time. Mistake variants (each compiled, then reverted):

- `stats` with `"per_page" = per_page;` (no `override`) -> compile error "query key `per_page`
  is already set by `Hub.repo`; use `override` or `append` ..., or add `remove "per_page";`".
- `issues` with `override "per_page" = per_page;` on an optional value -> compile error "this
  `override` reads an optional value, so it can be absent; say what absence means ...".
- `issues` keeping its old direct placement `query per_page: u32 = 50` -> the same "already set"
  error, pointing at the placement.
All three messages are precise and name the fix.

Prediction accuracy: the shape (scope block + `override`, compile error without it, query
position moving ahead) was right. I had not anticipated that a sibling which wants to keep
`set_per_page` must switch from an initialized value to an optional value with
`else inherit` — §11.1 shows that; my prediction assumed the setter would vanish or move.

## Mistakes of my own vs. library/documentation defects

- Own fixture mistakes: none that reached the compiler; the hand-written server worked first
  time. The deliberate compile failures in tasks 3 and 5 were deliberate.
- Library defects: none found. Every documented behaviour I exercised (Config, redaction,
  `Accept`, no output vs `-> ()`, Metadata/Status recovery, Decode not recoverable, inherited-key
  errors, query order) matched the run.
- Documentation-fallback rule: I did not read any implementation source, test, `v4/` record or
  review report. I read `crates/concord/Cargo.toml` (feature list, to set up the dependency)
  — manifest, not implementation. rustc diagnostics themselves printed lines from
  `crates/concord-core/src/value.rs` (unsolicited).

## Documentation issues

1. **QUICKSTART.md §3 (`format Json;` bullet) / AUTHOR_GUIDE §5.2** — told to `use concord::Json;`,
   but a client whose endpoints have no body and no typed output makes rustc warn `unused import:
   concord::Json` (task 1). §5.2 says unused statements "resolve nothing" but not that this shows
   as `unused_imports`. Fix: one sentence in §5.2 (and a parenthesis in QUICKSTART): "if no
   endpoint has a body or a typed output, the format is not referenced and rustc reports its
   import as unused; drop the statement or the import."
2. **QUICKSTART.md §3 signature note, CALLER_GUIDE.md intro ("Nothing else converts"),
   AUTHOR_GUIDE §2 code comment** — say `IntoValue<T>` accepts exactly `T` or `&str`/`&String`
   for `String`; the compiler note and DIAGNOSTICS.md (~l.131, 146) also list `&HeaderValue` for
   `HeaderValue`. Fix: add the `&HeaderValue` case to those three places.
3. **QUICKSTART.md §4 ("Errors" bullet and the `match` in `run`)** — the warning "look at
   `error.kind()` first if a 2xx must not be taken for an error document" next to an arm guarded
   by `error.status().is_some()` led me to predict that arm also catches `Metadata`. It does not:
   `status()` is `None` for `Metadata` (stated only in CALLER_GUIDE §6). Fix: say in QUICKSTART
   §4 that `status()` is `Some` only for `Status` errors, so the shown arm is already 2xx-safe.
4. **QUICKSTART.md §3 / CALLER_GUIDE §1 / AUTHOR_GUIDE §2** — what `{Client}Config` is and when it
   exists is clear, but *why* required client values are a named struct while scope/endpoint
   required values are positional is not stated anywhere (nor in rustdoc). Fix: one sentence of
   rationale in AUTHOR_GUIDE §2.
5. **AUTHOR_GUIDE §11 table** — has rows for "Move a value between owners: breaks" and "Move a
   literal ... entry into the enclosing scope: unchanged", but the common case in task 5 (an
   *initialized endpoint value* becoming a scope literal plus an optional `override .. else
   inherit` on the endpoint, which keeps calling code compiling) is only in §11.1's prose and
   example. Fix: add a row pointing to §11.1 for that edit, noting `maybe_x` is new.
6. **README.md "Status"** — reports version 4.1.0 and the `v4.1.0` tag; nothing identifies the
   snapshot as the 4.2 candidate. (May be intentional for a candidate; mention if not.)
7. Minor: README's documentation table links "Semantics" to `v4/SEMANTICS.md`, and AUTHOR_GUIDE
   §4.9 sends readers there for the build order — a user guide depending on a records directory
   (I did not follow it; not needed for any task).

## Remaining confusion

- None blocking. Header order across names is unspecified by design, so "check order" in task 5
  can only be checked for query keys and for repeated values of one header name.
