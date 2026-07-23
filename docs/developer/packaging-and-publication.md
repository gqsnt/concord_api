# Packaging and publication posture

`concord` is the only documented application entry point. The implementation
packages are independently documented on docs.rs because their public semantic
contracts must remain reviewable, but their crate-level documentation directs
ordinary users back to the façade.

Package names use Cargo's conventional hyphens while Rust crate paths retain
underscores:

| Package | Rust crate path | Role |
|---|---|---|
| `concord` | `concord` | Public façade, features, defaults, macros, and stable paths |
| `concord-core` | `concord_core` | Semantic runtime kernel and extension contracts |
| `concord-extras` | `concord_extras` | Downstream implementations of public core contracts |
| `concord-macros` | `concord_macros` | Extras-agnostic declaration compiler |
| `concord-test-support` | `concord_test_support` | Internal deterministic support used by the explicitly unstable development feature |

Every sibling dependency uses the exact workspace version and disables default
features. A registry release therefore proceeds in dependency order:

1. `concord-test-support`;
2. `concord-core`;
3. `concord-extras` and `concord-macros`;
4. `concord`.

`concord-test-support` remains an internal, undocumented application entry
point even though it must be registry-resolvable for the core package's
explicitly unstable `dangerous-dev-tools` feature and development checks.
`concord-examples` and `concord-perf` are not release artifacts.

## Current release blocker

All workspace packages intentionally retain `publish = false` until the Phase
14 release audit. The supplied repository has no finalized license,
repository, ownership, or registry metadata, and the `concord` name on
crates.io is occupied by an unrelated package. Publishing the façade therefore
requires explicit name ownership or a registry/naming decision. No publish
operation should be attempted before that decision.

The dependency graph is otherwise shaped for publication: registry-resolved
workspace edges have exact sibling versions with defaults disabled,
implementation packages have conventional identities and empty defaults, and
registry artifacts have package descriptions plus docs.rs all-feature
metadata. Phase 14 owns the final metadata, package archive inspection, and
temporary-registry smoke build after the registry blocker is resolved.
