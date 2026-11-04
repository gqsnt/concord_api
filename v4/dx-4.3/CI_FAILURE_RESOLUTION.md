# Windows CI failure on the 4.3 PR: investigation and resolution

Status of the investigation: **cause not established; gate-induced dynamic-port exhaustion
excluded by measurement; no defect found in the code under test; diagnostics and a lifecycle
control retained.** Nothing was disabled, ignored, retried or weakened.

## The observed failure

| Item | Value |
|---|---|
| PR | [gqsnt/concord_api#2](https://github.com/gqsnt/concord_api/pull/2), head `d4ebd34e88a4698f9e936b53884ba50106643adb` |
| Run / job | [41904216870](https://github.com/gqsnt/concord_api/actions/runs/41904216870) (pull_request, attempt 1) / [126834420624](https://github.com/gqsnt/concord_api/actions/runs/41904216870/job/126834420624) `verify (windows-latest)` |
| Executed checkout | `72db8ae7dbd3b7db723a155afc49156cefd88502` (synthetic merge of main `5ed600b` and the PR head), tree `77dfa2c99e3ad79865d964af2d649b5408a46439` = the PR head's tree |
| Runner | Windows Server 2025 10.0.26100, image `windows-2025-vs2026` 20261030.294.1, region westus3 |
| Failed step | `test-examples-isolated`: `concord-example-riot` alone, test binary `wire` (8 passed, 1 failed, 0.02 s) |
| Failure | `optional_tournament_fields_are_omitted_not_null` panicked at `examples/riot/tests/support/mod.rs:137` (`TcpListener::bind("127.0.0.1:0")`): OS error 10055 (WSAENOBUFS) |
| Other jobs of the run | Linux success; nightly `PASS nightly-clone-compat`, 16 passed, 0 failed |
| Same tree, same minute | push run [41904212230](https://github.com/gqsnt/concord_api/actions/runs/41904212230), Windows job 126834404351 (canadaeast): **success**, every step |

The panic precedes client construction, requests and the optional-field wire assertions: the
failure concerns the loopback fixture's listener, not encoding. WSAENOBUFS is what Windows
returns when a `bind` to port 0 finds no free dynamic port, but Microsoft also documents it for
exhausted kernel socket buffers and leaked endpoints; the code alone does not identify which
resource was missing.

## Investigation

1. **Fixture lifecycle (source).** The Riot helper (a copy of the native/core helpers) owns its
   accept task and aborts it on drop; connection tasks are spawned separately and end when the
   client closes the connection or when the test's runtime shuts down. Each `#[tokio::test]` has
   its own runtime, so no task or socket outlives its test; the same pattern runs in every
   workspace test step on Windows without failure.
2. **The gate's own socket churn, Linux (local, unchanged `d4ebd34`).** Full
   `scripts/verify.sh --package --bench`, 37/37 PASS, sampled every second
   ([ci-failure/linux-d4ebd34-socket-churn.json](ci-failure/linux-d4ebd34-socket-churn.json)):
   912 TCP connections opened over the whole gate, peak 479 sockets in TIME_WAIT,
   `test-examples-isolated` 178 connections.
3. **The gate's own socket churn, native Windows.** Commit `7eefdee` adds
   `scripts/socket-snapshot.py` (diagnostic only): machine-wide TCP counters, the dynamic port
   range, its excluded and in-use ports, after every gate step and around every example package,
   with a summary line printed by `scripts/verify.sh` on every run and the full series in
   `target/verify-socket-pressure.log` (printed by CI when a step fails).

   | Run / job (Windows) | Checkout | Region | Peak TIME_WAIT | Fewest free dynamic ports | Excluded | Connections opened | Result |
   |---|---|---|---|---|---|---|---|
   | [41921343094](https://github.com/gqsnt/concord_api/actions/runs/41921343094) / 126894342080 (push) | `7eefdee` | eastus | 294 | 15,884 of 16,384 | 200 | 1,575 | success |
   | [41921346743](https://github.com/gqsnt/concord_api/actions/runs/41921346743) / 126894355705 (pull_request) | `e12492b` (merge of `5ed600b` and `7eefdee`) | westus3 | 306 | 15,870 of 16,384 | 200 | 1,427 | success |

   The gate's own use of the dynamic range peaks below 4 % of the free ports at step boundaries
   (within-step peaks are not sampled; they are bounded by the connections a step opens). An
   exhaustion of the dynamic range by the gate's own tests would need two orders of magnitude
   more sockets than measured.
4. **Repetition tied to the failure mode.** The identical Riot tests, with their full preceding
   workload, have passed on native Windows in job 126834404351 (same tree as the failure) and in
   both instrumented jobs above; earlier implementation runs passed too (VERIFICATION.md).

## Conclusion and evidence strength

- **Established:** the failure is the fixture's listener bind, before any provider request; the
  same tree passed on another runner; the gate's measured socket footprint on native Windows
  leaves ≥ 15,870 of 16,384 dynamic ports free.
- **Excluded (by measurement, at step granularity):** dynamic-port exhaustion caused by the
  gate's own connection churn or a fixture that accumulates sockets across tests.
- **Not established:** which resource the failing runner lacked. No counters exist for that
  virtual machine (its job predates the instrumentation). The remaining hypothesis is a
  runner-side transient condition (other processes or kernel buffer pressure on that machine); it
  is a hypothesis, not a proven cause.

Code changes to the tests or fixtures would therefore be inappropriate **as a fix**: no faulty
layer was demonstrated, and a bind retry would hide a recurrence rather than explain it. The
retained measures are:

- the per-step socket snapshots (`scripts/socket-snapshot.py`, controls in
  `scripts/tests/test_socket_snapshot.py`), so a recurrence on any runner comes with that
  machine's dynamic range, exclusions and usage;
- a lifecycle control, `examples/riot/tests/fixture_lifecycle.rs::repeated_fixture_cycles_release_their_connections`:
  100 cycles of the failing fixture lifecycle (ephemeral bind, kept-alive requests, client and
  server dropped) asserting that the server side releases its connection within a bounded wait
  before the next bind; the helper gained only a live-connection counter
  (`examples/riot/tests/support/mod.rs`). Verified to fail when a connection task is made to
  leak (temporary local mutation, cycle 0) and to pass on the real fixture.

The wire assertions, the isolated-example gate and the feature-isolation checks are unchanged.
