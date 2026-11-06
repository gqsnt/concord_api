# Maintenance change: null indicator and multiple pages

Maintainer run: 2026-11-06. Requested runner: a Claude Opus subagent. The session environment
names model ID `claude-opus-5-5`; reasoning effort is not exposed to the agent, so no effort
is claimed. Starting HEAD: `3bb3d67c` (author and consumer commits). Concord library frozen at
`47a25fbf`. I did not write this SDK. I did not read `FRICTION.md` or `FIRST_USE.md` until the
first attempt was finished and passing. Reading them afterwards changed nothing.

Ticket: "Null indicator and multiple pages". The perturbation asks for an absent value and a
multi-page metadata response, and for null to stay distinct from zero.

## How I read the ticket

- **Absent value** means an observation whose `value` is JSON `null`. I did **not** read it
  as a missing `value` key. The README says a missing key is rejected, and two existing tests
  depend on that (see the M4 mutation below). The provider pages I checked do not say how a
  missing observation looks in JSON: Basic Call Structures and the V2 enhancements page are
  silent, and the V2 page only says "the fact data type is now a number". So nothing in the
  provider contract I could find requires accepting a missing key. JSON `null` as the absent
  marker is the author's modelling assumption, carried over without change. It is not a
  documented fact.
- **Multi-page metadata response** means the first array element announces `pages > 1`. I
  did not read it as the World Bank metadata catalog, which the README puts out of scope. The
  only response sample in the docs is single-page (`page="1" pages="1" per_page="50"
  total="30"`), so the multi-page bytes are synthetic.

## What I found

The ticket's behaviour was already in place before this change:

- `Observation.value: Option<f64>` plus `#[serde(deserialize_with = "required_nullable")]`
  (`src/lib.rs:35-37`, `:45`). `null` becomes `None`, `0` becomes `Some(0.0)`, and a missing
  key is a decode error.
- The data array is `Option<Vec<Observation>>` in `WireResponse`, and `null` becomes an empty
  vector (`src/lib.rs:101`, `:111`, `:258`).
- The range continuation is `page < pages` → `page + 1`, taken from each page's own metadata
  (`src/lib.rs:347`). Paging numbers can be strings or numbers (`number_or_string`).
- Existing tests already prove null versus zero across a two-page traversal
  (`workflows.rs::time_range_reaches_last_page_and_preserves_null`, several `first_use.rs`
  tests).

The gap was in the evidence, not in the behaviour. Every existing range fixture has exactly
two pages, so the continuation rule had only been tested on one transition (1→2). Mutation M3
below makes the traversal stop cleanly after page 2, and every pre-existing test still passes.
The existing fixtures also never contain an all-null page in which every value is a real
`null` observation. (The author's `page-1.json` has a single null, and the consumer's empty
intermediate page has a `null` data array.)

## Edits by kind

| Kind | Edit |
| --- | --- |
| Declaration (`concord::api!`) | none |
| Adapter (`SendPolicy`, `year`, `range`) | none |
| Model (`Observation`, `PageMetadata`, `WireResponse`) | none |
| Tests | new `tests/maintenance.rs` with 4 wire tests; new fixtures `fixtures/maintenance/sparse-{1,2,3}.json` |
| Docs | `README.md`: one sentence in the range section saying that an all-null or null-data page is not an end, and that `None`/`Some(0.0)` holds on every page. `PROVENANCE.md`: one table row. This file. |
| Existing tests / callers | unchanged; all pass; no caller change |

The fixtures describe three pages (per_page 2, total 5) for a synthetic indicator,
`XX.SYNTH.SPARSE`. The values are null, null | 0, null | null. Page 1 is all null, the zero
sits next to a null on page 2, the last page is partial and its only value is null, and
`per_page` is a string on page 1 only. Each test asserts the exact request target for every
page (`date=2001%3A2005&page=N&per_page=2&format=json`), one request per indicated page, GET,
no auth and an empty body. The tests are:

1. `sparse_pages_follow_metadata_and_keep_null_distinct_from_zero`: `next_page` page by page,
   checking metadata, `next` and the checkpoint; ends with `Exhausted` after two `Ok(None)`.
2. `sparse_items_are_complete_in_provider_order`: `items().collect_complete()` returns all
   five values (four `None`, one `Some(0.0)`).
3. `sparse_cap_before_last_page_is_bounded_and_resume_keeps_trailing_null`: `max_pages(2)`
   gives `Bounded { resume: At(3) }` after two requests, and resuming yields the trailing
   `None`.
4. `sparse_provider_error_on_last_page_keeps_prefix_and_is_typed`: a provider error envelope
   at HTTP 200 on page 3 yields `Failed`, `failed: At(3)` and a typed `Error::Provider`, and
   the four-value prefix keeps its nulls.

## Mutation evidence

These were temporary edits to `src/lib.rs`, each reverted with `git checkout` (the final diff
does not touch `src/`). The new-suite column for M1 to M3 comes from `--test maintenance` runs.
The pre-existing columns for M1, M2 and M4 used `--no-fail-fast`. The M3 run of the
pre-existing suites did not need that flag, because nothing failed and every binary ran.

