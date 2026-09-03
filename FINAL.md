# Concord v2 constitution

Concord 2.0.0 is the single active language and runtime contract in this
repository. It supersedes the previous major-version contract.

The product identity, projection laws, non-goals, and migration posture are
recorded in [the v2 architecture decision](docs/v2_architecture.md). Normative
syntax, execution, and cross-cutting rules live in the
[DSL specification](docs/spec.md), [runtime model](docs/runtime.md), and
[design invariants](docs/design_invariants.md).

There is no production v1 parser mode, public compatibility alias, or alternate
executor. Selected old spellings exist only in migration diagnostics and the
[migration guide](docs/migration_v1_to_v2.md).

The release gate is `just verify`; workspace version 2.0.0 and generated
contract handshake 32 move together.
