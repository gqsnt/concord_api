# Fresh-use trial: Atelier client with Concord 4.0.0-alpha.1 (snapshot fresh-34bc6b6)

**This is an agent simulation, not a human study.** A single LLM agent role-played a Rust developer
meeting Concord for the first time, following `fresh/FRESH_USE_resolved.md`.

- Model: Claude Opus 5.5 (`claude-opus-5-5`), running as a Claude Code subagent.
- Effort setting: **not observable** from inside the run (no effort value is exposed to the agent).
- Crate: `/home/canarit/projects/concord_v4_native_work/fresh/trial` (path dependency on the snapshot,
  feature `form`). Snapshot not modified. No Concord source, tests, example sources or macro expansion
  were read. No server left running (the loopback servers live inside the test processes).

## Final declaration

```rust
concord::api! {
    /// The Atelier artwork API.
    pub client Atelier(#[sensitive] token: String, header("accept") accept: HeaderValue) {
        base "https://atelier.example.test/v3";
        headers { "authorization" = "Bearer {token}"; }
        request json;
        response json;

        GET list_artworks(query artist?: String, query page?: u32) -> Vec<Artwork> {
            path "artworks";
        }
        POST create(body artwork: NewArtwork, header("x-signature") signature?: HeaderValue) -> Artwork {
            path "artworks";
        }

        scope artwork(id: u64) {
            path "artworks/{id}";

            GET get() -> Artwork {}
            DELETE delete(header("x-signature") signature: HeaderValue) {}
            PATCH patch(json("application/merge-patch+json") patch: serde_json::Value) -> Artwork {}
            GET notes() -> text String { path "notes"; }
            PUT upload_image(bytes("image/png") png: Vec<u8>) { path "image"; }
            GET image() -> bytes Vec<u8> { path "image"; }
        }

        POST search(body query?: SearchQuery) -> Vec<Artwork> { path "search"; }
        POST login(form credentials: Login) -> TokenResponse { path "session"; }
        GET export() -> Csv Vec<Row> { path "export"; }
    }
}
```

`Csv` is a unit struct with `impl Decode<Vec<Row>> for Csv` (header line skipped, `split_once(',')`,
errors as `BodyError::new(..).with_source(..)`). `x-signature` is a per-request value (required on
`delete`, optional on `create`). The per-request timeout (T9) is caller code via `into_parts`.

## Lookup log (in order)

| # | Source | Section / item | Why | ~Lines |
|---|---|---|---|---|
| 1 | `README.md` | whole (mandated) | orientation | 166 |
| 2 | `docs/QUICKSTART.md` | whole (mandated) | first declaration | 253 |
| 3 | rustdoc of own crate (`cargo doc --no-deps`) | `Atelier`, `AtelierSearch`, `AtelierArtworkPatch` | confirm what the blind first draft generated: accept-header value, optional body (`set_query`/`maybe_query`, "Absent: no body, no Content-Type"), patch setter | ~60 (text-extracted) |
| 4 | `docs/AUTHOR_GUIDE.md`, `RECIPES.md`, `CALLER_GUIDE.md` | `grep '^#'` headings only | find sections for media types, custom formats, sensitivity, timeouts | ~70 |
| 5 | `docs/AUTHOR_GUIDE.md` | 5.1 The formats, 5.2 Defaults, 5.3 Media types and Content-Type | merge-patch media type (T3), `image/png` (T4) | 100 |
| 6 | `docs/AUTHOR_GUIDE.md` | 5.7 Custom formats | CSV format (T7): `Decode<T>`, `BodyError` | 58 |
| 7 | `docs/AUTHOR_GUIDE.md` | 4.1 Placing a value, 4.2 Blocks | per-request `header("x-signature") sig: HeaderValue` (T8) | 58 |
| 8 | `docs/AUTHOR_GUIDE.md` | 7 Sensitive values | does a pre-flagged `HeaderValue` stay redacted without `#[sensitive]`? (T8) | 53 |
| 9 | `docs/RECIPES.md` | Error documents; Streaming (skimmed); A per-request timeout | T9 `into_parts` + `timeout_mut`; T10 confirmation | 42 |

Total ≈ 860 lines, of which 419 mandated. Not needed: `DIAGNOSTICS.md` (no macro compile error
occurred), `CALLER_GUIDE.md` body, Concord's own rustdoc (not built with `--no-deps`; `Prepared`,
`Error`, `Decoder` method names were guessed and confirmed by the compiler), any source.

## Incorrect predictions

1. **Lockfile:** expected the copied workspace `Cargo.lock` to satisfy `--offline --locked`; it
   failed ("cannot update the lock file") because it has no entry for the new root crate.
2. **First draft (before any guide) would have been silently wrong**, not a compile error: written as
   `PATCH patch(body patch: Value)` and `PUT upload_image(bytes data: Vec<u8>)`, it compiled and
   would have sent `application/json` and `application/octet-stream`. I predicted media-type
   overrides would need a `content-type` header; the actual syntax is in place:
   `json("application/merge-patch+json")`, `bytes("image/png")`.
