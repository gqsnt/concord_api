# Phase 10 Conformance Report

## Verdict

`PHASE_10_COMPLETE`

`Concord 0.1 Release Candidate`

This report supersedes the release-evidence conclusions recorded by the original Phase 10 chain;
it does not remove or rewrite that history. Concord 0.1 remains a locally validated release
candidate, not a published release. The corrected audit proves 504 current direct passing
requirement owners, 775 source-derived passing claim atoms, zero unresolved atoms, zero waivers,
zero partial release-final rows, and zero missing commit-scoped artifacts. All nine normative
documents are `Normative Release Final`.

## Correction authority and identities

- Correction starting HEAD: `8c8dc914194e99a4074bef378ae2ca481fb1b4b7`
- Branch: `master`
- Starting and final tree policy: clean
- Superseded source/test commit: `52ceef99e7cca1fac256a55d85296480817b199a`
- Superseded evidence/report/state commit: `5a6af781abf7551cc6da979c969f54f997572ffe`
- Superseded evidence anchor: `1c662d2e20b9e5c267743c108415de437bac6278`
- Superseded metadata descendant: `8c8dc914194e99a4074bef378ae2ca481fb1b4b7`
- Corrected immutable source/test commit: `9317a94cf020d81dbd216ed2a7dc999550586898`
- Corrected evidence/report/state commit: `5b157225b387387664a202d4cbfe5f85ba8c5289`
- Corrected evidence anchor: `e0648e36ea9450741724ac067cfe184500e724c6`
- Final metadata descendant: the documentation-only commit that replaces the anchor token above
- Head policy: `anchor-must-be-an-ancestor`

The correction baseline, object existence, ancestry, catalog hashes, and clean boundary were
observed before editing. The Phase 0-9 evidence prefix remained exactly 1,473 rows, 1,041,408 bytes,
and SHA-256 `a3f6878a4a15aa4d6c30616b1e26c8244568a32be04f1cb8ce4bb0a72dc983f1`.
No executable artifact changed between the superseded source commit and the correction baseline.

## Verified defects and correction boundary

The accepted raw-response product, generated feature gating, behavior-oriented naming, canonical
examples, public contracts, and one-path execution architecture were preserved. Correction was
limited to these verified defects:

1. The public-head benchmark measured a complete loopback request instead of preparation.
2. The credential-ready benchmark only acquired a handle and borrowed a value.
3. The former registry audit trusted totals, current-worktree paths, and one source identity for
   evidence that belongs to different commits.
4. Several Phase 10 rows had invalid paths or mismatched normative owners.
5. The security role did not select every registered probe, including the full raw-success
   hook/completion lifecycle assertion.
6. Workspace feature unification made an existing development-only absence test assert the wrong
   feature topology.
7. The first change-map correction did not give paths changed after the source commit
   evidence-state precedence.

No new production dependency, Cargo feature, stable public abstraction, unsafe block, requirement
ID, evidence label, waiver, or product behavior was introduced.

## Executable correction commits

| Commit | Correction |
|---|---|
| `7fa5de2cbc079a153e74ab8383dd19be42a7a33b` | Make benchmarks operation-exact; implement commit-scoped source/evidence/final registry modes and self-tests; select security probes by target. |
| `a6126319947cdc019bca3696a7bf9dffb2ddb5b2` | Distinguish exact development-only absence from valid workspace feature unification. |
| `42719f520d7e363a14c3b24502c3697e32c5678e` | Bind the raw-success security registry row to the existing exact lifecycle unit probe and execute it in the maintained role. |
| `9317a94cf020d81dbd216ed2a7dc999550586898` | Classify any exact path changed after the source commit as evidence-state, including paths also present in the superseded source ancestry. |

The final executable source identity is the fourth commit. No executable, test, fixture, example,
benchmark, manifest, lockfile, Nextest, recipe, or audit-script change occurred after it.

## Corrected performance package

The private `concord_perf` package still contains one harness-free, bounded local benchmark with
nine categories and no product threshold. Two categories now use an unstable, doc-hidden seam under
the existing `dangerous-dev-tools` boundary:

- `public_head_preparation` consumes a real public `Request<E>`, calls the production preparation
  path, and returns only aggregate observations. It performs no managed HTTP send or response
  processing.
- `credential_ready_preparation` constructs an HTTPS protected request with ready static
  credentials and runs the production ready preflight and exact-version finalization path. It
  performs no provider acquisition, refresh, or transport.

The seam exposes neither a native request nor credentials, authentication material, limiter state,
or a production plan. Exact development tests assert the named preparation/finalization flags, and
smoke output rejects any category that fails to execute. The other seven categories remain core
client construction; generated client/facade/request creation; codec encoding/decoding; body-limit
polling; limiter ready-path acquisition; pagination loop overhead; and stream wrapping. Smoke mode
executed all 9/9 categories with one local iteration.

## Commit-exact release-audit architecture

`scripts/audit-release-registry.ps1` has four deliberately separate modes:

