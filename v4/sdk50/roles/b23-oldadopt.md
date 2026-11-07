# Adoption lane (between waves 2 and 3): new Concord facts in earlier SDKs

You work in a git worktree of the Concord Rust workspace (branch `sdk50/b23-oldadopt`). Read
`AGENTS.md`/`CLAUDE.md` if present, `docs/AUTHOR_GUIDE.md` §5.11 ("One header field, strictly"),
§5.11b (`Error::delivery()`) and §5.13 ("Name the result": `send P -> Result<_, Error>;`). You
cannot commit; leave changes in the working tree. Network is for loopback tests only. Edit only
`examples/*-sdk` packages listed below (not `crates/`, not the five wave-2 SDKs postmark, mailgun,
zendesk-support, intercom, hubspot-crm).

## Adopt, only where it removes or simplifies code (measure lines before/after)

1. `send Policy -> Result<_, Error>;` (or the SDK's real uniform output shape, e.g.
   `Result<Response<_>, Error>`) in every SDK whose policy is implemented generically with one
   output shape: candidates `discord-rest`, `nasa-neows`, `fred`, `open-library`,
   `usgs-earthquake-catalog`, `dropbox`, `openai`, `pokeapi`, `gitlab`, `ntfy` (check each
   `type Output`). Skip a policy implemented for specific types (world-bank, frankfurter,
   nager-holidays, airtable, amazon-s3) and say so.
2. `concord::field::{single, unsigned}` where an SDK builds a `ResponseHead` with an invented
   status/version only to read fields (`github-sdk/src/meta.rs`, `gitlab-sdk/src/meta.rs`,
   `nasa-neows-sdk/src/lib.rs`, `distribution-registry-sdk/src/upload.rs`), or parses a numeric
   header by hand.
3. `Error::delivery()` in `discord-rest-sdk` (replacing `transport().is_connect()` in the
   ambiguous-write classification) and any other SDK that hand-classifies "not sent". **Behavior
   rule:** if an existing test asserts an outcome that would change, do not edit the assertion;
   keep the old behavior there and report the conflict.

Rules: every existing test assertion stays unchanged (compile-fail doctests included); README and
rustdoc updated where public signatures change. For item 1 check the generated rustdoc of one SDK
before and after (`cargo doc -p <pkg> --no-deps`, the `send` method of one endpoint) and quote both
signatures in your report.

Verification (`CARGO_BUILD_JOBS=4`, one cargo job at a time): `cargo test -p <each changed
package>`, clippy `--all-targets -D warnings`, rustdoc `-D warnings`, `cargo fmt --all -- --check`.

## Report

Per SDK and per item: lines removed/added, skipped with reason, any behavior conflict; the two
rustdoc signatures; commands and results.
