# SDK50 REST Countries maintenance

Date: **2026-11-07**. Live provider calls: **NOT_RUN**. All SDK requests used local fixtures
via `TestServer::start(|request: &Recorded| -> Reply)`. No commits were attempted.

## Change and caller contract

Added `Region`, `Subregion` and `Language`, each with synchronous `parse`, `as_str` and
`Display`, plus `InputError` variants. Their contents are private, so callers must validate
before calling `api.region(&region)`, `api.subregion(&subregion)` or `api.language(&language)`.

The helpers select `Property::REGION`, `SUBREGION` or `LANGUAGES` and delegate to the existing
`property(...).read(...)` endpoint. The hierarchy remains client → property scope → read
endpoint. They return the existing generated `RestCountriesPropertyRead`, retaining
`ListOptions`, optional `q`, one-request `send()`, and the existing explicit `pages()` walker.
No second transport, pagination adapter, response model or auth policy was introduced.
Name search, code lookup and filtered listing declarations and implementations are unchanged.

Validation accepts non-empty, unpadded plain text with internal ASCII spaces and Unicode.
It rejects controls, other whitespace, dot-only path segments and URL syntax (`/`, `\`, `?`,
`#`, `%`, `+`). It preserves spelling/case and requires spaces rather than pre-encoded text.
This is an SDK policy, not a claim that the provider publishes this grammar. Values are not
checked against a closed country/language dataset. A well-formed unknown value is permitted
and an empty fixture page is ordinary success; it never starts pagination or auth work.
Language matching/normalization remains the provider's responsibility. Language response
data continues to be available through `Country::other`; response models were not expanded.

## Prior exposure and reading order

This maintenance turn began with the ticket and environment instructions, a clean
`git status --short`, worktree filename discovery and no previous implementation knowledge.
Before the first edit I read:

- Root and SDK `Cargo.toml`.
- The SDK's `src/lib.rs`, `src/input.rs`, `src/model.rs` and `src/error.rs`.
- SDK `README.md` and `PROVENANCE.md`.
- `tests/first_use.rs`, `tests/workflows.rs`, `tests/protocol.rs` and `tests/support/mod.rs`.
- The two official pages listed below, using read-only web access.

Some batched file/tool output was truncated; relevant README, model and protocol sections
were subsequently read in smaller ranges. File discovery did not find an applicable
`AGENTS.md`. No other SDK implementation or campaign/parent-directory material was opened.
The existing provenance document describes its original author's earlier reading; that is
not this maintainer's prior exposure.

I implemented both source changes, added all three new tests, and ran the full package suite
successfully **before opening `FRICTION.md` or `FIRST_USE.md`**. Only then read those two SDK
documents to compare the observed maintenance costs with the previous author/consumer notes.
Neither document was edited. Root `Cargo.lock` and the frozen manifest/lock were subsequently
read structurally for the scope check; generated rustdoc was built, not used as prior input.
No source files outside the worktree were read.

## Provider evidence and access conditions

Exact pages read **2026-11-07**:

| Page | Evidence used |
|---|---|
| [Countries API](https://restcountries.com/docs/countries) | Read by property: exact case-insensitive reads, region/subregion examples and array reverse lookup. Field reference: readable `languages`. List/Read/Pagination: field and page options. |
| [API versions](https://restcountries.com/docs/countries/api-versions) | Maintained v5, API hostname and bearer-key authentication; no inferred v3 contract. |

Rechecked operational access conditions on these pages: API key required; free page ceiling
100, paid 500; premium fields need a paid plan; account/rate limits produce provider errors.
Separate legal terms, acceptable-use and pricing pages are outside the permitted read boundary
and were not reviewed. No live access was attempted, even with the demo key.

The exact no-match envelope is not shown in the docs; this change retains the established
SDK/ticket empty-page contract and records that limitation in `PROVENANCE.md`. The language
fixture's `English` value is an authored sample, not live evidence of accepted aliases.

## Files and locations edited

All locations below are relative to `examples/rest-countries-sdk/`.

| File | Edited location and purpose |
|---|---|
| `src/input.rs` | Module description; `InputError` variants/Display; new `valid_lookup_value`, `lookup_values!` and three typed inputs. Existing input behavior is unchanged. |
| `src/lib.rs` | Public input re-exports; three helpers in `impl RestCountries`, after `country`. The `concord::api!` block, send policy and walker are unchanged. |
| `tests/lookups.rs` (new) | Local validation/construction test and all-three-lookups no-match regression, checking exact paths, request counts and bounded completion. |
| `tests/lookup_consumer.rs` (new) | Public-API consumer: each lookup, allowlist/omit, page size, explicit offsets, complete walks, capped partial results and absent/withheld fields. Inline synthetic fixture pages. |
| `README.md` | Typed lookup example (compiled doctest), API table rows, input contract and test inventory. |
| `PROVENANCE.md` | Dated maintenance re-read, endpoint grounding, SDK validation decisions, fixture origins and unchanged dependencies. |
| `MAINTENANCE.md` (new) | This observable change, diagnostic and scope record. |

No fixture files or existing tests were edited; every assertion in `tests/first_use.rs` remains.
The root workspace member and lock entry were already present at the start, so neither root
file needed an edit. No new dependency was added.

## Facts represented in several places

- Each lookup's type/method/property mapping appears in input docs, the helper and public
  re-export, README and the independent test's expected path. Existing `Property` constants
  were reused. The non-obvious singular `language` → plural `languages` route deserves the
  explicit wire assertion.
- The plain-text rule is implemented once in `valid_lookup_value`; `lookup_values!` supplies
  all constructors/formatting. Its caller explanation appears in rustdoc, README and tests,
  with provenance distinguishing SDK policy from provider restrictions.
- Each public type also needs an `InputError` variant and static Display message. Those
  declarations and the re-export are manual; the constructor macro does not maintain them.
- Paging/selection facts are explained again in the README and exercised in the new consumer,
  but their implementations remain solely in `ListOptions`, `CountryPage` and `pages()`.

## Diagnostics and validation

The first build intentionally omitted `--locked` and succeeded offline without changing
the lock. All subsequent cargo invocations used `--offline --locked`, `CARGO_BUILD_JOBS=4`,
one cargo process at a time and the default worktree `target/`:

```sh
CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-rest-countries --no-run
CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-rest-countries
CARGO_BUILD_JOBS=4 cargo clippy --offline --locked -p concord-sdk-rest-countries --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-rest-countries --no-deps
```

The initial implementation compiled and passed on its first test run; no accidental omitted
edit was caught. A deliberate mutation then replaced the language helper's `LANGUAGES`
mapping with `REGION`. It compiled, but the focused regression failed at the wire assertion:
actual `/countries/v5/region/Unmatched%20language`, expected
`/countries/v5/languages/Unmatched%20language`. This shows that a wrong mapping is a test
failure, not a type error. The source was restored in a `finally` block, then the full suite
passed again, including the added README example.

Final results: **27 integration tests and 4 doctests passed**, zero failures (no unit tests
declared). This includes the original two first-use consumer tests unchanged and three new
integration tests. Clippy and rustdoc passed with warnings denied. Rustfmt checks of the
changed Rust files and `git diff --check` passed.

A structural manifest/lock comparison with frozen baseline
`c3fd4f1ee30c1727e26b285270f2cc8d8b694e77` verified that its only workspace member addition is
this SDK and its only lock package addition is `concord-sdk-rest-countries` 0.0.0. Every
existing lock package is identical, not merely at the same version. A separate HEAD diff
confirmed no maintenance changes to either root file or `tests/first_use.rs`, and no tracked
changes outside the owned SDK.

## Remaining duplication and Concord candidates

The three helpers repeat the same property/read forwarding pattern as `country`; the local
macro removes repeated newtype implementations but leaves error declarations, exports and
documentation maintenance. No generic request, field-option, response-decoding or pagination
logic needed copying. Existing status/document error plumbing in `src/error.rs` remains
generic boilerplate, consistent with the later-read FRICTION notes.

Candidate shared improvements, recorded only (no shared crate edits):

1. Evaluate a declaration-level alias for a typed lookup through a fixed scope argument,
   retaining the generated endpoint/options/send policy. It could reduce helper/export/docs
   drift; this small slice did not require it to make the change.
2. Add a reusable redacting provider-error/document container, keeping provider body and status
   classification in the SDK (the existing FRICTION proposal).
3. Put a cap/error recovery example near the public pagination entry point, including retained
   items and resume offset, as suggested by the earlier first consumer.

The provider-specific pieces should stay here: choosing the v5 property paths, value/alias
semantics, projected optional fields and premium notices, the inferred no-match contract and
the empty out-of-range-page rule. The two documentation pages do not establish full legal
access terms or live language matching. Local fixture success makes neither claim.
