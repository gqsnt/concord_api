# Maintenance change: change the page limit while following the server's next URL

## Scope and starting state

- Ticket: "Change page limit while following server next URL". Brief perturbation:
  change `limit` while preserving a server-returned next URL; the consumer follows the
  returned URL rather than inventing it.
- Role: maintenance reviewer; I did not write this SDK. Requested runner: Claude Opus
  subagent. The harness system prompt names the model ID `claude-opus-5-5`; reasoning
  effort is not exposed to the run.
- Worktree branch `sdk20/w1-sdk02-pokeapi`, clean at `729a8dda` (author commit `2079f14b`
  plus first-use commit). The frozen candidate `94d13527` is an ancestor, and
  `git diff --stat 94d13527 HEAD -- crates scripts docs checks` is empty.
- Baseline before any edit: `cargo test --locked -p concord-sdk-pokeapi` passed
  (first_use 14, protocol 6, workflows 3, doc-tests 0).
- `FRICTION.md` and `FIRST_USE.md` were not read until the first attempt passed all tests
  (see "After the first attempt").

## What I read to find the places

1. `src/lib.rs` (whole file), `README.md`, `PROVENANCE.md`, `Cargo.toml`, `.gitattributes`,
   `tests/workflows.rs`, `tests/protocol.rs`, the page fixtures. For `tests/first_use.rs` only
   a grep for `follow`/`limit`/test names, to know which wire targets it pins.
