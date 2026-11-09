# H12 disposition — replay of the baseline Kubernetes e1 failure (2026-11-09)

Why: the Stop-1 review (`reviews/stop1-review-sol.md`, blocking) found that H12's one recorded behavioral failure —
baseline I4 Kubernetes e1 ("Ingress moved from `networking.k8s.io/v1` to `networking.k8s.io/v2`"), oracle 1 of 4
(`BASELINE.md`, I4 section) — had no defensible disposition: the H12 survey's stale-literal explanation covered the
propagation flags, not the oracle failure. The reviewer saw signs of instrument confounding: the saved agent patch
moves the shared path and `Ingress::API_VERSION` to v2, yet the oracle log reports v1; the existing-suite log has
E0460 ("found possibly newer version of crate `concord_core`") artifact errors; and the baseline ran two tasks at a
time on one shared measure target (`work/targets/i4-measure`). The baseline's Kubernetes e1 run
(05:02:12–05:04:48 UTC) overlapped Kubernetes e2 (05:02:22–05:04:49) — the same package built from two different
source trees into one target.

Method (`work/tools/h12_replay.sh`; outputs sealed in `work/sealed/h12-replay/`, log `work/logs/h12-replay.log`): the
baseline I4 runner (`work/wt/base` tools) at the baseline commit `734991e9`, `--replay` (no agent launched), one replay
at a time, **each on its own fresh measure target**, against the sealed oracle and the SDK's whole suite:

| replay | oracle | oracle tests executed `ok` | existing suite | missed propagation |
|---|---|---|---|---|
| the baseline agent's saved edit (`work/baseline/i4/runs/kubernetes.e1/agent.patch`) | **pass** | 4/4 | pass | 0 |
| known-good: the sealed reference patch | pass | 4/4 | pass | 0 |
| known-bad: a no-op change (one comment line in `PROVIDER.md`) | fail | 1/4 | pass | 3 |

The no-op tree fails exactly as the baseline run did (1 of 4: only "other groups did not move" passes), while the
agent's edit passes everything on a private target. **The baseline failure was an instrument artifact** — the oracle
and suite ran against artifacts of the base tree built concurrently into the shared target — not an editing failure.

Consequences:
- Corrected baseline I4: Kubernetes e1 is a valid, complete edit (oracle 4/4, no missed propagation): **40/40 oracle
  passes** (recorded 39/40), 7 propagation-flag tasks (recorded 8). Erratum in `BASELINE.md`.
- **H12 has no measured task deficit** (its survey: a client `path` already gives a shared prefix; the remaining
  propagation flags are stale test literals). "Not carded" stands, now with this disposition.
- Instrument lesson, applied to the LF holdout: concurrent I4 runs must use private measure targets (or run one at a
  time). This replay reconciles H12's one recorded failure only. Shared-target contamination can also produce false
  passes; the other 39 baseline records were not replayed and their independence is not certified — a baseline
  instrument limitation that stays recorded (stop review 2).
