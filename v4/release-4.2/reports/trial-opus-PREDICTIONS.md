# Predictions (Brief E, documentation-first trial)

Participant: Claude Opus 5.5 (model id claude-opus-5-5, as stated by the harness); effort level not observable.
Read before the first prediction: brief, README.md, docs/QUICKSTART.md (complete).

## Task 1 — client with a required root token; construct it; is a `Config` generated, why?

Expectation (written before any further lookup):

```rust
concord::api! {
    pub client Demo(#[sensitive] token: String) {
        base "https://demo.example.test/api";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;
        ...
    }
}
let api = Demo::new(http, DemoConfig { token: "t".into() });
```

- A `DemoConfig` struct is generated, with one public field `token: String`, because the client
  has a *required* value (QUICKSTART §3: "Only required client values become config fields; a
  client without any has no config struct and is created with `Client::new(http)` alone").
  Rationale I expect: a required value has no default, so the constructor must receive it, and a
  named struct keeps call sites readable/stable when more required values are added (vs. positional args).
- `Debug` of the client prints `token: <redacted>`.
- A `set_token(..)` setter exists on the client (QUICKSTART signature list).
- I expect it to compile first try; risk: a client with no endpoints might be rejected or
  warn, so I will add at least one endpoint.

## Task 2 — `format Json;` imported normally; typed JSON output endpoint + no-output endpoint; no output vs `-> ()`

Written after task 1 (lookups so far: README, QUICKSTART, AUTHOR_GUIDE §2, CALLER_GUIDE intro + §1).

- Import: `use concord::Json;` and `format Json;` in the client block. Task 1 showed a
  surprise: with only a no-output endpoint, `use concord::Json;` is reported *unused* by rustc,
  so the macro apparently only references the format path where an endpoint needs it. With a
  typed output endpoint I expect the warning to disappear.
- `GET item(id: u64) -> Item { path "items/{id}"; }` decodes JSON, sends `accept: application/json`.
- `DELETE remove(id: u64) { path "items/{id}"; }` returns `Result<(), Error>`, decodes nothing,
  accepts any 2xx with any/no Content-Type, sends no `accept` from Concord (reqwest's `*/*`? — the
  QUICKSTART says reqwest sends its default `accept: */*`; I am not sure reqwest actually does
  that by default, will check on the wire).
- `-> ()`: the format decodes `()`; for `Json` I expect an empty body to fail (serde_json cannot
  decode `()` from empty input; it needs `null`) and a body with `Content-Type` other than JSON to
  be a `Metadata` error; and `accept: application/json` is sent. Both have the same Rust return
  type `Result<(), concord::Error>`, so the difference is only runtime behaviour.

## Task 3 — borrowed `&str` and a parsed value as arguments

Written after tasks 1–2. Caveat: I already read the CALLER_GUIDE intro (before §1) during task 1,
which says `impl IntoValue<T>` accepts exactly `T` or `&str`/`&String` for `String`, and that
`.into()`, `.parse().unwrap()` and `Default::default()` as arguments are E0283. So this
prediction is informed, not naive.

- `api.shop(name)` with `name: &str` compiles (IntoValue<String> for &str); `&String` too.
- `api.item(s.parse().unwrap())` where the parameter is `u64`: E0283 "type annotations needed";
  fix `s.parse::<u64>()`.
- `api.shop("acme".into())`: E0283; fix: pass `"acme"` directly.
- `.into()` *does* work inside the `{Client}Config` struct literal (exact field type).
- A `u32` value passed where `u64` is expected: E0277 (no widening).
- A newtype parameter, e.g. `UserId(u64)`: `api.user(7)` will not compile; needs `UserId(7)`.

## Task 4 — 2xx with refused Content-Type; recover the unread response; same for non-2xx

Written after task 3. Read so far on this topic: README Formats, QUICKSTART §3/§4 ("Errors"),
AUTHOR_GUIDE §5.1/§5.5. Not yet read: CALLER_GUIDE §6.

- Server answers `GET item` with 200 `text/html` `<p>hi</p>` -> `send()` gives
  `Err(e)` with `e.kind() == ErrorKind::Metadata`, `e.status() == Some(200)` (I think status()
  is `Some` for Metadata too; QUICKSTART uses `error.status().is_some()` as the "has a response"
  test, which I suspect also catches Metadata — so the QUICKSTART match arm would take a 2xx
  Metadata response as an error document; the QUICKSTART warns about that).
- `e.into_response()` -> `Result<reqwest::Response, Error>`; `Ok(resp)`; `resp.text().await`
  returns `<p>hi</p>`.
- Non-2xx: 404 with `application/json` `{"title":"nope"}` -> `ErrorKind::Status`,
  `into_response()` gives the response; decode with `<Json as Decode<Problem>>::decode_body((), bytes)`.
- A Decode error (bad JSON with right Content-Type) -> `into_response()` returns `Err(original)`.
- `ErrorKind` is at `concord::ErrorKind` (guess).

## Task 5 — move an endpoint-level default header/query into an enclosing scope; third sibling keeps its own

Written after task 4. Read so far: QUICKSTART, AUTHOR_GUIDE §2 and §5, CALLER_GUIDE §1/§6; only
the AUTHOR_GUIDE heading list for §4 and §11.1 ("Moving a default into a scope"), not their text.

Before (my guess at the syntax, from QUICKSTART/§2 examples):

```text
scope repo(repo: String) {
    path "repos/{repo}";
    GET issues(query per_page: u32 = 50, header("x-api-version") ver: String = "2024-01") { .. }
    GET pulls (query per_page: u32 = 50, header("x-api-version") ver: String = "2024-01") { .. }
    GET stats (query per_page: u32 = 10, header("x-api-version") ver: String = "2023-06") { .. }
}
```

After: either a nested scope `scope listing { headers {..}; query {..}; GET issues..; GET pulls.. }`
— which changes the navigation path (`repo.listing().issues()`), a breaking change — or put the
default on the `repo` scope (`scope repo(repo: String, query per_page: u32 = 50, ...)` or a
`query { per_page = 50; }` / `headers { "x-api-version" = "2024-01"; }` block) and have `stats`
override it, probably with an `override` keyword (I saw `override "accept-language" = ...` in the
CALLER_GUIDE declaration). I expect:
- An endpoint redefining an inherited key without `override` is a compile error.
- Wire order: inherited (outer) keys first, then endpoint keys; after the move the query order of
  `per_page` relative to other endpoint query params may change (outer first). Header order
  likewise.
- Setter location changes: `set_per_page` moves from the endpoint request type to the scope
  handle if declared as a scope value — a caller-visible API change; using a block literal
  (no value) removes the setter entirely. §11.1 probably discusses this trade-off.
