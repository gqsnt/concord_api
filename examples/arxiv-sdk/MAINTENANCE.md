# Maintenance — typed query builder and sort options

Maintainer: an independent maintenance role. I did not write this SDK or its first consumer.
Date: 2026-11-07. Baseline: the worktree at `fd02861e` (author `71d9f800`, consumer `fd02861e`,
frozen Concord `c3fd4f1e`). **Live provider calls: NOT_RUN.** Every request in this change
went to `concord-example-testserver` on loopback. No shared crate, check, script, other
example, `Cargo.toml` or `Cargo.lock` was edited. The lockfile is byte-identical to its
state before the change, and no new crate was added.

## Ticket and provider contract

The ticket: callers need precise searches. They need field prefixes, `AND`/`OR`/`ANDNOT`,
grouping, escaped or quoted terms, plus `sortBy`/`sortOrder`. Paging, pacing and the
30,000 window must work as they do for term search, and term search and fetch-by-IDs must
not change.

Source: <https://info.arxiv.org/help/api/user-manual.html>, downloaded once on
2026-11-07 at 07:08 UTC (HTTP 200, 147,672 bytes) into `target/tmp/` and read as text.
The sections used:

| Section | Contract used |
| --- | --- |
| §3.1.1 table | `search_query`, `id_list`, `start` (default 0), `max_results` (default 10). |
| §3.1.1.2 | 0-based `start`; "a 3 second delay" between calls; at most 30,000 results, in slices of at most 2,000. |
| §3.1.1.3 | `sortBy` values `relevance`, `lastUpdatedDate` and `submittedDate`; `sortOrder` values `ascending` and `descending`. Sample: `search_query=ti:"electron thermal conductivity"&sortBy=lastUpdatedDate&sortOrder=ascending`. No default order is stated. |
| §5.1 | Prefix table `ti au abs co jr cat rn id all`. `id` carries the note "use id_list instead". The Boolean operators are `AND`, `OR` and `ANDNOT`. Parentheses (`%28`/`%29`) group "for Boolean operator precedence"; double quotes (`%22`) make phrases; a space (`+`) separates terms. Examples: `au:del_maestro+ANDNOT+%28ti:checkerboard+OR+ti:Pyrochlore%29` and `ti:%22quantum+criticality%22`. |
| §5.1 | The `submittedDate:[YYYYMMDDTTTT+TO+YYYYMMDDTTTT]` date filter. **Not implemented** (see below). |
| §3.1.1.3 | `relevance` is described as "Apache Lucene's default RELEVANCE ordering". This is the only hint about the parser. |

The manual describes **no escape character** for terms. Its only quoting tool is the
double-quoted phrase. I inferred that a phrase may not contain `"` or `\`, because in
Lucene's phrase syntax those characters end or escape it. That is an inference, not
documentation, and it was not checked against the live service.

## What changed for developers

New caller surface, re-exported at the crate root:

```rust,ignore
let q = Query::term(Field::Author, "del_maestro")?           // au:"del_maestro"
    .and_not(Query::term(Field::Title, "checkerboard")?
        .or(Query::term(Field::Title, "Pyrochlore")?));       // ANDNOT (ti:"…" OR ti:"…")
let pages = api.papers().query(q)
    .set_sort_by(SortBy::SubmittedDate)
    .set_sort_order(SortOrder::Descending)
    .set_size(PageSize::new(10)?)
    .pages(Resume::Start, NonZeroUsize::new(3).unwrap());
