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
   core, extras/macros, then façade;
5. use exact sibling versions while releasing the set;
6. run `cargo package --list` and `cargo package` for each public package;
7. build a consumer from packaged artifacts, including a renamed façade;
8. verify docs.rs feature metadata and public Rustdoc;
9. run MSRV, stable, all-feature, no-default, fixture, security, and deny
   lanes;
10. publish only with an explicit user instruction for that exact action.

The repository remains at version `0.1.0`. Phase 14 owns the migration guide,
changelog/release notes, bridge-version audit, compatibility removal, and
final package/version table. Do not infer publication success from workspace
build success.

## Dependency and feature policy

Public packages never use wildcard sibling versions. Implementation packages
have empty default features. `concord` owns user defaults and forwarding.
Examples depend only on the façade. Core never depends upward and macros stay
extras-agnostic.

Run `cargo metadata`, the dependency policy test, fixture workspace checks,
warning-denied Rustdoc, and cargo-deny before any packaging dry run. Record
unavailable tools and network acquisition separately.
