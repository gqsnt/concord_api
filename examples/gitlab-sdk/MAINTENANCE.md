# Page size and Link query maintenance

The ticket specified a change in representation but no new numeric limit or specific query. The
offline scenario therefore chooses an initial `per_page=3` and a returned link with `per_page=1`,
an encoded synthetic cursor, repeated keys, an empty value and a `+` spelling. These values do
not assert a new GitLab default, limit or cursor format. The link points to the same collection by
project ID and conflicts with `x-next-page`. Its target is authoritative.

## Edits by kind

- **Declaration:** none. `src/api.rs` already declares optional `per_page` and generates
  `set_per_page`; no next-page query is declared or constructed.
- **Adapter:** no executable change. `src/pages.rs` already retains the checked link URL in
  `NextPage`, replaces the prepared request URL, and executes using the declared decoder. Its
  module documentation now explicitly describes preservation of the query through continuation.
- **Model:** none. `PerPage` already accepts all sizes in the documented 1–100 range, and the
  opaque continuation is already a URL. No range, default or response model was changed.
- **Tests:** `changed_link_query_survives_cap_and_failed_retry` in `tests/workflows.rs` checks the
  initial raw request target, the retained URL at a one-page cap, the typed API error and retry
  position after a 500, and the exact linked request target on both attempts. It also checks
  authentication and Accept headers and successful completion after retry. Existing hand-authored
  body fixtures are reused.
- **Docs and evidence:** README and `src/pages.rs` describe the tested behavior; `PROVENANCE.md`
  identifies the synthetic values; `FRICTION.md` adds a reproduction for the existing native
  URL-replacement workaround. No fixture bytes or `.gitattributes` changed.

## Maintenance observations

**Zero independently maintained production descriptions of a provider fact had to change** for
this perturbation: page size is a caller value and the next query is server data. The single new
scenario defines its linked target once and uses it in the response and wire assertions; the
initial size appears in both the caller setup and its expected request. The README and module docs
independently describe the existing authority rule and were clarified, not changed to a new
provider rule. The executable adapter, declaration and models required no synchronization. The
scenario felt local, although understanding why needed reading the adapter as well as the public
workflow.

Read the SDK README, declaration and `params`, `pages`, `link`, `call`, workflow and protocol
tests, test support, provenance and first-use test code; also read Concord's README,
`docs/RECIPES.md` and `crates/concord-pagination/README.md`. Generated facade rustdoc with
`cargo doc --locked -p concord --no-deps --features csv,jsonl`. The existing GitHub SDK supplied
the original adapter pattern according to `FRICTION.md`; it was not read during this maintenance
attempt. `FRICTION.md` and `FIRST_USE.md` were read only after the first implementation and full
test run passed.

The generated `GitLabProjectIssues` type and `set_per_page` spelling matched the existing code;
there was no compiler or generated-name surprise. The existing public Concord workaround remains:
each linked request prepares the listing, takes its native request apart, replaces its URL and
uses `Decoder::execute`. `FRICTION.md` entry 4 describes it and now includes the new test as a
reproduction. No library code was changed.

The first full offline run passed: 18 first-use, 23 protocol, 11 workflow and 9 doc tests.
No live GitLab calls or real credentials were used.

## Between waves 2-3 (coordinator integration)

The URL replacement described above is now `Prepared::follow` followed by `receive()`. The
maintenance test is unmodified and passes. See [INTEGRATION.md](INTEGRATION.md).
