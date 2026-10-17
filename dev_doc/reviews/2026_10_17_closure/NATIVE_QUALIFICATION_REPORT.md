# Native qualification and evidence boundaries

## Qualified sources

| Source | Scope |
|---|---|
| ae6006184f3dc903fdfe66c572098ac810e2c6c5 | Owner-pushed correction baseline |
| 91c6cb911aecb4278e1fbd8de1e6053591a3bbff | Corrected library/macro product, security dependency floor and exact EOF oracle |
| 5b3772236d7c1b71f86ebeba35f7d33a30cf3b64 | Corrected isolated no-TLS test fixtures; native Windows full gate passes |
| 579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31 | Native Linux full gate and paired performance campaign; only evidence tooling added since 5b |
| 2f647826dfefb61a6e5a6770f8af2c523198011e | Three stronger affected-endpoint assertions and attribution/archive tooling; library product unchanged |
| Final return commit | Exact identity is generated in FINAL_RETURN_RECEIPT.json; later CLI/test changes receive their own native checks and exact CI |

A comparison of Git blobs binds unchanged library, macro, example and
performance inputs to their qualified source. The sole IntoValue documentation
comment change is separately proved to be exactly one byte-for-byte sentence
replacement, then regenerated and checked in native Rustdoc. It does not call a changed
support CLI byte-identical or substitute a source comparison for running new
test oracles. The final return checks its changed tool/test inputs independently.

## Positive native gates already observed