```

- `Field` is a closed enum of the nine documented prefixes. `Field::prefix()` returns the
  wire spelling.
- `Query::term(field, text) -> Result<Query, Error>` is the only fallible step. The text
  follows **the same rule as `SearchTerm`**: 1–512 UTF-8 bytes, not blank, and no `"`,
  `\` or control character. Otherwise it returns `Error::Input("invalid query term")`
  before any I/O. A term is always sent as a quoted phrase: `prefix:"text"`. Operator
  words, colons, parentheses and brackets inside the text are therefore phrase text.
  Single words are quoted too (`ti:"checkerboard"`). The manual's phrase syntax allows
  this, but whether arXiv ranks a quoted single word exactly like the bare word is
  **unverified** (NOT_RUN).
- `and`, `or` and `and_not` are infallible and `#[must_use]`. Any combined operand is
  parenthesized, so the grouping on the wire always matches the order in which the query
  was built (`(a OR b) AND c`, and `a ANDNOT (b OR c)`). The SDK never relies on the
  provider's operator precedence and never flattens or reassociates. A query is rendered
  eagerly into one string. Deep nesting therefore never recurses in `Display` or `Drop`.
- With these rules a caller cannot build a syntactically invalid `search_query`. Fields
  come from the enum, operators from the methods, phrases are always closed, and every
  group is balanced.
- `Query`'s `Debug` prints `Query(<redacted>)`, and the endpoint marks `search_query`
  `#[sensitive]`, as term search does.
- `papers().query(q)` is a new endpoint on the same route. It has `set_start`/`set_size`
  (the same types and defaults as search), optional `set_sort_by(SortBy)`/`set_sort_order(SortOrder)`
  with `maybe_*`, and `pages()`. The two sort keys are independent, as the manual
  describes them. When unset, **neither key is sent** and the provider's own order
  applies.
- Paging, pacing, the window, errors and redaction come from the same code as term search.
  Every `Feed` endpoint already goes through the shared `Policy`. The traversal is now one
  generic function, `offset_pages`. `Resume::Start` uses the request's own `start`. The
  traversal refuses the page after 30,000 locally, as
  `PagesError::Provider(Error::Input)` with `resume = At(30000)`, exactly as term search
  does. Tests assert all of this.
- **Term search and fetch are unchanged for callers.** The wire is the same: the
  consumer's exact query-string assertions and the protocol suite's exact targets pass
  unmodified. The same holds for setter names, types and defaults, and for errors.
  `Query::term(Field::All, t)` renders the same `search_query` as `SearchTerm::new(t)`.
  A test asserts this, and also that both types accept and refuse the same inputs. Term
  search gained no sort setters. A caller who wants sorted term search uses
  `query(Query::term(Field::All, t))`.
- There is one documentation addition to term search's `pages()`: `Resume::Start` starts
  at the request's own `start`, and `Resume::At(n)` replaces it. The consumer had
  reported this as undocumented, and the new endpoint shares the behaviour.
- New public trait `Paging`, generated by the parameter group. It is implemented by
  `ArxivPapersSearch` and `ArxivPapersQuery`. Callers need not import it, because the
  endpoints' inherent setters are unchanged. SDK-side generic code can use it.

## Files and locations edited

| File | Location | Change |
| --- | --- | --- |
| `src/query.rs` (new, 151 lines) | whole file | `Field`, `Query` (eager wire string, `compound` flag), `SortBy` and `SortOrder` with documented `Display` spellings. |
| `src/input.rs` | top; `SearchTerm::new`; `SearchTerm`'s `Display` | The phrase rule moved into `pub(crate) fn valid_phrase`, which both types use. `all` is now spelled through `Field::All.prefix()`. Behaviour is unchanged. |
| `src/lib.rs` | `mod`/`pub use` lines | `mod query;` and `pub use query::{Field, Query, SortBy, SortOrder};`. `use concord::Sends`. |
| `src/lib.rs` | `api!` client body | New `params Paging(query start: u32 = 0, query("max_results") size: PageSize = PageSize::default());`. |
| `src/lib.rs` | `search` declaration | The inline `start`/`size` values became `..Paging` (same wire order and setters). |
| `src/lib.rs` | `papers` scope | New `GET query(#[sensitive] query("search_query") expr: Query, ..Paging, query("sortBy") sort_by?: SortBy, query("sortOrder") sort_order?: SortOrder) -> Feed`. Scope doc updated. |
| `src/lib.rs` | `impl ArxivPapersSearch::pages` | The body moved to the generic `offset_pages<E: Paging + Sends<Result<Feed, Error>> + Clone + Send + 'static>`. The doc gained the `Resume` sentence. |
| `src/lib.rs` | new `impl ArxivPapersQuery::pages` | A one-line wrapper around `offset_pages`. |
| `README.md` | header line, usage example (a compiled doctest), term/query paragraph, citation, `Input` row, redaction sentence, "Excluded" list | Documents the query, sort, window and redaction facts. Removes the claims that Boolean syntax and sorting are out of scope. |
| `tests/query.rs` (new, 4 tests) | — | Regression and consumer tests (below). |
| `MAINTENANCE.md` (this file) | — | — |