- `SelfTest` executes 15 acceptance and rejection classes, including file/tree paths, missing and
  ignored paths, wrong identity classes, non-ancestry, unknown requirements, title/digest drift,
  untracked normative prose, dependency cycles, duplicate ownership, incomplete claims,
  unresolved requirements, and unapproved waivers.
- `Source` validates the immutable executable source, the 504-owner parse, and the 1,473-row
  historical prefix. It explicitly reports final evidence completeness as not assessed.
- `Evidence` validates the corrected evidence-state commit, resolves the evidence self-reference,
  and proves all registries and commit-scoped paths.
- `Final` additionally verifies the evidence-only anchor substitution, the two-file actual-anchor
  metadata substitution, exact controller identities, ancestry, and a clean final worktree.

Every semicolon-delimited path is trimmed, required to be durable, and resolved as a Git blob or
tree at the row's own `verified_commit`. Current-worktree existence is never substituted. Each
recorded commit must exist and be ancestral to the audited descendant. Source rows use
`9317a94cf020d81dbd216ed2a7dc999550586898`; report, final catalogs, normative status, and release
governance rows use `5b157225b387387664a202d4cbfe5f85ba8c5289`; the anchor is used only to freeze that state; and the
metadata descendant records the actual anchor. No final row cites `git_diff/`.

## Durable release registries

| Registry | Rows | Audited meaning |
|---|---:|---|
| `RELEASE_REQUIREMENT_REGISTRY_0.1.csv` | 504 | Exact owner document, title, paragraph digest, current direct evidence row, identity class, result, and waiver field for every requirement. |
| `RELEASE_CLAIM_REGISTRY_0.1.csv` | 775 | Every source-derived independent claim atom with exact normative metadata and proof path, command, commit, result, and later-owner field. |
| `RELEASE_CHANGE_MAP_0.1.csv` | 152 | Every observable path changed since the Phase 9 metadata baseline, mapped to `DOC-CHG-001` and applicable behavior owners. |
| `NORMATIVE_LANGUAGE_ALLOWLIST_0.1.csv` | 4 | Only the specification-index definitions of `MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT`, and `MAY`. |
| `NORMATIVE_DEPENDENCY_REGISTRY_0.1.csv` | 29 | Exact authority-direction edges among the nine normative documents; acyclic by deterministic graph audit. |
| `NORMATIVE_OWNERSHIP_REGISTRY_0.1.csv` | 6 | Unique owners for public signatures, DSL productions, generated naming, finite limits, lifecycle ordering, and feature/compatibility rules. |
| `RELEASE_VALIDATION_REGISTRY_0.1.csv` | 33 | Exact working directory, command, tool versions, feature graph, source identity, selected tests, exit status, result, and observation time. |

The authoritative Markdown parser finds exactly 504 stable definitions. The claim decomposition is
derived from normalized owner paragraphs rather than preserving the superseded reported total of
595. It yields exactly 775 claims, all direct and passing, with no `later_owner`.

## Corrected evidence ownership

The Phase 10 suffix retains 58 unique evidence IDs for the same requirements but corrects their
paths, commands, assertions, and identities. It contains 33 immutable-source rows and 25
evidence-state rows; 24 of the latter are cross-phase catalog or governance audits. All are direct
and passing. The minimum owner corrections include:

- `P10-RAW-005` now proves `SEC-RESP-003` body exclusion across error display/debug and sources,
  hooks, debug events, completion, and structural raw wrapper debug using the handler, exact raw
  runtime tests, and maintained security role.
- `P10-RAW-007` separates reduced-guarantee Rustdoc, structural `Debug`, logical/native URL
  behavior, and bounded runtime collection under `SEC-DANGER-001`.
- `P10-RAW-011` uses the core dangerous-feature UI role for disabled raw type and terminal
  diagnostics under `FEAT-DIAG-001`.
- `P10-RAW-013` uses evidence-state README and Rustdoc proof for the binary-wide Cargo feature
  unification rule under `COMPAT-FEATURE-001`.
- `P10-GOV-001` proves the documented pre-1.0 minor/patch compatibility policy.
- `P10-GOV-002`, `003`, and `004` respectively prove zero undocumented waivers, tracked normative
  language, and the acyclic dependency registry.
- `P10-GOV-005` uses the tracked release change map, never the ignored contract map.
- `P10-GOV-006`, `015`, `020`, `021`, and `022` bind implemented-documentation, language,
  terminology, non-goal surface, and no-unreviewed-expansion claims to exact durable audits.
- `P10-REL-023` is evidence-state evidence for `CONF-RELEASE-001` and records the exact final audit,
  not `just release` or source mode, as the evidence-completeness proof.

The known invalid `concord_core/src/execute/response.rs` citation was replaced by
`concord_core/src/response/handler.rs`. No final row cites
`git_diff/phase-10-validation.md`, `git_diff/phase-10-contract-map.md`, or any ignored artifact.

## Security registry execution