| Mutation | Pre-existing suites | New `maintenance.rs` |
| --- | --- | --- |
| M1 `null` decoded as `Some(0.0)` | fail: 5 first_use, 1 workflows | 4/4 fail |
| M2 a page with no non-null value ends the traversal | fail: 1 first_use, 3 workflows | 4/4 fail |
| M3 traversal silently ends after page 2 (`pages.min(2)`) | **all 34 pass** | 4/4 fail |
| M4 missing `value` key accepted as `None` (`serde(default)`) | fail: `first_use::year_rejects_missing_value_and_malformed_success_json`, `protocol::failures_preserve_head_and_reject_bad_shapes` | 4/4 pass (this suite does not exercise a missing key) |

## Places that encode the same fact

**"An absent value is `null`, distinct from zero; the key is required."** This fact appears
in seven places:

1. the field type `Option<f64>` (`src/lib.rs:37`);
2. the `deserialize_with` attribute and the three-line `required_nullable` function
   (`src/lib.rs:36`, `:45-47`). The "required" half is encoded only by the attribute's
   presence: without it, serde would treat a missing `Option` field as `None`;
3. the struct rustdoc (`src/lib.rs:23`) and the field rustdoc (`src/lib.rs:35`);
4. the README "One year" paragraph (`README.md:23`);
5. the README range sentence I added (`README.md:47`), which is one more copy in prose;
6. assertions in three independently written test files (`workflows.rs`, `first_use.rs`, and
   the new `maintenance.rs`), plus the missing-key rejection in `protocol.rs` and
   `first_use.rs`;
7. the fixture descriptions in `PROVENANCE.md` and `fixtures/first-use/PROVENANCE.md`.

**"Continue while `page < pages`; an empty page is not an end."** This fact appears in five
places:

1. the `next` expression (`src/lib.rs:347`);
2. the page-consistency checks in `SendPolicy` (`src/lib.rs:247-266`);
3. the `year` rustdoc (`src/lib.rs:288`) and README line 23 ("use the range traversal");
4. the README range paragraph (`README.md:47`);
5. the tests in all four test files.

None of these places had to change. In code, each fact lives in one location: a field plus
its attribute, and one expression. The repetition is in prose and in independent tests, and
that repetition is appropriate. Inferred from M4, not performed: if the ticket had meant
"accept a missing key", it would have touched one attribute, two rustdoc lines and one README
sentence, and it would have overturned two tests from two different authors. That is a
caller-visible contract change, which needs a provider reason I did not find.

## What I had to read

- **SDK:** `src/lib.rs` (all of it), `README.md`, `PROVENANCE.md`, `Cargo.toml`,
  `.gitattributes`, all three existing test files, and the author and consumer fixtures plus
  `fixtures/first-use/PROVENANCE.md`.
- **Harness and library:** the public items of `examples/testserver/src/lib.rs` (`Recorded`,
  `Reply`, `Script`), and a grep of the public signatures in `crates/concord-pagination`.
  Neither the Concord docs nor the rustdoc were needed beyond that.
- **Provider docs:** Basic Call Structures and V2 enhancements, fetched as documentation pages
  (not API calls). Both are silent on null representation and on multi-page responses.
- **After the first attempt:** `FRICTION.md` and `FIRST_USE.md`. Neither changed the result.
  The consumer had predicted first-page-only `year` and null versus zero.
- No other provider SDK was consulted.

## Generated-name or compile surprises

None. The new test file compiled and passed on the first run. Formatting needed only a rustfmt
reflow, applied to the new file alone. No generated operation names or setters were needed
beyond the SDK's own `year` and `range`. There was no private reach-through, no native escape
and no library edit, and there is **no new Concord friction**.

## Did it feel local?

Yes. The change was local to verification: one new test file, three fixtures, one README
sentence and one provenance row, with zero code edits. Finding the relevant facts meant
reading one 364-line source file. The semantic change did not touch several independently
maintained code descriptions, because none needed to change. What it did touch was prose:
the null/zero fact now appears in two README sentences and two rustdoc lines, and a future
change to that fact would need all four kept in step.

## Observations recorded, not acted on

- **`year()` returns only page 1 when metadata announces more pages.** This is documented,
  and the consumer pins it in `year_returns_only_first_page_when_provider_announces_more`. It
  cannot happen inside the slice's one-country, one-indicator, annual contract at the default
  `per_page=50`. Changing it would be a caller-visible change that the provider contract does
  not require.
- **No within-response `pages == ceil(total / per_page)` check.** If one response announced
  too few pages for its own total, the traversal would end cleanly and short. The only
  documented sample is consistent with such a check. But there is no documented multi-page
  sample, and the consumer's null-data case (`total` 0, `pages` 1) would need an exception.
  I did not add the check, because it is speculative validation and the smallest change
  leaves it out. This one is for the owner to decide.

## Command and result

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-world-bank
```

| Suite | Result |
| --- | --- |
| unit | 0 tests |
| `first_use` | 25 passed |
| `maintenance` | 4 passed |
| `protocol` | 5 passed |
| `workflows` | 4 passed |
| doctests | 2 passed |
| `cargo clippy --locked -p concord-sdk-world-bank --all-targets` | clean |
| `cargo fmt -p concord-sdk-world-bank --check` | clean |

No live provider calls, credentials or secrets were used.
