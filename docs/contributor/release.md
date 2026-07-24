# Release and publication

All packages currently retain `publish = false` intentionally. This is not a
claim that packaging is complete: the repository snapshot has no approved
license identifier/file, canonical repository URL, publication owner, or
resolved façade-name ownership. Publishing any package would therefore be
unsafe and is outside the current authorization.

Before enabling publication:

1. approve and add the license and repository metadata;
2. verify ownership/availability of all public package names;
3. choose public packages and keep support packages private;
4. preserve the dependency order:
   core, macros/extras, then façade;
5. use exact sibling versions while releasing the set;
6. run `just verify-package` to create and inspect each local product archive;
7. build each extracted normalized library against the previously packaged
   siblings;
8. verify docs.rs feature metadata and public Rustdoc;
9. run MSRV, stable, all-feature, no-default, fixture, security, and deny
   lanes;
10. publish only with an explicit user instruction for that exact action.

The repository remains at version `0.1.0`. Phase 14 provides the migration
guide, changelog/release notes, bridge-version audit, compatibility removal,
and final package/version table. Do not infer publication success from
workspace build success.

## Final package and version table

| Package | Version | Product role | Direct Concord dependency | Registry state/order |
|---|---:|---|---|---|
| `concord-core` | 0.1.0 | Semantic runtime kernel and public extension contracts | none (test support is dev-only) | disabled; first product package |
| `concord-extras` | 0.1.0 | First-party downstream implementations | `concord-core = 0.1.0` | disabled; after core |
| `concord-macros` | 0.1.0 | Extras-agnostic declaration compiler | none | disabled; alongside extras |
| `concord` | 0.1.0 | User façade, feature owner, generated bridge composition | exact core, extras, macros | disabled; last |
| `concord-examples` | 0.1.0 | Internal importable product narrative | `concord = 0.1.0` | internal only |
| `concord-test-support` | 0.1.0 | Deterministic tests and fixture subprocess support | none | internal only; not a product dependency or registry prerequisite |
| `concord-perf` | 0.1.0 | Observational performance probes | `concord = 0.1.0` | internal only |
| `concord-xtask` | 0.1.0 | Portable documentation/fixture checks | none | internal only |

Package names use hyphens in Cargo metadata and underscores as Rust crate
identifiers. Applications name only `concord`.

## Generated bridge contract

The final generated contract version is **23** in both `concord-macros` and
`concord-core`; the façade reexports the exact handshake through its hidden
bridge. Any emitted bridge path or signature change requires an atomic version
bump in the compiler and runtime plus the mismatch fixture. The bridge is
version-checked compiler/runtime plumbing, not a stable extension surface.

## Release artifacts and limitations

- [Migration guide](../../MIGRATION.md)
- [Changelog/release notes](../../CHANGELOG.md)
- [Unsupported capabilities](../reference/unsupported.md)
- [Compatibility reference](../reference/compatibility.md)

Interactive rust-analyzer acceptance, remote Linux/Windows/macOS Actions,
registry ownership, and approved legal/repository metadata remain external
release gates until directly observed or supplied. Their absence does not
weaken the runtime contract, but it prevents a publication-ready claim.

`cargo run --locked -p concord-xtask -- package` technically verifies all four
product archives in dependency order. It inspects each normalized manifest and
checks each extracted library with locked resolution against already packaged
siblings. Ordinary registry preparation still cannot resolve unpublished
siblings; that is a release-state fact, not a product dependency-cycle or
test-support prerequisite.

## Dependency and feature policy

Public packages never use wildcard sibling versions. Implementation packages
have empty default features. `concord` owns user defaults and forwarding.
Examples depend only on the façade. Core never depends upward and macros stay
extras-agnostic.

Run `just verify-release-local` before requesting remote release validation.
Record unavailable tools, remote platforms, editor sessions, and network
acquisition separately.