All 15 registered sentinel classes are selected by exact maintained Nextest expressions:
credentials and secrets; authentication header/query values; request body; multipart values;
successful response body; terminal status body; raw success body; native authenticated URL;
pagination cursor/identity; rate-limit key; provider values; public sensitive headers;
continuation path/query values; custom controller state; and collected items.

The maintained role executes nine integration selections and thirteen exact unit selections. The
raw-success row selects
`raw_response_reuses_limiter_hooks_observation_and_success_completion`, which proves the caller
receives the successful bytes while hooks, completion, limiter trace, automatic debug, and
structural wrapper `Debug` exclude the high-entropy body sentinel. The registry audit also rejects
unknown or unselected probes. Zero Nextest retries and bounded concurrency are retained.

## Preserved product and architecture

Under `dangerous-raw-response`, `concord_core::dangerous::RawResponse` retains private fields and
the exact stable accessors `url`, `status`, `version`, `headers`, `metadata`, `body`, and
`into_body`. Eligible buffered, JSON, text, custom buffered codec, bytes, and no-content generated
requests retain `.raw_response() -> RawResponseRequest<E>`. Streaming, prepared, paginated,
feature-disabled, and non-success raw surfaces remain absent.

Raw execution still uses one `Request<E>`, one private `EndpointSpec` bridge, one managed Reqwest
client, one response classification/collection path, ordinary body limits, authentication,
limiting, hooks, hidden native retry, deadlines, cancellation, request counts, and completion.
It returns logical rather than authenticated native URLs, complete bounded successful bytes and
actual headers, skips only the declared decoder and decoder-owned media validation, and exposes no
native Reqwest response. Direct await, `.response()`, metadata, pagination, prepared endpoints,
ordinary status errors, authentication recovery, limiting, and completion remain intact.

## Validation of immutable source

Every command below ran from `F:\projects\concord_api` against
`9317a94cf020d81dbd216ed2a7dc999550586898` and exited 0. The exact toolchain was
`rustc 1.85.0 (4d91de4e4 2025-02-17)`, Cargo 1.85.0, LLVM 19.1.7, Nextest 0.9.140,
Just 1.46.0, cargo-deny 0.20.2, PowerShell 7.6.3, Windows PowerShell 5.1.26100.8655, and
Git 2.45.1.windows.1.

Focused and release-role commands:

```text
pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest
pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode Source -SourceCommit 9317a94cf020d81dbd216ed2a7dc999550586898
cargo nextest run --profile ci -p concord_core --all-features --test development -E 'test(=benchmark_operations::benchmark_seams_execute_head_preparation_and_ready_credential_finalization)'
cargo bench -p concord_perf --bench concord_overhead --no-run
cargo bench -p concord_perf --bench concord_overhead -- --smoke
cargo nextest run --profile ci -p concord_core --all-features --test raw_response --test dangerous_feature_ui
just _test-dangerous-features
just _feature-matrix
just _examples
just _benchmarks
just _msrv
just _grammar-and-diagnostics
just _reqwest-assumptions
just _security-and-properties
just _rustdoc-audits
just _release-audits
```

Maintained commands:

```text
just fmt-check
just check
just check-all
just check-no-default
just clippy
just clippy-all
just test
just test-all
just test-no-default
just doctest
just doctest-all
just doc
just doc-all
just deny
just diff-check
just release
```

Observed totals include 536/536 default tests, 550/550 all-feature tests, one exact no-default test,
two default and four all-feature doctests, 64/64 grammar/diagnostic selections, 25 integration plus
five unit Reqwest probes, nine integration plus thirteen unit security/property selections, 10
canonical example targets, 9/9 benchmark categories, 68 grammar entries, 18 Reqwest assumptions,
15 security rows, five property families, 340 naming-audited executable files, and 36 coherent
Trybuild pairs. Cargo Deny passed advisories, bans, licenses, and sources; accepted duplicate-crate
warnings did not change policy. The final `just release` rerun completed the full graph in 97.9
seconds.

The Rust 1.85 matrix compiled core no-default and all-features, macros, downstream generated and
public-extension fixtures, canonical examples, and the generated dangerous raw workflow. Feature
validation covered core default/all/no-default, JSON, multipart, each compression capability,
each dangerous feature independently, raw terminal presence/absence, macros, fixtures, examples,
and benchmark compilation.

## Final evidence and status gate

After the immutable source matrix passed, the separate evidence-mode audit validated the corrected
evidence commit and all commit-scoped paths. The documentation-only anchor changes only the exact
`5b157225b387387664a202d4cbfe5f85ba8c5289` self-reference in approved evidence documents. The final two-file
metadata descendant changes only `e0648e36ea9450741724ac067cfe184500e724c6` in this report and the controller. The
tracked final audit then proves the exact source/evidence/anchor/metadata chain, clean worktree,
504 requirements, 775 claims, 1,531 evidence rows, 1,473 byte-identical historical rows, zero
waivers, zero partial final rows, zero unresolved atoms, zero missing paths, and nine
`Normative Release Final` documents.

No remote Git operation, publication, registry upload, tag, deployment, credential access, waiver,
or Phase 11 work occurred.
