# github / e2: shared header (add a header to every request)

**Provider changelog.** GitHub now requires every REST request to carry the header
`X-GitHub-Client: concord` identifying the client library family. Requests without it are
rejected. This applies to every request the SDK sends to the API: generated operations and the
native requests built by `GitHubContext::request` / `RepoContext::request` (used for operations
the SDK does not declare). Nothing else changed.

Public names the hidden check uses:
- every generated operation and every native request from `GitHubContext::request` /
  `RepoContext::request` sends `X-GitHub-Client: concord`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
