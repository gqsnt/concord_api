# sentry / e5: one field of the provider error document

**Provider changelog.** Sentry error documents (`{"detail": ".."}`) may now carry an optional
`errorId` string identifying the failure for support requests. Documents without it are
unchanged. Nothing else changed.

Public names the hidden check uses:
- `Problem::error_id: Option<String>` (public field), reachable from
  `Error::Provider(error)` → `error.document` → `Ok(problem)`.
