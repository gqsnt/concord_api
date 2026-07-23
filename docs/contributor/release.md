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
   test support, core, extras/macros, then façade;
5. use exact sibling versions while releasing the set;
6. run `cargo package --list` and `cargo package` for each public package;
7. build a consumer from packaged artifacts, including a renamed façade;
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
| `concord-core` | 0.1.0 | Semantic runtime kernel and public extension contracts | optional internal test support | disabled; after test support |
| `concord-extras` | 0.1.0 | First-party downstream implementations | `concord-core = 0.1.0` | disabled; after core |
| `concord-macros` | 0.1.0 | Extras-agnostic declaration compiler | none | disabled; alongside extras |
| `concord` | 0.1.0 | User façade, feature owner, generated bridge composition | exact core, extras, macros | disabled; last |
| `concord-examples` | 0.1.0 | Internal importable product narrative | `concord = 0.1.0` | internal only |
| `concord-test-support` | 0.1.0 | Deterministic tests, fixtures, and feature-gated core development support | none | internal user surface; registry prerequisite and first if approved |
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

Interactive rust-analyzer acceptance, non-Windows CI lanes, registry ownership,
and approved legal/repository metadata remain external release gates until
directly observed or supplied. Their absence does not weaken the runtime
contract, but it prevents a publication-ready claim.

The package inventories for test support, core, extras, macros, and façade
pass. Full package verification passes for test support and macros. Core
cannot complete registry preparation until exact test support is published;
extras and façade likewise wait for exact core. This is why the internal test
support package precedes product packages in the registry order even though
applications never depend on it.

## Dependency and feature policy

Public packages never use wildcard sibling versions. Implementation packages
have empty default features. `concord` owns user defaults and forwarding.
Examples depend only on the façade. Core never depends upward and macros stay
extras-agnostic.

Run `cargo metadata`, the dependency policy test, fixture workspace checks,
warning-denied Rustdoc, and cargo-deny before any packaging dry run. Record
unavailable tools and network acquisition separately.