Not edited: `src/policy.rs`, `src/atom.rs`, `src/model.rs`, `src/error.rs`, the fixtures,
`tests/first_use.rs`, `tests/protocol.rs`, `tests/workflows.rs`, `tests/common/mod.rs`,
`Cargo.toml`, and the root `Cargo.toml`/`Cargo.lock`. The author records `PROVENANCE.md`,
`VALIDATION.md` and `FRICTION.md` are left as written. Two of their facts are now
historical:
- `VALIDATION.md`'s line counts. The source is now 785 lines: `lib.rs` 145, `input.rs` 91,
  `query.rs` 151, and the rest unchanged.
- `FRICTION.md`'s pointer `lib.rs::pages`. That logic is now `lib.rs::offset_pages`.

**Facts that had to change in several places:**
- The phrase rule was SearchTerm's alone and is now shared. Without the extraction it
  would exist twice. The test compares the two types on the same inputs.
- The `start`/`max_results` window: its key spellings and defaults now live in one
  declaration, the group. They are still spelled again in `fetch`'s `query { }` block and
  read back by string in `policy.rs`.
- `path "query"` and `#[sensitive] query("search_query")` are now written on three
  endpoints and two endpoints respectively.
- The README's scope statements (header line, term-search paragraph, "Excluded") had to
  change by hand. Nothing checks them, so they were found by reading. The doctest only
  proves that the new example compiles.

## Did the compiler or the tests catch an omitted edit?

Mutations, each applied to a copy, run with
`cargo test --locked --offline -p concord-sdk-arxiv --tests --no-fail-fast`, then
restored (each file `cmp`-checked afterwards). The script and backups are in
`target/tmp/mut/`.

| Omitted or wrong edit | Caught by |
| --- | --- |
| M1: a combined operand is not parenthesized | tests: 3 query tests |
| M2: `ANDNOT` spelled `AND NOT` | tests: 3 query tests |
| M3: the phrase rule allows `"` | tests: 1 query test, the consumer's `search_refuses_bad_input_before_dispatch` and the protocol input test, because the rule is shared |
| M4: `sortBy` key misspelled | tests: 2 query tests |
| M5: `#[sensitive]` dropped from the query's `search_query` | tests: the `Prepared` `Debug` assertion. `Query`'s own `Debug` alone would hide it. |
| M6: `query` declares `start`/`size` inline instead of `..Paging` | **compiler**: E0277 `ArxivPapersQuery: Paging` not satisfied, at `offset_pages` |
| M7: `ArxivPapersQuery::pages` missing | **compiler**: E0599 in the tests (it would not be caught without a caller) |
| M8: `descending` misspelled | tests: 3 query tests |
| M9: the `jr` prefix misspelled | tests: the prefix-table test only |
| M10: the window-end check removed from the policy | tests: the consumer, the protocol suite and the query window test |
| M11: the traversal ignores the resume offset | tests: 10 tests in three suites (first_use 5, query 2, workflows 3) |
| M12: `max_results` key renamed inside the group | **tests only**: 28 failures with `Error::Input("missing page parameter")` at run time. The compiler is silent, because `policy.rs` reads the key by string. |

Forgetting the `pub use` line is an unresolved-import compile error in every test file
(not run as a mutation; standard rustc behaviour). Nothing catches stale README prose.

## Validation (observed, this worktree, Linux, one cargo command at a time)

`CARGO_TARGET_DIR=<worktree>/target CARGO_BUILD_JOBS=4`:

