# Release and publication

All packages retain `publish = false`. The repository has no approved license
identifier or file, canonical repository URL, publication owner, or confirmed
registry-name ownership. Do not infer or add that metadata, and do not publish
without an explicit instruction for the exact operation.

## Product order

| Package | Version | Role | Direct Concord product dependency |
|---|---:|---|---|
| `concord-core` | 0.1.0 | Runtime kernel and public semantic contracts | none |
| `concord-macros` | 0.1.0 | Extras-agnostic declaration compiler | none |
| `concord-extras` | 0.1.0 | First-party downstream implementations | `concord-core = "=0.1.0"` |
| `concord` | 0.1.0 | Application façade and generated bridge owner | exact core, macros, and extras |

If publication is approved later, publish core first, macros and extras after
core is available, and the façade last. Keep exact sibling requirements for
the coordinated release.

Internal workspace packages are not publication prerequisites:

- `concord-examples` contains importable product examples and depends only on
  the façade;
- `concord-test-support` contains deterministic and loopback test
  infrastructure;
- `concord-perf` observes public façade behavior;
- `concord-xtask` owns typed documentation, fixture, MSRV, policy, and package
  checks. Its core/extras path dependencies read normative limit constants.

## Local package proof

Run:

```text
just package
```

The package task processes core, macros, extras, and the façade in dependency
order. It:

1. creates each `.crate` archive with locked Cargo resolution;
2. inspects the normalized manifest and rejects path or test-support
   dependencies;
3. checks core and macros directly from their extracted archives;
4. simulates unavailable registry siblings only from already-created product
   archives;
5. checks every extracted library with all features and a locked dependency
   graph.

The simulation never patches test support. Passing this task proves local
archive structure and dependency closure; it does not prove registry
ownership or publication.

Build the source ZIP with:

```text
just source-archive
```

That recipe uses `git archive`, so the ZIP contains only tracked content from
`HEAD` and no `.git` directory or ignored build output.

## Generated bridge contract

The generated contract version is **24** in `concord-macros` and
`concord-core`. The façade reexports the handshake through its hidden bridge.
Any emitted bridge path or signature change requires an atomic version bump
and the retained contract-mismatch fixture. The bridge is compiler/runtime
plumbing, not a public extension API.

## Release checklist

Before enabling publication:

1. approve and add legal and repository metadata;
2. verify registry ownership for every selected public package;
3. keep support and tooling packages private;
4. run `just verify`;
5. run `just source-archive` and inspect the ZIP;
6. inspect generated Rustdoc and feature metadata;
7. obtain successful remote Linux, Windows, and macOS CI results;
8. publish only after an explicit instruction naming the packages and order.

See the [changelog](../../CHANGELOG.md), [compatibility reference](../reference/compatibility.md),
and [unsupported capabilities](../reference/unsupported.md).
