# kubernetes / e5: one field of the provider error document

**Provider changelog.** The `Status` error document's `details` object carries `name`, the name
of the object the failure concerns (`{"kind":"Status",..,"reason":"NotFound","details":
{"name":"web","kind":"pods"},"code":404}`); this SDK must now surface it. It may be absent.
Nothing else changed.

Public names the hidden check uses:
- `model::StatusDetails::name: Option<String>` (public field), reachable from
  `ApiError::read(error).await?.status()?.details`.
