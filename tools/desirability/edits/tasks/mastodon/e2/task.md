# mastodon / e2: shared header (add an optional client-wide header)

**Provider changelog.** Mastodon instances localize error messages and some fields according to
`Accept-Language`. A client that has a preferred language sends `Accept-Language: <tag>` on
**every** request (REST and streaming); a client without one sends no such header, as before.
Nothing else changed.

Public names the hidden check uses:
- `Mastodon::with_language(self, language) -> Mastodon` (`language`: `&str` or `String`, for
  example `"de"`), chainable like `with_token`;
- without `with_language`, no `Accept-Language` header is sent.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
