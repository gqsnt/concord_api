# P1 capsule: one strict `Link` header parser in `concord-pagination`, adopted by four SDKs

You implement a bounded change in the Concord Rust workspace (your working directory, a git
worktree). Read `AGENTS.md`/`CLAUDE.md` if present and `crates/concord-pagination/src/lib.rs`.
You cannot commit (`.git` is read-only); leave the finished change in the working tree.

## Evidence

Four SDKs carry their own RFC 8288 `Link` tokenizers: `examples/github-sdk/src/link.rs`
(`parse`, `parse_str`), `examples/gitlab-sdk/src/link.rs` (same shape), `examples/snowflake-sdk/
src/links.rs` (`parse_str`, `partition_links`; its maintainer adapted GitHub's tokenizer), and
`examples/distribution-registry-sdk/src/pages.rs` (`parse_next`). Their *grammar* (several field
lines, comma-separated link-values, `<target>`, `;` parameters, token or quoted-string values with
escapes, `rel` holding several space-separated relation types, case-insensitive parameter names
and relation types, rejecting malformed input instead of treating it as "no next page") is
generic HTTP mechanics. Their *policy* differs and must stay in each SDK: which relations matter,
same-origin/path/collection checks, page-index rules, how ambiguity (two `next`) is handled.

## Goal

1. Add a `link` feature to `concord-pagination` (optional dependency `url` from the workspace; no
   `reqwest`, no `http`, no Concord crate — keep pagination independent of HTTP crates otherwise).
   Module `concord_pagination::link` with a small, documented public API, for example: parse the
   field values (`impl IntoIterator<Item = &[u8]>` or `&str`s) into an ordered list of `Link`s
   (target resolved against a base `&url::Url`, the response URL; relation types; other
   parameters, raw) or a typed `LinkError` that says what was malformed (static text and byte
   positions only: never echo the header value); plus a convenience lookup of a relation that
   reports "absent", "one" or "several". Read the four implementations and their tests first and
   choose the smallest API that serves all four without carrying any provider rule. Unknown
   parameters are kept or ignored, never an error. Decide and document empty-field handling,
   `rel` repetition (RFC: first wins / reject — choose and justify), and non-UTF-8 bytes.
2. Migrate GitHub, GitLab, Snowflake and the registry to it: delete their tokenizers, keep their
   provider validation in the SDK, enable the feature in their manifests. **Every existing test
   assertion must still hold unchanged** (malformed headers stay errors, ambiguity stays an
   error where it was one). If a provider test encoded a behavior the shared parser cannot
   reproduce, stop and explain the conflict instead of weakening the test.
3. Unit tests in `concord-pagination` for the grammar: multiple field lines, commas inside quoted
   strings, escaped quotes, relative and absolute targets, uppercase `REL`, several relations in
   one `rel`, missing `<`, unterminated quote, empty values, non-UTF-8, a `next` repeated.
4. Docs: the module rustdoc with a short example; one paragraph in `docs/RECIPES.md` (pagination
   section) pointing to it. Update `scripts/verify.sh` with a `test-pagination-link` step
   (`cargo test --locked -p concord-pagination --features link`) and a doc step if the crate's
   docs build per feature there; update `scripts/check-graph.py` so `concord-pagination` may
   depend on `url` and still never on reqwest/Concord crates (with and without the feature).

## Verification you run (bounded; `CARGO_BUILD_JOBS=6`, one cargo job at a time)

`cargo test --locked -p concord-pagination` (default and `--features link`), and
`cargo test --locked -p <each migrated SDK package>`; clippy `-D warnings` on those packages;
`python3 -I scripts/check-graph.py`. Report commands, exit codes and test counts.

## Report (final message)

The public API you chose and why (what each of the four needed), lines removed from each SDK, the
provider rules that stayed local, behavior differences you found between the four parsers and how
you resolved them, and anything that did not pass.