| Command | Result |
| --- | --- |
| `cargo test --locked --offline -p concord-sdk-arxiv` (baseline, before edits) | 32 pass: first_use 10, protocol 16, workflows 5, doctest 1 |
| same, after the change | **36 pass**: first_use 10, protocol 16, **query 4**, workflows 5, doctest 1 (the extended README example) |
| `cargo clippy --locked --offline -p concord-sdk-arxiv --all-targets -- -D warnings` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-arxiv --no-deps` | pass |
| `cargo fmt -p concord-sdk-arxiv --check` | pass |
| `git diff --check`; `cmp Cargo.lock <pre-change copy>` | pass; identical |

New tests in `tests/query.rs`:
- **Regression**, `query_syntax_uses_documented_prefixes_operators_quotes_and_explicit_grouping`:
  every §5.1 prefix as a literal, the manual's ANDNOT-with-grouping example, left and
  right grouping, no flattening, operator words and parentheses kept inside a phrase,
  refusals (blank, quote, backslash, tab, newline, more than 512 bytes) and acceptance at
  512 bytes, `Query`/`SearchTerm` agreement, the §3.1.1.3 spellings, and redacted `Debug`.
- **Regression**, `query_wire_matches_the_manual_and_omits_unset_sort_keys`: exact request
  targets (`%22`, `%28`/`%29`, `+`) for an unsorted query, the manual's sort sample, an
  order-only query with a custom window, a by-only query through generic `Paging` code,
  and term search through the same generic code (unchanged wire). Four 3-second waits
  across five dispatches on one policy.
- **Consumer**, `sorted_query_pages_inspects_entries_and_resumes_like_term_search`: a
  sorted category/title query traverses the two existing fixtures, which contain unknown
  extension elements and missing optional fields. Every page repeats the query and sort.
  It also checks the bounded budget with resume `At(1)`, `Resume::Start` honouring
  `set_start`, and request and `Prepared` `Debug` withholding the query.
- **Consumer**, `query_window_and_provider_failures_match_term_search`, on the ordinary
  `Arxiv::with_http(http_client())` path: windows ending past 30,000 are refused with no
  request; the traversal at 29,990 is refused locally at 30,000; a 200 error entry gives
  `Error::Api` and a 400 gives `Error::Http`; there is no retry and no query text in the
  error output. The pages are synthetic and generated in the test (each marked `SYNTHETIC`).

## Remaining generic duplication

1. **Two identical `pages()` wrappers.** A parameter group's trait carries only setters.
   A method written once for every including endpoint therefore has to be either an
   inherent wrapper per endpoint (chosen here) or a trait that callers must import. The
   second would break the consumer's import list
   (`use concord_sdk_arxiv::{Arxiv, Error, Feed, IdList, PageSize, Paper, SearchTerm, http_client}`).
2. **The window's key spellings live in three places.** They are in the group
   declaration, in `fetch`'s literal block and in the string lookups in `policy.rs`. M12
   shows that a rename compiles and fails only at run time. The 100-entry cap appears in
   both `PageSize::new` and `policy.rs`, and 30,000 appears only in `policy.rs`. All of
   this predates the change, but a second endpoint now depends on it.
3. **`path "query"` appears on all three endpoints.** It could move to
   `scope papers { path "query"; }`. I left it alone to keep the diff to the ticket.
4. **`#[sensitive] query("search_query")` appears on two endpoints**, with different
   required types (`SearchTerm` and `Query`). A group cannot hold required values, so
   this repetition is expected.

## Provider-specific responsibilities (stay in this SDK)

- The prefix table. Keeping `Id` despite the manual's advice to use `fetch`.
- Operator spellings, and the decision to parenthesize every combined operand instead of
  trusting the provider's precedence.
- The phrase rule: what may appear inside quotes, always quoting, and the 512-byte cap
  (a local choice, inherited from `SearchTerm`).
- Sort spellings, independent `sortBy`/`sortOrder`, and omitting both when unset.
- Paging, the 30,000 window, 3-second pacing and Atom error interpretation. These are
  unchanged and shared by all `Feed` endpoints through `Policy`.
