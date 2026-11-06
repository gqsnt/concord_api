# Independent first use: PokéAPI

## Scope and starting state

- Task: application consumer of `concord-sdk-pokeapi`, outcomes A (named detail)
  and B (list traversal and selected detail), using only its public API.
- Requested runner: Codex gpt-6-astra, xhigh. The session identifies this runner
  as GPT-6-based Codex; the exact backend/effort is not independently exposed.
- Supplied worktree was clean at
  `2079f14b29b1632d66c4bf9b7e0584789a976e3b`, not the task's `94d13527`.
  `94d13527` is an ancestor, and `git diff --stat 94d13527 HEAD -- crates scripts
  docs checks` was empty. The supplied worktree is preserved.
- `tests/first_use.rs` is a new file, started empty. SDK source, existing SDK
  tests, and `FRICTION.md` have not been read. Existing SDK fixtures are not
  reused: the README does not point to fixture files.

## Predictions recorded before implementation

### A. Read named Pokémon

1. Configure a caller-owned `concord::reqwest::Client` with no proxy, redirects
   disabled, and a bounded timeout; pass it to `Client::new(...).with_base_url`
   with the loopback origin plus `/api/v2`.
2. `api.pokemon("clefairy").await` should send exactly one bodyless
   `GET /api/v2/pokemon/clefairy/`, with `Accept: application/json` and no auth.
   A numeric ID is passed as a string. A 200 JSON body yields
   `Response<Pokemon>` with typed scalar fields, remaining JSON in `extra`, and
   status/headers/final URL in `meta`.
3. Each method waits for the whole body. Complete JSON and a completed HTTP
   body are both required; even complete JSON followed by premature chunked
   EOF must fail rather than return a Pokémon. No incremental API is promised.
4. Non-2xx returns `Error::Concord` containing `ErrorKind::Status`, metadata,
   and an unread response accessible on the inner error. Error bodies have no
   documented provider schema; synthetic text/JSON rejection bodies must be
   preserved, not mistaken for successful detail.
5. Wrong success Content-Type produces `Metadata`; malformed JSON produces
   `Decode`; incomplete HTTP body produces `Transport`, all retaining the
   received head. A disconnect before the head produces `Transport` without
   metadata. `status()` on the inner error is only populated for status errors.

### B. Follow list pages and retrieve selected detail

1. `api.list(Some(2), Some(0)).await` should send
   `GET /api/v2/pokemon/?limit=2&offset=0`; `list(None, None)` should omit both
   query parameters. Values are `Response<PokemonPage>` with `count`, `next`,
   `previous`, and named `Resource` entries.
2. Keep decoded pages owned by the application. Follow each absolute `next`
   through `api.follow(next)` verbatim after URL parsing, including a changed
   limit and parameter order. An empty page with `next: Some(...)` continues;
   `next: None` alone signals successful exhaustion, including an empty list.
3. Select a named result and use `api.pokemon_url(&resource.url)` for detail.
   Unselected resources are not fetched. No endpoint declaration, operation
   name, manual execution, or direct HTTP request should be necessary.
4. A failed later page or selected detail returns the original SDK error;
   earlier pages/details remain owned by the application. A test application
   will distinguish complete traversal, a local page bound with continuation,
   and failure. A repeated next URL requires caller policy; a page bound will
   prevent an unbounded loop. There is no automatic retry.
5. Out-of-origin, out-of-collection, malformed, fragment, and userinfo URLs
   supplied to `follow`/`pokemon_url` should yield `InvalidUrl` before I/O.

## Ordered action/evidence trace

1. First entry point: `examples/pokeapi-sdk/README.md`, then SDK rustdoc for
   `Client`, `Pokemon`, `PokemonPage`, `Resource`, `Response`, `ResponseMeta`,
   and `Error`. Both rustdoc builds passed with no diagnostics:
   `cargo doc --locked --offline -p concord-sdk-pokeapi --no-deps` and
   `cargo doc --locked --offline -p concord --no-deps --features csv,jsonl`.
2. Public Concord material used: root `README.md`, `docs/CALLER_GUIDE.md`,
   error/metadata sections of `docs/RECIPES.md`, and facade rustdoc for `Error`
   and `ErrorKind`. The SDK README's loops and signatures supply the consumer
   path. None of the five existing provider SDKs supplied a solution.
3. Read `examples/testserver/src/lib.rs` public definitions and lifecycle docs
   to learn `TestServer`, `Recorded`, `Reply`, `Body::FailAfter`, and shutdown.
   This is a test-harness source read, not an SDK implementation/test read.