2. Concord docs: `docs/RECIPES.md` (interleaved query parameters, the "an operation the SDK does
   not declare" caution about seeds), `docs/CALLER_GUIDE.md` sections 6a and 7,
   `crates/concord-pagination/README.md`, and `Prepared::request` /
   `Prepared::into_parts` rustdoc in `crates/concord-core/src/prepared.rs`.
3. Other provider SDKs, for comparison only: `examples/github-sdk/src/pages.rs` and the cursor
   helpers in `examples/stripe-sdk/src/pages.rs` (which spell their cursor keys as string
   literals). Neither supplied the solution; the seed-and-replace-URL pattern was already in
   this SDK's `follow`.

The places were found by reading: the SDK is small (one source file) and the adapter for
URL following is a single method. No search beyond the grep above was needed.

## Edits by kind

| Kind | Edit |
| --- | --- |
| Declaration | None. `GET list(query limit?: u64, query offset?: u64)` (`src/lib.rs:73`) already generates `set_limit`. |
| Adapter | `Client::follow_with_limit(next, limit)` (`src/lib.rs:187`): prepares `list().set_limit(limit)`, validates `next` with the existing `checked_url`, splices the declared `limit` pair into the server URL with the new private `replace_pair` (`src/lib.rs:239`), sends through the existing `send`. `follow` is unchanged. |
| Model | None. `PokemonPage`, `Resource`, `Response`, `Error` are unchanged. |
| Tests | `tests/workflows.rs` `caller_changes_limit_while_following_the_server_next_url`: list with `limit=2&offset=0`, server returns `next = ?offset=1&limit=1`, caller follows with limit 3, wire target is `?offset=1&limit=3` (server offset and order kept), `meta.url` matches. `tests/protocol.rs` `follow_with_limit_rewrites_only_the_limit_pair`: exact wire targets for replace-in-place, absent `limit` (appended), no query, repeated `limit` (first replaced, later dropped), an unrelated `x=a%20b` pair kept byte for byte with an empty segment dropped; GET, empty body, `Accept: application/json`; foreign origin, fragment and wrong collection give `InvalidUrl` with no request. Existing tests unchanged. |
| Docs | `README.md`: traversal paragraph describes `follow_with_limit`; the URL-rules paragraph names it among the validating methods and qualifies "not regenerated from `limit` or `offset`". `PROVENANCE.md`: the new hand-authored next URLs are recorded as synthetic. Rustdoc on the new method. |

No fixture file was added: the workflow test reuses `page-first.json` and `page-last.json`
(their `HOST` replacement), and the protocol test reuses `page-empty.json`.

## The same fact in several places

Fact 1, "the page-size query key is `limit`, a decimal integer":

- Declaration, `src/lib.rs:73`: the single source of the wire key and its encoding. Not edited.
- Adapter: deliberately **not** restated. `follow_with_limit` takes the encoded pair from the
  prepared `list().set_limit(limit)` request instead of spelling `"limit"`. This relies on a
  structural fact of the declaration, written as a comment: with only `limit` set, the list
  operation's query is exactly that pair. If a fixed or client-level query pair is ever added to
  `list`, this assumption breaks; the exact-target wire tests would then fail. The alternative,
  a `"limit"` literal as in the Stripe SDK's cursor helpers, would be a second copy of the key.
- Tests, fixtures and README spell `limit` independently, as expected for wire-level evidence.

Fact 2, "position comes from the server's next URL; the SDK does not regenerate the query":

- `README.md` traversal paragraph, `README.md` URL-rules paragraph, `follow` rustdoc and now the
  `follow_with_limit` rustdoc. Two of these are independently maintained prose descriptions that
  had to change: the README sentence "Query strings ... are not regenerated from `limit` or
  `offset`" became inaccurate for the new method and was qualified, and the sentence naming the
  methods that validate URLs had to add the new method. Neither is checked by anything.

Fact 3, "accepted continuation URLs are same-origin, in the list collection, without fragment
or userinfo": one code place (`checked_url`, reused unchanged) plus README prose (edited to name
the new method) and per-method `# Errors` rustdoc (the new method points to `follow`'s).

So one semantic change touched one code place (new adapter method) and two independently
maintained prose descriptions in the README; the declaration and model needed nothing.

## Generated-name and compile surprises

- Generated names: none. `set_limit` was already used by `Client::list`.
- Compile: one error, from Rust rather than Concord. A closure
  `|segment: &str| segment.split_once('=').map_or(segment, |(key, _)| key)` failed with
  "lifetime may not live long enough" (closure signatures do not tie the output lifetime to the
  input). Rewritten as a nested `fn key_of(segment: &str) -> &str`; compiled on the next run.
- Doc-tests: the crate has none; `README.md` code blocks (written with `#` hidden lines) are not
  compiled by any target or script (no example's `src/lib.rs` includes its README, and
  `scripts/`, `checks/` and `.github/` do not mention `pokeapi`). My README change is prose with inline code
  only, so nothing new is uncompiled, but the existing README loop remains unchecked.

## Did it feel local

Yes. The change is additive: one public method and one private helper in `src/lib.rs`, reusing
`checked_url` and `send`. The existing `follow` contract, its tests and the independent
first-use suite needed no change. The non-local part was documentation: README prose restated
"the query is not regenerated" and "which methods validate URLs" in sentences that a new method
silently falsifies.

## Behaviour choices (SDK adapter, not provider-documented)

- Repeated `limit` pairs: the first is replaced in place and later ones dropped, so the request
  carries exactly one `limit`. The provider facts recorded in this SDK do not cover repeated
  parameters.
- Keys are compared as written (`limit`); a percent-encoded spelling of the key would not match
  and the caller's pair would be appended instead. PokéAPI next URLs are not documented to do
  this; not handled further.
- Empty query segments (`&&`) are dropped; every non-empty segment other than `limit` keeps its
  bytes and position. `query_pairs_mut` was not used because it re-encodes every pair.
- `limit` is any `u64`, as in `Client::list`; no new provider bound was invented.
- The provider documentation was not re-fetched for this change; the `limit`/`offset` facts are
  the ones the README and `PROVENANCE.md` already record.

## After the first attempt: comparison with FRICTION.md and FIRST_USE.md

- The author's friction entry 2 says the seed-and-replace-URL pattern came from the GitHub SDK's
  `meta::send_edited`; `follow_with_limit` reuses this SDK's copy of that pattern unchanged.
  Concord's RECIPES caution against rewriting a seed's URL concerns seeds of a *different*
  endpoint; here the seed is the same `list` endpoint, so it does not apply.
- The first-use consumer pinned `follow` with server-changed limits and reordered parameters;
  those 14 tests still pass with no edit. The first-use friction about a fieldless
  `Error::InvalidUrl` applies equally to `follow_with_limit` (same error); not changed.
- Nothing in either file changed the first attempt.

## Friction entries

1. **SDK adapter: query-pair splicing.** Changing one pair of a server URL while keeping the
   others' bytes needs about twenty lines of string handling (`replace_pair`), because
   `Url::query_pairs_mut` can only rebuild the whole query with fresh encoding. Concord has no
   role here and none was expected; reproduction: `follow_with_limit_rewrites_only_the_limit_pair`
   (case `?offset=1&x=a%20b&&limit=1`).
2. **Concord: no named access to a declared query pair.** The adapter cannot ask the prepared
   request "the pair produced by `limit`"; it reads the whole query and depends on the declaration
   setting nothing else for `list` (comment at the call site). Not a workaround through private
   API; recorded because a later declaration edit could invalidate it without a compile error
   (the wire tests would catch it).

No library edit, private reach-through, handwritten operation name or rebuilt execution was
needed.

## Commands and results

All with `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b`:

- `cargo test --locked -p concord-sdk-pokeapi`: first_use 14 passed, protocol 7 passed,
  workflows 4 passed, doc-tests 0; no warnings.
- `cargo clippy --locked -p concord-sdk-pokeapi --all-targets -- -D warnings`: clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-pokeapi --no-deps`: clean.
- `cargo fmt -p concord-sdk-pokeapi --check`: clean.
- Mutation check (temporary, reverted): with the `replace_pair` call removed,
  `follow_with_limit_rewrites_only_the_limit_pair` failed, so the test detects a
  `follow_with_limit` that does not change `limit`.

## Post-wave-1 integration (coordinator)

The public `Client::follow_with_limit` added by this trial was reverted after the wave, in one
commit with its two tests (`follow_with_limit_rewrites_only_the_limit_pair`,
`caller_changes_limit_while_following_the_server_next_url`), its README paragraphs and its
PROVENANCE sentence. Reason: it rewrites the `limit` inside a URL the server returned, which
conflicts with the cohort rule to follow server links as returned. The ticket's ordinary case
(change the first request's `limit`, then follow the server's `next` unchanged) is already
covered by the author's `tests/workflows.rs::change_limit_follow_next_and_fetch_detail`. The
record above stays as the evidence of the trial; its line references describe the trial's
state, not the current one.