[Qualification run 38981477842](https://github.com/gqsnt/concord_api/actions/runs/38981477842)
completed all three jobs successfully on 579e8eac:

- Linux: complete just verify-implementation, analyzer and release-audit;
  generated Rustdoc and package/source evidence retained.
- Runtime: five process repetitions per case and all native command receipts.
- Scale: all requested endpoint counts and repeated 1,000-endpoint cases.

[CI #28](https://github.com/gqsnt/concord_api/actions/runs/38978327078)
on 5b377223 completed the native Windows implementation gate from
2026-10-16 22:53:46Z to 23:29:43Z, followed by the successful analyzer
through 23:31:42Z. This is a complete native gate, distinct from platform smoke.

[CI #30](https://github.com/gqsnt/concord_api/actions/runs/38983860517)
on 2f647826 has native passing results for 1,624 default tests and 1,703
all-feature tests, zero skipped; 44 macro tests and the three compiler-contract
harnesses; quality/Clippy/workflow syntax; real MSRV; documentation and
packaged downstream use; Linux and Windows smokes; and the Linux analyzer.
Each smoke exported the clean exact source and ran its four deterministic
runtime controls. The final native archive and final-return receipt own the terminal state
of the longer Windows job rather than retroactively changing an earlier
in-progress observation.

The generated mutation corpus is an unconditional integration test included in the
two successful workspace suites. The CI profile prints slow/failing test
names, so an individual PASS line is not invented. The strengthened cases
require exactly all five affected endpoints for credential-source changes
and default addition/change.

The package verification's native result identifies version 3.0.0-beta.1.
It checks extracted package archives, extracted Rustdoc and a downstream
consumer. Successful package tests do not publish any package.

[CI #29](https://github.com/gqsnt/concord_api/actions/runs/38981477798)
on the measured source 579e8eac also completed the full Windows gate:
implementation succeeded at 2026-10-17 00:23:52Z, analyzer at 00:27:33Z,
and the job at 00:29:48Z. This qualifies the exact source used for the Linux
and performance campaign on both mandatory platforms.

[The UI capture](https://github.com/gqsnt/concord_api/actions/runs/38985826352)
produced two genuine E0061 diagnostics for a required body argument omitted
or supplied twice. A second run without overwrite passed both negatives and
the real current-language positive fixture with stable inputs. The final
maintained harness contains those two entries; final CI must pass it normally.

## Toolchain and environment

The maintained stable pin is Rust 1.97.1; the minimum supported Rust is 1.86.0.
Windows and Linux are the two mandatory supported platforms. macOS is not
claimed as supported or qualified by these runs.

The earlier MSRV-labelled CI jobs #25 and #26 invoked bare cargo and actually
selected the repository's 1.97.1 pin. They are not accepted as 1.86 evidence.
From #27 onward the seven Cargo invocations explicitly select +1.86.0,
and the native logs identify rustc 1.86.0 and passing semantic MSRV UI checks.
The maintained just msrv recipe already selected that version explicitly.

The dedicated Linux qualifier runs on Ubuntu 24.04 with its runner image,
CPU, tool versions, environment and source snapshots recorded. Its reduced
debug information applies to engineering qualification. Performance uses a
separate, normal unstripped debug scale build and release runtime probes;
those regimes are not combined into one timing claim.

The local execution environment disconnected while a complete Linux run on
5b was still active. Its native exit is UNKNOWN. It is not relabeled PASS,
and no lost terminal log is reconstructed. Read-only GitHub access and native
Actions runners provided the separately identified replacement evidence.

## Earlier failures are preserved

| Attempt | Genuine outcome and correction |
|---|---|
| First aggregate launch | Invalid platform-qualified just setting; replaced by supported windows-shell selection |
| CI #25 / 6085722 | Stale three-poll EOF oracle failed; strict completion needs four polls. All-feature step was skipped after the failed default step. |
| CI #25 dependency policy | Four Rustls advisories plus the existing three license rejections |
| CI #26 / 91c6cb9 | Workspace tests passed; isolated no-default facade fixtures incorrectly assumed TLS, so the Windows full gate failed |
| CI #27 / 4265c06 | Facade fixture corrected; isolated core expiry fixture still assumed a TLS provider, so the full gate failed |
| CI #28 / 5b37722 | Both isolated fixture corrections present; native Windows full gate and analyzer pass |
| Disconnected local 5b run | Unknown native exit, preserved as a separate incomplete attempt |

No assertion was widened to accept the old incorrect behavior. The EOF test
requires exactly four polls and retains byte/media/length/completion assertions.
No production TLS rule was relaxed: uncredentialed fixtures use HTTP, the
credentialed fixture runs under default-tls, and a dev-only crypto provider
is initialized where required by test dependency unification.

## Dependency policy

Rustls is selected at 0.23.46 in both affected lockfiles and in the workspace
minimum. The four newly found advisories name that release as patched.
The later native deny summary reports advisories ok, bans ok, sources ok,
and licenses FAILED. The remaining three BSD-3-Clause policy rejections concern
alloc-no-stdlib, alloc-stdlib and brotli.

The owner's exclusion of license work does not convert that command to
success. Its real failure remains in the final archive and CI conclusion.
No advisory ignore, license exception or allowlist change was introduced.
No reachability claim is inferred for every advisory solely from dependency
presence.

## Final exact-source checks

The final-return workflow reruns the affected tests and tools, then waits for
the exact final source CI. It requires every engineering job to succeed and
checks that any remaining dependency failure is precisely the separately
reported license-policy case. The original workflow ZIP, Windows native log,
API states before/after collection and source identities are preserved.

The final native commands include evidence/parser/envelope integrity tests,
formatting, the complete xtask tests with example targets, real-history
release-audit, clean source-export, native warning-clean Rustdoc for the
corrected public sentence, and the actual contract CLI controls. There are
33 native command receipts: nine engineering commands and 24 CLI controls.
Independent results are retained even when a preceding command fails.
Expected CLI nonzero statuses are adjudicated by both their numeric exit and
their intended semantic outcome.

The public source/evidence success receipt is written only after checking the frozen package
manifest and every ZIP member against its SHA-256. A failed assembly keeps
incomplete evidence and does not create FINAL_RETURN_COMPLETE. The private
acceptance annex is delivered separately to its owner and is excluded from
the GitHub archive; package integrity does not claim that private annex is
inside the public evidence envelope.

## Limits

Synchronous user callbacks are checked before and after execution; they
cannot be forcibly preempted. Exact producer EOF is required for normal body
completion; early server responses do not imply producer drainage. Native H2
tests use prior knowledge; a separate TLS/ALPN matrix is not claimed.
Multipart interoperability is bounded to the actually exercised aiohttp 3.14.4
parser and scenarios. No remote provider latency, global RSS, macOS or human
study result is inferred.
