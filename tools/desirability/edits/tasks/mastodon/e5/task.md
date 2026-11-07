# mastodon / e5: one field of the provider error document

**Provider changelog.** Mastodon error documents may now carry an optional `error_description`
string next to `error` (`{"error": "invalid_token", "error_description": "The access token is
invalid"}`). Documents without it are unchanged. Nothing else changed.

Public names the hidden check uses:
- `ErrorDocument::error_description(&self) -> Option<&str>`, reachable from
  `Error::Api(api)` via `api.document()`.
