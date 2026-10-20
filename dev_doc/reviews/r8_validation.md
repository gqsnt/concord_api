# Concord V3 R8 validation

Qualified implementation candidate `cef236aa2502df5653d77de2975051b5a6d49565`. The later handoff commit
changes Markdown/JSON evidence only; terminal source binding proves every other
tracked input remains byte-identical. Exact final SHA/tree, export and remote/CI
verification live in the post-commit receipt supplied with the return ZIP.
Runtime repair commit `060b74e8c397a45f72adf494c799650a795fa43a` retains identical production code and
locks; subsequent commits change only checkout attributes and test fixtures/
regressions. Initial platform/analyzer passes retain their earlier scope. The
final complete platform gates and analyzers qualify this exact candidate anew.
Final source export and docs checks qualify the later documentation-only commit.

## Native reproduction and focused qualification

Windows native x86_64 MSVC, Rust 1.97.1. Separate detached BEFORE checkout and
target at baseline R7; only test overlays and test-support fixture extensions.
Production provider prefix was byte-checked against baseline. No reset/stash,
R6 reimport or mutation of existing checkouts. Findings map to actual run names
in [the evidence index](r8_evidence_index.json) and [dispositions](r8_findings.md).

`before-auth-native`, `before-oauth-native`, `before-provider-native`,
`before-body-cooldown-native` and `before-wire-native` retain nonzero exits and
their actual runtime assertions. Initial setup failures (`before-auth`,
`before-oauth`) are not reproductions. The first TLS body probe's second default
response and absent close header were repaired without changing its oracle;
corrected native known/chunked cases report two identical body-limit errors.
Every setup/failed attempt and full log is included, with the overlay ZIP and
`receipt_interpretation.md`; no required R8 log is omitted.
The initial Windows full gate failed three historical migration byte/hash checks
because core.autocrlf=true converted unpinned JSON/captured harness files to
CRLF. The next gate exposed the same CRLF-only issue in generated-member text
snapshots. `.gitattributes` now pins JSON and captured text to LF; task-owned fixture checkout
bytes were restored from exact Git blobs. No captured hash or assertion changed.
`windows_fixture_normalization.json` and `windows_fixture_normalization2.json`
preserve all 39 before/after hashes; the all-feature recording regression,
generated-member snapshot regression and complete Windows repeat qualify it.
`linux-focused-current` exposed a CONNECT fixture Broken pipe panic on normal
peer closure; its one-helper transport failure is not accepted as TTL evidence.
`windows-implementation3` was deliberately interrupted before changing fixture
inputs; its partial log is retained and is not a complete gate pass. The proxy
now treats the existing peer-disconnect error class consistently on writes as
well as reads. Delayed TLS heads declare Connection: close to prevent stale-pool
reuse. The stricter TTL probe requires Unavailable after two bounded helper
exchanges and zero application handoffs, not any failure. With the same repaired
fixture, `before-ttl-fixture-corrected` still sends the expired token; its 120s
control passes. `linux-ttl-fixture-corrected` passes all eight TLS cases. Both
complete final platform gates qualify the fixture follow-up and stricter oracle.

`after-focused-final2` passed 10 core R8 tests, one extras R8 boundary test,
eight public TLS R8 tests and the existing R7 OAuth-delay regression. It runs
the following with Rust 1.97.1 and --locked (not a zero-selected filter):

- cargo test -p concord-core --lib --all-features r8_ -- --nocapture
- cargo test -p concord-extras --lib --all-features r8_ -- --nocapture
- cargo test -p concord --test r8_oauth --all-features -- --nocapture
- cargo test -p concord --test r7_oauth_delay --all-features -- --nocapture

The final committed test input strengthened repeated-malformed response
controls; both complete implementation matrices qualify that exact source.

## Maintained qualification

| Receipt | Native command exit | Source SHA |
|---|---:|---|
| `windows-implementation4` | `0` | `cef236aa2502df5653d77de2975051b5a6d49565` |
| `linux-implementation2` | `0` | `cef236aa2502df5653d77de2975051b5a6d49565` |
| `windows-auxiliary2` | `0` | `cef236aa2502df5653d77de2975051b5a6d49565` |
| `windows-analyzer2` | `0` | `cef236aa2502df5653d77de2975051b5a6d49565` |
| `linux-analyzer2` | `0` | `cef236aa2502df5653d77de2975051b5a6d49565` |
| `linux-deny-default2` | `0` | `cef236aa2502df5653d77de2975051b5a6d49565` |
| `windows-deny-all-features2` | `4` | `cef236aa2502df5653d77de2975051b5a6d49565` |
| `linux-deny-all-features2` | `4` | `cef236aa2502df5653d77de2975051b5a6d49565` |

Each platform's `just verify-implementation` covers formatting,
default/no-default/all-feature checks, feature variants, warning-denied Clippy
and tests; explicit pinned facade/core UI; MSRV 1.86.0 product libraries and UI;
fixture locks/prepared offline checks; docs-check/doc-audit/doctests/rustdoc;
downstream clients; xtask/contract/migration tools; benchmark compilation and
clean maintained source export. No performance measurement or claim is made.
Fixture locks were fetched explicitly before offline execution on both platforms,
and rust-src was installed with the maintained pin. Root/fixture locks unchanged.

Windows separately passed `just package`, `just workflow`, default `just deny`
and `just analyzer`. Linux/WSL passed `just analyzer`; Linux standalone pwsh and
actionlint were unavailable, so package/workflow were qualified on native
Windows. These are supported-platform results for new code, not old R7 passes.
Analyzer exit 0 includes existing informational/inactive-code diagnostics and
the pre-existing dev-dependency cycle; no zero-diagnostic assertion is made.

Windows 11 build 26200, native x86_64 MSVC; Ubuntu 26.04 Linux/WSL2 kernel
6.18.40.1, x86_64 GNU. Both pin rustc 1.97.1
(8bab26f4f 2026-07-14); MSRV 1.86.0 (05f9846f8 2025-03-31).
Linux uses a task-owned ext4 target and TMPDIR; no foreign files were removed.
Receipts include command/cwd, actual SHA/tree, complete tracked inputs and
untracked overlay hashes, lock bytes, toolchain/platform, UTC times, native exit
and full output/hash. The wrapper's initial cp1252 tail-print failures occurred
after storing successful child logs/receipts; their native exits are preserved.

## Reserved policy and external limits

Default-feature deny passes on Windows and Linux/WSL. All-feature deny was actually run on
both platforms and exits 4: advisories/bans/sources pass; licenses reject
BSD-3-Clause in alloc-no-stdlib, alloc-stdlib and brotli (**POL-01**). It is a
reserved owner policy decision, not a functional defect; deny policy, advisory
ignores, features and dependencies were not changed to waive it.

External Edition 1 conformance, human comprehension/independent human review,
broader V3-O07 performance, stable compatibility freeze, legal/package namespace
authority, merge and publication remain unaccepted or reserved. No model/backend
identity attestation is made. CI is observed for the exact final pushed SHA in
the external push receipt and completed job evidence, separately from local gates.
