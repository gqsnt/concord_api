# notion / e5: one field of the provider error document

**Provider changelog.** Notion error documents (`{"object":"error","status":..,"code":..,
"message":..}`) now carry an optional `documentation_url` string linking the documentation of the
error. It may be absent. Nothing else changed.

Public names the hidden check uses:
- `ProviderError::documentation_url: Option<String>` (public field), reachable from
  `Error::Provider { error, .. }`.
