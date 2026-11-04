# Windows CI failure on the 4.3 PR: investigation and resolution

Status of the investigation: **cause not established; dynamic-port exhaustion was not observed
in the measured reruns, and the measurements argue against ordinary gate churn exhausting an
otherwise healthy range; no defect found in the code under test; diagnostics and a lifecycle
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
   `target/verify-socket-pressure.log` (printed by CI when a step fails). The `7eefdee` version
   used `netstat -ano`, which omits bound sockets that neither listen nor connect, and mixed
   IPv6 rows into the IPv4 range; the figures below therefore count visible IPv4/IPv6 TCP
   sockets and slightly overstate the free ports. The correction head collects `netstat -anoq
   -p TCP` (IPv4, bound sockets included) and prints `?` for unrecognized or failed output.

   | Run / job (Windows) | Checkout | Region | Peak TIME_WAIT | Fewest free dynamic ports | Excluded | Connections opened | Result |
   |---|---|---|---|---|---|---|---|
   | [41921343094](https://github.com/gqsnt/concord_api/actions/runs/41921343094) / 126894342080 (push) | `7eefdee` | eastus | 294 | 15,884 of 16,384 | 200 | 1,575 | success |
   | [41921346743](https://github.com/gqsnt/concord_api/actions/runs/41921346743) / 126894355705 (pull_request) | `e12492b` (merge of `5ed600b` and `7eefdee`) | westus3 | 306 | 15,870 of 16,384 | 200 | 1,427 | success |

   At step boundaries the visible occupancy of the dynamic range was 300–314 ports, about 2 % of
   it (roughly a 50-fold margin to the free ports reported). Within-step peaks are not sampled, `ActiveOpens` counts
   connections, not listeners or bound-only sockets, and these are other machines than the one
   that failed: the figures establish the gate's ordinary footprint on healthy runners, not the
   state of the failing runner.
4. **Repetition tied to the failure mode.** The identical Riot tests, with their full preceding
   workload, have passed on native Windows in job 126834404351 (same tree as the failure) and in
   both instrumented jobs above; earlier implementation runs passed too (VERIFICATION.md).

## Conclusion and evidence strength

- **Established:** the failure is the fixture's listener bind, before any provider request; the
  same tree passed on another runner; in the measured reruns the gate's visible socket footprint
  on native Windows left ≥ 15,870 of 16,384 dynamic ports free at step boundaries.
- **Not observed, argued against:** dynamic-port exhaustion by the gate's own connection churn
  (not observed in the measured reruns; the margin argues against ordinary churn exhausting an
  otherwise healthy range) and a fixture that accumulates connection tasks or listeners across
  cycles (the lifecycle control below would fail).
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
- a lifecycle control, `examples/riot/tests/fixture_lifecycle.rs::repeated_fixture_cycles_release_their_connections_and_listener`:
  100 cycles of the failing fixture lifecycle (ephemeral bind, kept-alive requests, client
  dropped, server dropped) asserting, within bounded waits, that the server's connection tasks
  end after the client closes and that its accept task (which owns the listener) ends after the
  server is dropped. The helper gained only counters and a `Probe` that reads them after the
  server is dropped (`examples/riot/tests/support/mod.rs`); responses and wire behaviour are
  unchanged. Negative controls (temporary local mutations): a connection task that never ends,
  and a `Server::drop` that no longer aborts the accept task, each fail at cycle 0; the real
  fixture passes.

The wire assertions, the isolated-example gate and the feature-isolation checks are unchanged.

## Independent review of the correction

Reviewer: Codex CLI 0.166.0, requested model `gpt-6.1-sol` with reasoning effort `xhigh`,
read-only sandbox; observed in the session header: `model: gpt-6.1-sol`, `reasoning effort:
xhigh`, `sandbox: read-only`. Scope: `git diff d4ebd34 2cdecd3` in a detached checkout of
`2cdecd3`. Verdict: **ACCEPT_WITH_FIXES**. Resolutions, all in the commit that follows `2cdecd3`:

| # | Finding (severity) | Resolution |
|---|---|---|
| 1 | `netstat -ano` omits bound non-listening ports, so `dyn_free` overstates (major) | `netstat -anoq -p TCP`; BOUND rows counted (control in `scripts/tests/test_socket_snapshot.py`); Linux `dyn_free` documented as an upper estimate |
| 2 | `connections() == 1` races reqwest's pool return (major) | assertion relaxed to 1–2 accepted connections; the bounded zero-live wait stays |
| 3 | "excluded" and "two orders of magnitude" overstate the evidence (major) | wording narrowed to "not observed in the measured reruns; argues against ordinary churn"; margin stated as roughly 50-fold over visible occupancy, with the measurement's limits |
| 4 | the control did not observe listener/accept-task teardown (minor) | accept-task counter and `Probe`; bounded wait after `drop(server)`; negative control added |
| 5 | failed or localized collector output became zeros (minor) | nonzero exit, missing English headers or unknown states give `?`; controls added |
| 6 | IPv6 rows mixed into the IPv4 range (minor) | the diagnostic is IPv4-only on both platforms |
| 7 | 60 s per command, no snapshot budget (minor) | 10 s per command, 20 s per snapshot; a timeout disables later snapshots of the run (`target/socket-snapshot.disabled`, removed at gate start) |
| 8 | STATUS points at verification that does not cover the correction head (minor) | STATUS marks the correction head's verification pending until its exact-commit results are recorded |
