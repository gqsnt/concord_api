# Testing API clients

Separate three proof layers.

First, construction tests import the real declaration modules and assert
generated types and capability presence without executing. This proves
compiler and public-API shape while preserving inert construction.

Second, runtime tests use deterministic scripted or loopback transport. Queue
exact response heads/chunks, execute a real generated request, and assert:

- request attempts and ordering;
- sanitized hooks and completion events;
- first-excess withholding;
- one-additional-attempt authentication recovery;
- pagination sequence, cycle detection, and all-or-error behavior;
- cancellation without detached correctness work.

Third, compile-fail tests own invalid DSL and capability absence. Match focused
public-contract diagnostics; do not replace compiler privacy/capability proofs
with source-string searches.

Avoid external services and arbitrary sleeps. Use paused deterministic time or
explicit synchronization. A canonical example test imports
[the example library](../../crates/concord-examples/src/lib.rs); it does not
redeclare a synthetic API. See the
[construction test](../../crates/concord-examples/tests/construction.rs) and
[local runtime test](../../crates/concord-examples/tests/local_runtime.rs).

Contributor ownership and maintained commands are in
[Testing](../contributor/testing.md).
