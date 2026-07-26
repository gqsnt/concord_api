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
source-derived and deterministic within one generated client contract, not
globally namespaced across unrelated `api!` expansions.

Build the source ZIP with:

```text
just source-archive
```

The xtask creates `target/` when needed and delegates archive creation to
`git archive`, so the recipe works in a fresh clone. The ZIP contains only
tracked content from `HEAD` and no `.git` directory or ignored build output.
Use `target/concord-source.zip` as the review or release source artifact; do
not substitute a ZIP of the working directory.

## Generated bridge contract

The canonical current version, owners, bump decision, atomic update sequence,
and mismatch proofs are maintained in
[Generated contract versioning](generated-contract.md). A release that changes
the private compiler/runtime protocol must update every owner and the isolated
contract-mismatch fixture together. The bridge remains implementation
plumbing, not a downstream extension API.

## Release checklist

Before enabling publication:

1. approve and add legal and repository metadata;
2. verify registry ownership for every selected public package;
3. keep support and tooling packages private;
4. run `just verify`;
5. retain successful rust-analyzer diagnostics and analysis-statistics output;
6. run `just package` and inspect every normalized product manifest;
7. run `just source-archive`, compare ZIP entries with tracked `HEAD`, and
   inspect the ZIP;
8. inspect no-default, selected-feature, and all-feature generated Rustdoc;
9. confirm `rate-limit` remains a default façade feature, its selected façade
   and extras Rustdoc is warning-free, and the removed unpublished names are
   absent;
10. confirm generated value mutation uses only `set_*`, absence-capable
    `clear_*`, and declaration-default `reset_*`; compile defaulted `Option`
    and `Option<Vec<_>>` with all three controls, and inspect the four
    compile-fail proofs that bare methods are absent from client builders,
    scope façades, explicit endpoint snapshots, and named request wrappers;
11. confirm effective direct and final-host-derived rate partitions reject
    pagination-controlled dependencies under current and Rust 1.85 semantic UI
    owners;
12. confirm generated Rustdoc says “deterministic declaration rule ID” and the
    manual records the non-global namespace boundary for shared limiters;
13. confirm request and response empty-chunk limits appear once in the generated
   limits reference and remain available without default features;
14. record the exact branch, commit, and clean worktree status;
15. obtain successful remote Linux, Windows, and macOS CI results;
16. publish only after an explicit instruction naming the packages and order.

See the repository `CHANGELOG.md`, [compatibility reference](../reference/compatibility.md),
and [unsupported capabilities](../reference/unsupported.md).
