# P1 migration capsule: adopt the F2 response helpers across the existing SDKs

You migrate existing SDKs in the Concord Rust workspace (your working directory, a git worktree
at branch `sdk50/p1-f2-adoption`). Read `AGENTS.md`/`CLAUDE.md` if present and
`docs/AUTHOR_GUIDE.md` §5.11. You cannot commit; leave the change in the working tree. This is a
migration task: you are given the exact solution; nothing here is first-use evidence.

## New core surface (already implemented in `crates/concord-core`; do not change core)

- `concord::Error::head() -> Option<ResponseHead<'_>>` and `Error::meta() -> Option<ResponseMeta>`
  now cover **every** error raised after a response head arrived: `Status`, `Metadata`, `Decode`,
  `Limit`, mid-body `Transport`, failed drains. `Error::status()` is unchanged (Status only).
- `Received::decode_with_meta()` / `decode_limited_with_meta(limit)` → `Result<(T, ResponseMeta),
  Error>`: the declared handling plus the head (moved, not cloned, for finite outputs).
- `Received::document::<F, D>(limit)` → `(ResponseMeta, Result<D, Error>)`: any status, bounded,
  runs `F::check_content_type` and `F::context` before reading, then `F::decode_body`. A `Decode`
  failure keeps the bytes (`Error::body_unredacted()`). `IgnoreContentType<F>` skips the media
  check (for SDKs that today decode whatever the content type is).
- Already migrated (use as the pattern): `examples/frankfurter-sdk/src/lib.rs` (`SendPolicy`),
  `examples/gitlab-sdk/src/send.rs` (`api_error`), `examples/nager-holidays-sdk/src/lib.rs`,
  `examples/dropbox-sdk/src/{wire.rs,error.rs}`.

## Scope

For each SDK below, replace hand-written mechanics with the helpers **where they apply**:
(a) `let meta = received.meta(); match received.decode().await { Ok(v) => .., Err(e) => wrap(e, meta.clone()) }`
→ `decode_with_meta` (+ `From<concord::Error>` reading `error.meta()`);
(b) error-document reading via `bytes_limited` + `<Json as Decode<D>>::decode_body((), bytes)`
(which skips the format's head checks/context) → `received.document::<F, D>(limit)`, choosing `F`
or `IgnoreContentType<F>` so the **existing behavior on wrong-media bodies is preserved exactly**;
(c) `From<concord::Error>` impls that set `meta: None` → read `error.meta()` (this fixes losses
such as ClickHouse `Insert::send` returning `Error::Concord` without the head it received).

SDKs: openai, airtable, world-bank, notion, microsoft-graph, slack, twilio, open-meteo, snowflake,
linear, ntfy, distribution-registry, clickhouse, google-drive, pokeapi, amazon-s3, and the
controls github, stripe, shopify, salesforce, kubernetes (`examples/<name>-sdk`), plus
`examples/native`, `examples/quickstart` and `examples/compat` where they show the old error-body
recipe. Leave a site unchanged when a provider rule makes the helper wrong (Slack's HTTP-200
`ok:false`, Snowflake's 200/202 status-selected documents, S3's XML errors through its own
parser, provider-specific raw retention) and say why.

## Rules

- **Every existing test assertion must hold unchanged.** Do not edit tests except to fix a
  compile error caused by a changed private helper signature; never change an expected value.
  If a test encodes behavior the helper changes, keep the old code at that site and report it.
- Public SDK API changes: avoid them; where an error enum's payload type must change, prefer a
  variant payload change compatible with the existing patterns in the tests (Frankfurter changed
  `ErrorBodyError::Decode(CodecError)` to `Decode(concord::Error)`; every test used `_`). Report
  each public change.
- Fix the ClickHouse metadata loss (`Insert::send`) and the Salesforce lost `ApiUsage` on failed
  status/decode if the helpers make it natural; add one focused regression test for each fix you
  make (these are new tests, not edits).
- Build each changed package's tests (`CARGO_BUILD_JOBS=6 cargo test --locked -p <pkg> --no-run`)
  and clippy `-D warnings` on it. The sandbox denies loopback sockets, so you cannot run the
  suites; the coordinator will. Run `cargo fmt --all`.

## Report (final message)

Per SDK: sites migrated (file:symbol), lines removed/added in `src/`, sites deliberately kept and
why, public API changes, new regression tests. Then the remaining generic residue you saw that
these helpers do not remove (for the next design round).
