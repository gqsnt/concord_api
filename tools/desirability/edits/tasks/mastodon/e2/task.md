# mastodon / e2: shared header (add an optional client-wide header)

**Provider changelog.** Mastodon instances localize error messages and some fields according to
`Accept-Language`. A client that has a preferred language sends `Accept-Language: <tag>` on
**every** request (REST and streaming); a client without one sends no such header, as before.
Nothing else changed.

Public names the hidden check uses:
- `Mastodon::with_language(self, language) -> Mastodon` (`language`: `&str` or `String`, for
  example `"de"`), chainable like `with_token`;
- without `with_language`, no `Accept-Language` header is sent.
