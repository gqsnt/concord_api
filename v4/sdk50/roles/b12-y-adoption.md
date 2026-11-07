# Between-wave capsule Y: adopt `limit`, `ResponseHead::unsigned` and `concord-ratelimit`

You work in the Concord Rust workspace (your working directory, a git worktree on branch
`sdk50/b12-y`). Read `AGENTS.md`/`CLAUDE.md` if present, `docs/AUTHOR_GUIDE.md` §5.10a and §5.11,
`crates/concord-ratelimit/README.md`. You cannot commit. Network is enabled for loopback tests;
contact no external service. Migration task: the solutions are given; this is not first-use
evidence. **Do not edit `examples/fred-sdk` or `examples/usgs-earthquake-catalog-sdk`** (another
task owns them) or any crate under `crates/`.

1. **Declared limit.** In each SDK whose send policy (or wrapper) calls `decode_limited(N)` /
   `decode_limited_with_meta(N)` only to bound a finite success body, declare `limit N;` on the
   client (or the scope/endpoint where the bound differs) and call `decode()` /
   `decode_with_meta()` instead, so the native `prepare()?.send()` path is bounded too. Keep
   error-document bounds explicit. SDKs: nasa-neows, open-library, discord-rest, and the SDK20
   users (slack, linear, dropbox, google-drive, airtable, snowflake, world-bank, notion,
   microsoft-graph, distribution-registry). Skip a site where the bound is runtime-configured or
   differs per call (say why).
2. **Strict header fields.** Replace hand-written single-header numeric parsing with
   `ResponseHead::single` / `ResponseHead::unsigned` (`concord::FieldError`) where the existing
   semantics are the same or stricter only for inputs no test covers: nasa-neows `RateLimit`
   (its parser has the same semantics), and evaluate github-sdk / gitlab-sdk counters (report any
   behavior change; do not change a tested behavior).
3. **Discord rate limiting.** Migrate the generic parts of
   `examples/discord-rest-sdk/src/limiter.rs` (per-key windows, cooldowns, the global gate,
   waiting/refusing) to `concord-ratelimit`, keeping route-to-bucket aliasing, major-resource keys,
   Discord header/body parsing and error semantics in the SDK. Every Discord test (including its
   first-use consumer tests) must pass unchanged. Report lines removed/added.

Rules: never edit an existing test's assertions or expected values; add focused tests only where a
change adds behavior. Bounded verification (`CARGO_BUILD_JOBS=4`, one cargo command at a time):
each changed SDK's tests, clippy and rustdoc `-D warnings`. Report per SDK: sites changed, lines
removed/added, sites skipped and why, test counts.