3. **`Decoder::finish` is async.** RECIPES writes `decoder.finish(client.execute(request).await)`,
   which reads as synchronous; the code needs `decoder.finish(..).await` (I wrote it, it compiled).
4. Predicted that `x-signature` might need `#[sensitive]` on the per-request value; it does not —
   a pre-flagged native `HeaderValue` stays flagged and redacted (and `x-signature` is *not* on the
   name list, only `signature`). Confirmed by test, not an error, just an uncertainty.

Predictions that held without a lookup: `body query?: T` for an optional body (by analogy with
`query done?:`), `form x: T` with feature `form`, `-> text String`, `-> bytes Vec<u8>`, endpoint
without `->` for 204, `ErrorKind: PartialEq`, `Error::status()`, `Error::transport()`.

## Workarounds

- Ran `cargo build --offline` once (without `--locked`) to add the trial crate to the copied lockfile;
  all dependency versions stayed as pinned (reqwest 0.13.5). Every later command used
  `--offline --locked`. A crate-local `rust-toolchain.toml` copy and an empty `[workspace]` table
  were added so the crate stands alone.
- None needed in Concord usage itself.

## Task results

15 tests in `tests/atelier.rs`, all passing (`cargo test --offline --locked`); clippy clean.

| Task | `prepare()` checks | Loopback-server checks | Result |
|---|---|---|---|
| T1 client, token, accept, local server | URL, `authorization` value + `is_sensitive()`, `accept`, client/`Prepared` `Debug` without token, `with_base_url` | headers on the wire | pass |
| T2 list/fetch, optional query | no query / `artist=Ada+Lovelace&page=2` / `maybe_artist(None)` | target `/v3/artworks?artist=Ada`, decode | pass |
| T3 create, merge-patch, delete 204 | method, JSON body bytes, `content-type: application/merge-patch+json`, no body on DELETE | 204 with no output accepted, patch Content-Type on wire | pass |
| T4 text notes, PNG up/down | `image/png`, body bytes, PUT | `String` with raw quotes, PNG bytes on wire, bytes decoded | pass |
| T5 search with/without body | none: no body and no Content-Type; some: JSON bytes + Content-Type | empty body / JSON body on wire | pass |
| T6 form login | `application/x-www-form-urlencoded`, `username=ada&password=p%26w%3D1` | same body on wire, token decoded | pass |
| T7 CSV | — | `Vec<Row>` via own `Csv` decoder | pass |
| T8 signature | value sent, `is_sensitive()`, absent from request `Debug`, `Prepared` `Debug`, native `Request` `Debug`; optional variant absent unless set | header on wire | pass |
| T9 per-request timeout | — | 200 ms override → `ErrorKind::Transport` + `is_timeout()`; same endpoint without override succeeds (1.5 s) | pass |
| T10 404 problem | — | `ErrorKind::Status`, `status() == 404`, error `Debug` without token, `into_response` + `Json` decode | pass |

## Three most confusing points

1. **Media-type overrides are invisible from README/QUICKSTART** and their absence is not an error:
   a plausible first draft compiles and sends the wrong `Content-Type`. The rustdoc does state the
   Content-Type per endpoint, which would catch it if one reads it.
2. **Two ways to send the accept header** (`header("accept") accept: HeaderValue` in the parameter
   list vs `"accept" = accept;` in a `headers` block) — both compiled; the guide never says which to
   prefer. The fact that "Concord never sends `Accept`" is buried at the end of AUTHOR_GUIDE 5.3.
3. **Leaving the typed world for one setting** (T9): `into_parts` → native request → `finish(..).await`.
   Clear in RECIPES, but the snippet hides that `finish` is async, and the trade-off (native `Debug`
   no longer redacts unflagged values) is only in the "Assumes" line.

Minor: the trial instruction to copy the workspace lockfile and use `--locked` conflicts with Cargo's
behaviour for a new root crate.

## What would have prevented each lookup

| Lookup | Prevented by |
|---|---|
| 3 rustdoc check of optional body / accept | QUICKSTART signature list showing an optional body (`set_x`/`maybe_x`) and a `HeaderValue` client value |
| 4 headings grep | README "Documentation" table linking to section anchors (formats, media types, custom formats, sensitivity) |
| 5 media types | one QUICKSTART bullet: "a different media type is written in place: `json(\"application/merge-patch+json\") p: T`, `bytes(\"image/png\") b: Vec<u8>`" |
| 6 custom format | README already names custom formats as a RECIPES topic; a 6-line `impl Decode<T> for MyFormat` sketch in QUICKSTART's format bullet would suffice |
| 7 per-request header | QUICKSTART table of placements (`query x`, `header("name") x`) — only `query` appears there |
| 8 pre-flagged HeaderValue | generated rustdoc saying "Sensitive: native flag kept" on a `HeaderValue` header parameter |
| 9 timeout recipe | README's transport-boundary paragraph already nearly covers it; adding `timeout_mut()` and `decoder.finish(..).await` there would make the lookup unnecessary |
