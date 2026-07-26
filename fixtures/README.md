# Cargo graph fixtures

This workspace contains only proofs whose subject is Cargo resolution.

- `features` is invoked once per isolated absence/unification graph and owns the
  no-local-limiter fail-closed runtime proof.
- `dependency/renamed_facade` proves that generated paths resolve a renamed
  `concord` dependency.
- `dependency/contract_mismatch` replaces the façade bridge with an
  incompatible contract version and proves the generated handshake fails.

Enabled-feature success belongs to the main workspace matrix. Fixture targets
are redirected to the repository's `target/fixtures` directory by the fixture
runner. Contributors do not need to link this workspace in rust-analyzer unless
they are editing these manifest-level proofs.