- Exclusions, with reasons:
  - `submittedDate:[… TO …]`: it needs a typed GMT minute range; it is not in the ticket.
  - `search_query` combined with `id_list` (§3.1.1.1 filter mode): it would change `fetch`.
  - A total length cap on the query: the combinators are infallible, so an over-long GET
    reaches the provider and fails there as `Error::Http`. §3.1 suggests POST for long
    parameter lists, but the SDK only sends GET.
  - Untyped raw `search_query` strings.

## Candidate Concord improvements (recorded, not made)

1. **Group-level SDK methods.** Let a parameter group name an SDK-written generic method
   or trait that Concord forwards inherently on every including endpoint, as the group
   setters already are. This would remove wrapper 1 above and keep callers import-free.
   Evidence: two identical wrappers. The import-requiring alternative conflicts with an
   existing consumer.
2. **Read access to a group's values** (getters on the group trait), or typed access to
   declared values in `SendWith`. This would let `policy.rs` and `offset_pages` read
   `start`/`size` as typed values instead of re-parsing the URL by key. Evidence: M12
   compiles silently and fails 28 tests at run time. This complements the author's
   FRICTION row 2.
3. **Group-trait rustdoc.** It says "Replaces `start`." without the "Sent as: query
   parameter `start`" line that the endpoint setters carry. It also describes itself in
   macro terms ("parameter group", "`..Paging`"), the noise the consumer reported for
   endpoint pages. This is a small change in `concord-macros/src/emit.rs::group_traits`.

Things that worked without change:
- `concord::Sends<O>` did exactly what its rustdoc promises. The generic traversal sends
  through the client's policy without being handed it, and the future met `Pages::new`'s
  `Send + 'static` bounds with no adapter.
- An endpoint named `query`, although `query` is also a placement keyword, compiled and
  documented correctly.
- Two independent optional keyed values were simpler than a `QueryPairs` sort type. That
  type would have forbidden `sortOrder` without `sortBy`, which the manual does not forbid.

## Prior exposure (what I read before starting, in order)

1. My role brief.
2. `git status`, `git log --oneline -3` and `git show --stat HEAD~1 HEAD`. **The consumer
   commit's message summarised first-use findings:** the undocumented user agent,
   `Resume::Start` using `set_start`, the 30,000 traversal error looking like a provider
   failure, bare `Error`s in rustdoc, and `Policy::with_clock` needing concord-ratelimit.
   I therefore saw these before attempting the ticket, though not `FRICTION.md` or
   `FIRST_USE.md`.
3. `Cargo.toml` and all of `src/*.rs`.
4. `README.md`, `PROVENANCE.md` and `VALIDATION.md`.
5. `tests/common/mod.rs`, `tests/workflows.rs` and `tests/protocol.rs` in full.
6. `tests/first_use.rs`: an API-usage grep plus lines 1–30, 160–260 and 540–650 (fixtures,
   helpers, the exact search wire, `Resume::Start`, the 30,000 window and input refusals).
   I read these to know which assertions must survive.
7. Concord docs: `docs/CHEATSHEET.md` in full, and from `docs/AUTHOR_GUIDE.md` §2, §2.1,
   §4.1a, §5.12–5.13 and §9–10.
8. Shared source, read only:
   - `crates/concord-macros/src/emit.rs`: `group_traits`, and the generated `send`/`Sends`.
   - `crates/concord-core/src/prepared.rs`: the `Sends` documentation.
   - The `Pages::new` bounds and `next_offset`.
   - The `IntoValue` implementations.
9. The arXiv user manual, as cited above.

`FRICTION.md` and `FIRST_USE.md` were read only **after** the code and tests passed. They
added two things: the documentation sentence about `Resume::Start`, and the cross-check
that the new endpoint keeps the 30,000 traversal behaviour (an existing assertion pins it,
so it was mirrored, not changed). No other worktree, campaign file or `*-sdk` example was
read.