4. Consulted the official [PokéAPI v2 documentation](https://pokeapi.co/docs/v2)
   on 2026-11-06: Information, Resource Lists/Pagination, and Pokémon. Only the
   documentation page was fetched; no provider API calls were made.
5. Recorded both predictions above before writing either outcome's test code.
   All cargo commands use `CARGO_BUILD_JOBS=5` and the supplied shared
   `CARGO_TARGET_DIR`; dependencies are already cached.
6. Implemented A from the public signatures and ran
   `cargo test --locked --offline -p concord-sdk-pokeapi --test first_use`:
   **7 passed** on the first run. No compiler errors. One `dead_code` warning:
   `UNAVAILABLE` was declared for the not-yet-written B tests. A confirmed
   scalar/nested JSON projection, numeric-string ID, nullable experience,
   recoverable 404/plain-text and wrong Content-Type bodies, `Decode` for
   incomplete JSON with complete HTTP framing, `Transport` for complete JSON
   with incomplete chunked framing, and absent metadata before headers.
7. Starting B using the prediction already recorded above. The application
   helper will own successful page/detail responses and expose a distinct stop
   reason; this is caller traversal policy, using only `list`, `follow`, and
   `pokemon_url`.
8. Implemented B and reran the offline test command: **14 passed**, no warnings
   or compiler errors. Changed next-page limits and query order were preserved;
   the empty middle page continued; only the selected detail was fetched.
   A 503 next page and an unfinished next-page transfer both retained the
   earlier page/detail. A selected-detail 404 failed the workflow even when the
   list's `next` was null. Empty exhaustion and a caller page bound were distinct.
   Fifteen invalid URL cases produced `InvalidUrl` with zero requests to either
   loopback server. No SDK source/test intervention was needed.
9. `rustfmt --edition 2024 --check examples/pokeapi-sdk/tests/first_use.rs`
   requested line wrapping only; applied rustfmt to this new file alone.
   Ran the exact requested command with the required prefix:

   ```sh
   CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-pokeapi --test first_use
   ```

   Result: **14 passed, 0 failed, 0 ignored**, no warnings. The suite only
   contacted loopback servers. Each test checked method, raw target, Host,
   Accept, absent auth/cookies, an empty request body, request count, and server
   cleanup. Both the earlier explicit `--offline` run and this run passed.
10. Scope check: only this record, the new test, and four new fixture files were
    added. No tracked SDK/library files or existing tests changed.
    `git check-attr text` reports `unset` for every new fixture, confirming the
    existing `fixtures/** -text` protection applies.

## Fixture provenance

`fixtures/first_use/clefairy.json` is a hand-authored reduction of the documented
Pokémon shape. Its scalar values and nested ability/type example are based on
the documentation; it is not a captured response or a verbatim full documented
example. `not-found.txt` and `unavailable.json` are invented error bodies with no
claimed provider schema; `incomplete-detail.json` is deliberately invalid JSON.
The nullable-experience variation, named-list documents, other references,
counts, empty intermediate pages, and truncated transfers are synthetic test
scenarios. Lists are constructed as JSON in the test independently of the SDK
models, using the loopback port. This includes the final page whose JSON is
complete but whose chunked HTTP framing is cut short. These tests establish
offline SDK behavior, not live provider conformance.

## Interventions and friction

- **First source-read intervention:** the testserver harness API/lifecycle read
  in trace item 3, to construct the fake provider. No SDK source, existing SDK
  tests, or author friction notes were read at any point. No other provider SDK
  supplied the implementation.
- **Error recovery discoverability:** the README mentions
  `error.into_response()` without showing the outer SDK error match. Rustdoc
  reveals that the method is on the inner Concord error. The working public
  path is `let Error::Concord { error, meta } = failure ...`, then
  `error.into_response()?.bytes().await`. Reproduction:
  `a_not_found_preserves_the_unstandardized_error_body` and
  `b_failed_next_page_keeps_prior_page_detail_and_error_document` in the suite.
  This required navigating two error types, but no failed compile or SDK source
  read. Recovering bytes is a native response-body read, as documented; it does
  not rebuild/re-execute a request and is reasonable for an unspecified error
  document. No unnecessary native execution workaround was needed.
- **Traversal bookkeeping:** the README explicitly leaves automatic traversal
  out of scope. The application therefore owns `Selection`/`Stop` and
  `select_named` to keep prior results and distinguish `Complete`,
  `PageLimit { next }`, and `Failed`. Reproduction:
  `b_application_page_bound_stops_a_cycle_with_a_continuation` and the failed
  next-page tests. This is a visible caller cost, not a contradiction of the
  advertised SDK scope; no Concord pagination adapter was needed for this task.
- **URL rejection diagnostics:** all fifteen malformed, foreign, fragment,
  userinfo, or wrong-path cases return the same fieldless `Error::InvalidUrl`.
  The caller must retain context to explain which input failed; the SDK gives
  no rejection category. Reproduction:
  `b_invalid_continuation_and_detail_urls_are_rejected_without_io`.
- No private reach-through, handwritten operation names, library changes,
  retries, live provider calls, or credentials were used. The working paths are
  the README's `Client::pokemon`, `list`, `follow`, and `pokemon_url`.

## Post-wave-1 integration (coordinator)

The record above is unchanged; `tests/first_use.rs` was edited by the coordinator
for two SDK changes, with no case or expectation changed:

- URL rejection diagnostics (friction above): `Error::InvalidUrl` now carries a
  `UrlRejection` reason, so the two patterns read `Error::InvalidUrl(_)`.
- One adapter for every operation (`INTEGRATION.md`): `Client::pokemon` and
  `Client::list` were replaced by the `Call` trait, so nine call sites read
  `api.pokemon(id).call()`, `api.list().set_limit(2).set_offset(0).call()` and
  `api.list().call()`, and the import adds `Call`. `follow` and `pokemon_url`
  are unchanged.

## Between waves 2-3 (coordinator integration)

The record above is unchanged. The `Call` trait was replaced by the client's send policy
(`INTEGRATION.md`, "Between waves 2-3"), so the coordinator changed the nine `.call()` sites of
`tests/first_use.rs` to `.send()` and dropped `Call` from its import; no case or expectation
changed, and the suite still passes (14 tests).
