# Release process

All packages retain `publish = false`. The repository has no approved license
identifier or file, canonical repository URL, publication owner, or confirmed
registry-name ownership. Do not infer or add that metadata, and do not publish
without an explicit instruction for the exact operation.

This page separates local release evidence from external authority. A clean
local matrix can prove source, tests, documentation, packages, and archives at
one exact commit. It cannot approve legal metadata, reserve registry names,
establish remote CI on unobserved platforms, publish crates, or deploy the
documentation site.

## Release evidence

Record the candidate before running release gates:

```text
git rev-parse --abbrev-ref HEAD
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=all
git remote -v
```

Use one exact commit for tests, rendered docs, packages, and source archive.
If a tracked file changes after a gate, the evidence belongs to the earlier
tree and the affected owner must be rerun.

`CHANGELOG.md` is the human release summary. It should describe public
capabilities, compatibility changes, migration requirements, and important
fixes without copying the complete DSL, limit inventory, or internal
lifecycle. Link readers to the canonical book or Rustdoc owner.

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

- `concord-examples` contains importable product examples and uses the façade
  as its only Concord product dependency;
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

For `concord-macros`, inspect the normalized manifest and resolved feature
graph as well as archive closure. Its normal Syn requirement must be major 3
without `full`; the macro crate alone may enable `full` as a dev dependency
for its self-source parse test. Syn 2 and Syn 3 are an expected temporary
duplicate-major pair while upstream procedural-macro dependencies still
require Syn 2. Do not force those transitive dependencies forward or broadly
relax duplicate-version policy.

## Release-facing compatibility review

The release-facing body-progress contract has two feature-independent public
ceilings: `concord::body::MAX_EMPTY_REQUEST_CHUNKS` and
`concord::response::MAX_EMPTY_RESPONSE_CHUNKS`. The generated limits reference
must read both constants from core. Package review must preserve those exports
under no-default and all-feature builds; internal fairness budgets remain
private implementation details rather than compatibility constants.

The release-facing rate-limit surface uses the default façade feature
`rate-limit`, `concord::rate_limit::{LocalRateLimiter, RetryAfterObserver}`,
and recoverable `RateLimitErrorKind::StateCapacity`. The unpublished
`rate-limit-governor`, `GovernorRateLimiter`, and
`DefaultRateLimitObserver` names must not reappear as aliases. Package and
selected-feature Rustdoc checks must preserve the core custom-`RateLimiter`
contract when the first-party implementation is disabled.

The release-facing generated value-control surface uses `set_*` for every
public value mutation, `clear_*` when a value is absence-capable, and `reset_*`
when it has a declaration default. Those capabilities are independent:
defaulted `Option<T>` and `Option<Vec<T>>` values expose all three controls,
and `set_*` accepts the inner `T` or `Vec<T>`. Client/scope/endpoint
construction and navigation retain their domain names. Package examples,
façade UI, and Rust 1.85 semantic UI must compile the new names and prove the
unpublished bare mutation methods are absent on client builders, scope façades,
explicit endpoints, and named requests; do not add compatibility aliases.

Rate-limit partition validation is final-endpoint authoritative. Every
effective inherited or local direct value dimension must be independent of
pagination-controlled values, and `by [host]` must be rejected when final host
shaping recursively depends on one. Generated declaration rule IDs remain
source-derived and deterministic when `id` is omitted. Explicit generated IDs
remain exact across source renames; reuse within one invocation requires
identical ordered dimensions and windows, while cross-invocation compatibility
remains application-owned.

## Source archive

Build the source ZIP with:

```text
just source-archive
```

The xtask creates `target/` when needed and delegates archive creation to
`git archive`, so the recipe works in a fresh clone. The ZIP contains only
tracked content from `HEAD` and no `.git` directory or ignored build output.
Use `target/concord-source.zip` as the review or release source artifact; do
not substitute a ZIP of the working directory.

After creation, compare archive entries with:

```text
git ls-tree -r --name-only HEAD
```

The comparison must contain the same tracked entries exactly once. Inspect that
the archive has no `.git`, rendered site, temporary execution aid, Trybuild
`wip/`, or local review artifact. Because `git archive` reads `HEAD`, an
uncommitted fix is deliberately absent.

## Generated bridge contract

The canonical current version, owners, bump decision, atomic update sequence,
and mismatch proofs are maintained in
[Generated contract versioning](generated-contract.md). A release that changes
the private compiler/runtime protocol must update every owner and the isolated
contract-mismatch fixture together. The bridge remains implementation
plumbing, not a downstream extension API.

## Release checklist

For a local release candidate:

1. update and review `CHANGELOG.md`;
2. run `just verify`;
3. run no-default, default, selected-feature, all-feature, doctest, and
   warning-denied Rustdoc owners required by the changed surface;
4. retain successful rust-analyzer diagnostics and analysis-statistics output;
5. run `just package` and inspect every normalized product manifest;
6. run `just source-archive`, compare ZIP entries with tracked `HEAD`, and
   inspect the ZIP;
7. build the mdBook with 0.5.4 and complete the rendered-site review from
   [Documentation maintenance](documentation.md);
8. compile the performance benchmark and record deliberate timing observations
   separately;
9. record the exact branch, commit, worktree, tool versions, command results,
   unavailable checks, and external blockers.

Before enabling publication or any other external release action:

1. obtain an explicit instruction naming the action, packages, order, registry,
   remote, or deployment target;
2. approve and add legal and canonical repository metadata;
3. verify registry ownership for every selected public package;
4. keep support and tooling packages private;
5. obtain successful remote Linux, Windows, and macOS CI results for the exact
   candidate commit;
6. inspect no-default, selected-feature, and all-feature generated Rustdoc;
7. confirm `rate-limit` remains a default façade feature, its selected façade
   and extras Rustdoc is warning-free, and the removed unpublished names are
   absent;
8. confirm generated value mutation uses only `set_*`, absence-capable
   `clear_*`, and declaration-default `reset_*`; compile defaulted `Option`
   and `Option<Vec<_>>` with all three controls, and inspect the four
   compile-fail proofs that bare methods are absent from client builders,
   scope façades, explicit endpoint snapshots, and named request wrappers;
9. confirm effective direct and final-host-derived rate partitions reject
   pagination-controlled dependencies under current and Rust 1.85 semantic UI
   owners;
10. confirm generated Rustdoc distinguishes explicit stable rule IDs from
    derived deterministic declaration IDs and the manual records the
    cross-contract compatibility boundary for shared limiters;
11. confirm every precise `ClientBuildErrorKind` remains public, value-free
    except for optional static generated input metadata, and source-less; retain
    sentinel proofs for proxy, root PEM, identity PEM, and native build failures;
12. confirm request and response empty-chunk limits appear once in the generated
    limits reference and remain available without default features;
13. recheck the exact branch, commit, and clean worktree immediately before the
    authorized external action;
14. execute only the approved action and record its observed result.

## Related

- repository `CHANGELOG.md`
- [Compatibility and MSRV](../reference/compatibility.md)
- [Unsupported forms](../reference/unsupported.md)
- [Testing strategy](testing.md)
- [Documentation maintenance](documentation.md)
