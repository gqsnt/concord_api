# Concord

Concord 0.1 is a **locally validated release candidate** whose normative specification set is
**Normative Release Final**. Generated and stable prepared endpoints share one typed request,
body/authentication/limiter/deadline/hook/status/response lifecycle, private managed HTTP clients,
and one completion path. The release graph validates the complete feature matrix, public-only
extension and generated fixtures, canonical examples, local benchmarks, Rust 1.85 MSRV, security
and dependency assumptions, and all 504 normative requirements.

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
[Phase 10 report](docs/conformance/PHASE_10_REPORT.md). Phase 10 is the terminal implementation
phase in the approved program.

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

The release registry records direct passing evidence for every normative requirement. Historical
planned and partial evidence remains immutable and separate from current release evidence. This
repository state is not a published release: no remote publication, tag, registry upload, or
deployment was performed.

Concord is pre-1.0. A breaking public-contract change requires a minor-version increment; patch
releases preserve the documented public contract and may contain compatible corrections only.
