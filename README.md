# Concord

Concord 0.1 has a historically validated Phase 10 release-candidate baseline. The current normative
specification set is **Normative Implementation Candidate** under the approved Phases 11-14
roadmap. Generated and stable prepared endpoints share one typed request,
body/authentication/limiter/deadline/hook/status/response lifecycle, private managed HTTP clients,
and one completion path. Phase 12 adds arbitrary request and response specification types, exactly
three sealed response acquisition modes, one universal `ApiResponse<T>`, one bounded `ByteStream`,
type-neutral generated/prepared integration, and optional integrated records/CSV/NDJSON. The
complete maintained graph passes on Rust 1.85.

Prepared pagination remains outside the 0.1 product contract. The built-in limiter is bounded
best-effort process-local pacing; strict or distributed enforcement remains an
application-supplied custom limiter. The optional `dangerous-raw-response` feature enables the
binary-wide compiled-crate raw-success capability. Its `RawResponse` exposes complete bounded
successful bytes and actual response headers to the caller, so it intentionally provides a reduced
secrecy guarantee while retaining logical/native URL separation and private Reqwest ownership.
Availability follows normal Cargo feature unification across the compiled binary.

Start with the [documentation overview](docs/README.md) and the
[normative specification index](docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md). For current
delivery state, see the
[implementation program](docs/planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md) and the
[Phase 12 report](docs/conformance/PHASE_12_REPORT.md). Phase 13 is active and `NOT_STARTED`; Phase
14 owns whole-product conformance closure and is the only phase that may restore
`Normative Release Final`.

Current validation entry points are:

```text
just fmt
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

Nextest is the canonical runner for Rust unit and integration tests. `cargo test` is retained only
for doctests, which Nextest does not execute. The default, all-feature, and no-default recipes are
intentional checks of different compiled surfaces. `just release` also compiles canonical examples
and benchmark targets, exercises explicit MSRV configurations, and runs independent no-default
configurations for every dangerous feature; all-feature testing is not treated as feature-isolation
evidence. Standalone Cargo fixture packages are reserved
for behavior that depends on package metadata or dependency topology; source-only macro and core
public-surface cases use ordinary integration tests or Trybuild.

The evidence catalog preserves every Phase 0-11 row byte-for-byte and appends one direct passing row
for each of the 127 Phase 12-owned active paragraph versions. Superseded pagination, response, and
stream IDs remain resolvable for their historical source commits. Current whole-product
release-registry and claim closure is reserved for Phase 14. This repository state is not a
published release: no remote publication, tag, registry upload, or deployment was performed.

Concord is pre-1.0. A breaking public-contract change requires a minor-version increment; patch
releases preserve the documented public contract and may contain compatible corrections only.
