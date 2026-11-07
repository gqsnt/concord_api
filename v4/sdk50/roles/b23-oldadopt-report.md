Implemented and left uncommitted. Only 13 authorized SDK packages changed; core and the five wave-2 SDKs are untouched. All existing assertions and fenced doctests remain unchanged.

Counts below are **removed/added lines**, including documentation and tests. Before→after counts cover touched files.

| SDK | Item 1: result | Item 2: fields | Item 3: delivery | Lines before→after |
|---|---:|---:|---:|---:|
| discord-rest | 3/3 | 6/1 | 4/1 | 968→960 |
| nasa-neows | 3/3 | 9/3 | 0/0 | 294→288 |
| fred | 3/3 | 0/0 | 0/0 | 359→359 |
| open-library | 2/2 | 0/0 | 0/0 | 207→207 |
| usgs-earthquake-catalog | 3/3 | 0/0 | 0/0 | 376→376 |
| dropbox | 2/4 | 16/7 | 0/0 | 1147→1140 |
| openai | 3/3 | 0/0 | 0/0 | 579→579 |
| pokeapi | 3/3 | 0/0 | 0/0 | 369→369 |
| gitlab | 3/3 | 3/2 | 0/0 | 657→656 |
| ntfy | 3/3 | 7/31 | 0/0 | 1271→1295 |
| github | 0/0 | 13/11 | 0/0 | 822→820 |
| distribution-registry | 0/0 | 51/27 | 0/0 | 1643→1619 |
| amazon-s3 | 0/0 | 5/3 | 0/0 | 416→414 |
| **Total** | **28/30** | **110/85** | **4/1** | **9108→9082** |

The patch removes **26 lines overall**, including 27 added lines of test coverage.

All ten generic policies now name their actual output shape. Discord, NASA, FRED and Open Library return `Result<_, Error>`; USGS, Dropbox, OpenAI and PokéAPI use `Result<Response<_>, Error>`; GitLab and ntfy retain their respective error types. READMEs and rustdoc were updated.

Skips and compatibility decisions:

- **World Bank, Frankfurter, Nager Holidays, Airtable and S3:** item 1 skipped because their policies implement specific output types. The first four remain entirely unchanged: 0 removed/added.
- **S3 item 3:** retained its one-line SDK-variant classifier; delegating to `delivery()` would add code and change classification.
- **Discord:** retained its fractional-delay parser and shared digit parser. Existing assertions require whitespace rejection and fractional rounding. Its repeated-field helper and transport delivery check were simplified without changing those outcomes.
- **S3 numeric parsing:** retained its bounded numeric conversions and existing error mappings; only repeated-field checking shrank.
- Other zero entries had no applicable simplification. GitHub and registry were field-adoption targets, not item-1 policy candidates.

No adopted change conflicts with an existing asserted outcome. Deliberate parsing changes are: Dropbox accepts surrounding whitespace; ntfy rejects signed or repeated numeric `Retry-After`; GitHub download and registry lengths use strict unsigned parsing.

NASA’s generated `NeoWsNeowsLookup::send` rustdoc was checked before and after.

Before:
```rust
pub fn send(
    self,
) -> impl Future<Output = <Bounded as SendWith<Asteroid>>::Output> + Send
```

After:
```rust
pub fn send(self) -> impl Future<Output = Result<Asteroid, Error>> + Send
```

Verification passed with `CARGO_BUILD_JOBS=4`, offline Cargo, and one Cargo process at a time:

- `cargo test -p <pkg>` separately for all 13 changed packages: **568 tests/doctests passed**, zero failed or ignored.
- `cargo clippy -p <pkg> --all-targets -- -D warnings`: all 13 passed.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p <pkg> --no-deps`: all 13 passed.
- Baseline `cargo doc -p concord-sdk-nasa-neows --no-deps`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

Package names are `concord-sdk-<SDK>`, except GitHub’s `concord-example-github-sdk`. Exact command results are retained in [verification.json](/tmp/b23-oldadopt-evidence/verification.json). Generated rustdoc remains available; the completed build cache was removed.