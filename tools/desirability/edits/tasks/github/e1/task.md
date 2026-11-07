# github / e1: API version

**Provider changelog.** GitHub released REST API version `2026-09-01`; this SDK now targets it.
Every request (generated operations and the SDK's native `GitHubContext::request` /
`RepoContext::request` requests) must send `X-GitHub-Api-Version: 2026-09-01` by default; callers
may still choose another version with `set_api_version`. The documents this SDK models are
unchanged in that version. Nothing else changed.

Public names the hidden check uses:
- `ApiVersion::pinned()` / `ApiVersion::PINNED_TEXT` must be `2026-09-01`;
- a default `GitHubContext` sends `X-GitHub-Api-Version: 2026-09-01` on generated and native
  requests.
