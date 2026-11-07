# kubernetes / e3: optional query value on every list operation of a resource

**Provider changelog.** The Pod list operations (`GET /api/v1/pods` and
`GET /api/v1/namespaces/{namespace}/pods`) accept an optional boolean `pretty` query value that
asks the server to pretty-print the response (the same parameter the other list endpoints
document). Unset, nothing is sent, as before. Nothing else changed.

Public names the hidden check uses:
- `kube.core().pods().set_pretty(bool)` and `kube.namespace(ns).core().pods().set_pretty(bool)`
  send `pretty=<bool>` (adding it to every list operation is allowed).
