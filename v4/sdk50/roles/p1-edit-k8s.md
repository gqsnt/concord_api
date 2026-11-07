# Independent maintenance probe: one new watch parameter in the Kubernetes SDK

You maintain `examples/kubernetes-sdk` (package `concord-example-kubernetes-sdk`) in the Concord
Rust workspace (your working directory, a git worktree). You did not write it. Read
`AGENTS.md`/`CLAUDE.md` if present, the SDK's `README.md` and public docs, then its source as
needed. You cannot commit; leave the change in the working tree. Network is enabled for loopback
tests; contact no external service.

## Ticket

Kubernetes 1.27+ supports **streaming lists** on watch requests: the query parameter
`sendInitialEvents=true` (with `resourceVersionMatch=NotOlderThan` and
`allowWatchBookmarks=true`) makes a watch start with synthetic `ADDED` events for the current
state and then a bookmark annotated `k8s.io/initial-events-end: "true"`. (Kubernetes API concepts,
"Streaming lists"; cite the page you rely on.) Add support for setting `sendInitialEvents` (and
the `resourceVersionMatch` it needs) on **every** watch endpoint of this SDK, available both on
each endpoint's own builder and through the SDK's generic watch adapter, with tests showing the
wire query on two different watch endpoints and one test using the generic adapter. Keep every
existing test and assertion unchanged.

## Report (final message)

Every file and line you changed (and why), how many places had to change to add one parameter to
all watch endpoints, whether the compiler guided you, what you had to read to find the right place,
anything confusing about the declaration or generated API, and test results
(`CARGO_BUILD_JOBS=3 cargo test --locked -p concord-example-kubernetes-sdk`).
