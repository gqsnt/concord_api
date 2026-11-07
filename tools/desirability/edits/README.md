# I4 edit tasks: one provider fact changes, a fresh agent edits the SDK, a hidden oracle decides

Instrument I4 of `contracts/DESIRABILITY.md`, scored as amended by `PLAN_AMENDMENT_A1.md` §6:
the outcome of a task is (i) the hidden oracle, (ii) the authoritative semantic edit sites the
agent changed, (iii) missed propagation and (iv) for e4 the stage that detects an omitted
classification. Files and hunks are diagnostics only.

## The bank is sealed outside the repository

The answer-bearing bank lives in the loop workspace, **not** in this repository:
`<WS>/work/sealed/i4-bank/tasks/<sdk>/<kind>/` (`<WS>` = `/home/canarit/projects/Concord_Desirability_Loop`).
Per task it holds the hidden oracle (`oracle.rs`), the reference solution (`reference.patch`), the
known-bad and valid-variant controls (e4 `omission.patch` / `partial.patch`, `controls/*.patch`)
and the full `meta.json` (expected edit sites and their patterns, propagation checks, the
classification and its check, the controls, the variant notes, the oracle's test names). The
validation records (`validation.json`, `VALIDATION.md`, `VALIDATION_NOTES.md`) and the smoke-run
records (`smoke/`) sit next to it, outside the hashed listing.

This repository keeps only what the agent may see or what is needed to run the instrument:

- `tasks/<sdk>/<kind>/task.md`: the changelog entry and the public names the oracle calls (the
  agent's prompt is built from it);
- `tasks/<sdk>/<kind>/meta.json`: the non-revealing fields only (`sdk`, `kind`, `package`,
  `sdk_dir`, `title`, `public_names`); `run.py` and `validate.py` refuse to run if they differ
  from the sealed copy or if the public file carries any other field;
- `BANK.sha256`: the SHA-256 of the sealed bank's canonical listing (every file under
  `i4-bank/tasks/`, one line `<sha256 of file>  <path>` each, sorted by path; `run.bank_hash`).
  `run.py` refuses to run against a bank that does not hash to it; `validate.py` records the hash
  each validation ran on. Rewrite it with `validate.py --write-hash` after a deliberate bank change.
- `run.py`, `validate.py`, this README.

The bank is reachable only from the owner's file system. The agent's network is open (it must
reach its provider), and egress is not restricted, so nothing answer-bearing may be published:
see *Limits* for the commits that still carry the bank in their history.

8 control SDKs × 5 kinds = 40 tasks (condensed from the public `meta.json` titles):

| SDK | e1 version/prefix | e2 shared header | e3 optional list query | e4 sibling + classification | e5 document field |
|---|---|---|---|---|---|
| github | pinned `X-GitHub-Api-Version` 2026-09-01 | `X-GitHub-Client` (generated and native) | `triggering_actor` on both run lists + `RunFilter` | milestones listing | label `archived` |
| kraken-spot-rest | `/0/` → `/1/` (signed over the new path) | `API-Sign` → `API-Signature` | `country_code` on pairs | `POST /0/private/BalanceEx` | non-2xx `error` → `errors` |
| kubernetes | Ingress `networking.k8s.io/v1` → `v2` | optional `Impersonate-User` | `pretty` on the Pod lists | namespaced StatefulSet list/read | `Status.details.name` |
| mastodon | timelines `/api/v1/` → `/api/v2/` | optional `Accept-Language` | `exclude_replies` on both timelines | `GET /api/v1/statuses/{id}/context` | `error_description` |
| nager-holidays | `/api/v4/` → `/api/v5/` | optional `X-API-Key` | `subdivisionCode` | `GET /api/v4/NextPublicHolidays/{CountryCode}` | `nationalHoliday` → `global` |
| notion | `Notion-Version` 2026-09-03 | `Notion-Version` → `Notion-API-Version` | `archived` on both lists | `GET /v1/comments` | `documentation_url` |
| sentry | `/api/0/` → `/api/1/` | `X-Sentry-Organization` | `environment` on both lists | `GET .../issues/{id}/hashes/` | `errorId` |
| strava | `/api/v3/` → `/api/v4/` (OAuth unchanged) | `X-Strava-Client-Id` | `sport_type` | `POST /api/v3/activities` | `errors[].detail` |

## Tools

- `validate.py --scratch WT --all [--old-oracle REV] [--bank DIR]`: in a throwaway worktree at the
  base commit, for every task: the oracle does not pass without the reference; with it, the
  package's whole suite passes and the oracle's section reports every oracle test of its source
  as executed and `ok`; a harness control (the same tree with a `.cargo/config.toml` no-op runner)
  must not score as a pass; every known-bad control fails the oracle (with the stage that catches
  it) and every valid-variant control passes. Records go to the sealed bank (`validation.json`,
  `VALIDATION.md`). `--write-hash` rewrites `BANK.sha256`.
- `run.py --task SDK/KIND --provider codex|claude --commit SHA [--out DIR] [--bank DIR]`: one fresh
  agent on one task, isolated (below), then the measurements; writes `<out>/result.json`.
  Requested and observed model/effort are recorded separately (Codex rollout `turn_context`;
  Claude `modelUsage`). `--probe-only [--deep]` runs only the isolation check; `--replay PATCH`
  and/or `--replay-shell CMD` act in the agent's sandbox instead of an agent (instrument
  validation: the reference must score good, a control bad, a tampering attempt invalid).

### Isolation of the editing agent (`run.py`)

1. **Export**: `git archive SHA` minus `tools/desirability/`, unpacked into
   `<WS>/work/i4-runs/<run-id>/baseline` (made read-only, never mounted for the agent) and copied
   to `<run>/agent/src` with a fresh `git init` and one commit (trusted git environment). No
   object of the real repository is reachable from it.
2. **bubblewrap**: `--ro-bind / /`, then `--tmpfs ~` (the whole home: `~/projects` with the
   repository, every worktree, the loop workspace and the sealed bank, and also `~/.claude` and
   `~/.codex`, whose transcripts, rollouts and thread databases contain bank text) and
   `--tmpfs ~/projects`; then only `<run>/agent` (the export and the per-run provider home) and
   the per-run agent target directory are bound back writable, `~/.cargo` and `~/.rustup`
   read-only, the agent binary's release directory read-only, and the provider's credential file
   (`~/.codex/auth.json`) read-write into the fresh provider home (`<run>/agent/codex-home` =
   `CODEX_HOME`, with a three-line `config.toml`: model, effort and the owner's `service_tier`).
   The baseline, the measurement copy and the output directory are not visible. `/tmp`,
   `/var/tmp` and `/run` are private (the DNS resolver directory is bound back read-only); a new
   PID namespace with its own `/proc`; an empty environment plus an explicit list. The network is
   not unshared.
3. **Isolation check before every launch** (same sandbox): `ls ~/projects` shows only the path
   to the run and target directories, the export has exactly one reachable commit and no
   alternates, the bank, the baseline, the output directory and the transcript stores are absent,
   the PID namespace is private; the agent starts only if all hold. `--deep` also greps every
   readable file outside `/usr`, `/proc`, `/sys`, `/dev`, `/snap` and the toolchains for a bank
   marker, with a positive control string from the export.
4. **Codex sandbox mode**: Codex's own Linux sandbox cannot run inside bubblewrap on this host
   (its bundled bwrap needs a nested user namespace, denied by the AppArmor
   `bwrap-userns-restrict` stacking). The Codex lane therefore runs `-s danger-full-access`
   inside the bubblewrap sandbox, which is the confinement.

### Measurement never runs anything the agent could configure on the host

After the agent, the host only **reads** the agent's tree:

- a manifest of `.git/**` is compared with the one taken before the launch; changes beyond normal
  commits, index updates, branch switches and stashes (`run.GIT_NORMAL`: `index`, `HEAD` naming a
  branch or commit, `ORIG_HEAD`, `COMMIT_EDITMSG`, `packed-refs`, loose objects, `refs/heads/**`,
  `refs/stash`, `logs/**` of those) are tampering signals, and so are any `.cargo/` file, a
  changed `rust-toolchain(.toml)`, a nested `.git` and special files;
- the tree is copied to `<run>/measure` **without** any `.git`, any `.cargo` (whatever its type)
  and any cargo target directory (`CACHEDIR.TAG`); symbolic links stay links, special files are
  skipped; every copied file gets a fresh modification time, because the measurement target is
  shared between runs and cargo decides freshness by mtime (with preserved times, a tree whose
  sources are older than an earlier run's build reused that build: observed, see the controls);
- the diff (`agent.patch`, diagnostics) is `git diff --no-index --no-renames --no-ext-diff
  --no-textconv` between the immutable baseline and the copy, inside bubblewrap (network
  unshared, only the two trees and a scratch `HOME` visible) with `GIT_CONFIG_NOSYSTEM=1`,
  `GIT_CONFIG_GLOBAL=/dev/null`, `-c core.fsmonitor=false -c diff.external=
  -c core.hooksPath=/dev/null`;
- cargo runs on the copy inside bubblewrap (network unshared, its own target directory
  `work/targets/i4-measure` that no agent sandbox mounts) with the owner's `CARGO_HOME` (read-only
  in every agent sandbox; no `config.toml`) and the baseline's toolchain pinned by
  `RUSTUP_TOOLCHAIN`; the propagation reference counts apply `reference.patch` to a copy of the
  baseline, also in the sandbox.

### The oracle passes only if its tests ran

The oracle is copied to `examples/<sdk>-sdk/tests/oracle_<kind>.rs` in the measurement copy and
run with `cargo test -p <pkg> --test oracle_<kind> -- --format pretty`, then listed with
`-- --list`. `run.oracle_verdict` passes it only if the exit status is 0, **every** test function
of the sealed oracle source is reported as `test <name> ... ok`, no other test ran, nothing failed,
was ignored or filtered out, the single summary line counts exactly those tests, and the binary's
listing names exactly those tests. Exit 0 without that is `not_executed` (a no-op runner, a
harness replaced by the agent). `validate.py` applies the same verdict to the oracle's section of
the whole-suite run.

### Exposure and tampering → invalid

The transcript, the agent's diff, its changed paths and every `.git` file it created or changed
(loose objects decompressed) are scanned for paths only the hidden bank has (`run.LEAK`). Any
exposure hit or tampering signal sets `outcome.status = "invalid"` and nothing is scored (the
measurements stay under `unscored`).

### Scoring of edit sites

Each named provider fact is classified by where the agent's final tree states it: at the
reference site, at a declared alternative, elsewhere in the SDK (an undeclared location: read
`agent.patch`; the oracle stays authoritative), or not recognised. An unrecognised behavioural
fact (authoritative, classification or propagation role) is **`needs review`** when the oracle
passed (the behaviour is there; the pattern may not know its spelling) and **`not stated`** (fact
missing) when it did not; documentation and test-expectation facts are `not stated` when
unrecognised. Classification and propagation facts are about one place, so for them only the
reference file and the declared alternatives count. `outcome.facts` counts these classes;
`facts_stated` is the number of facts stated anywhere without the old fact left at the reference
site; `facts_needing_review` and `facts_not_stated` name the others. For e4, the classification
row says whether the reference site is present and whether the oracle passed; when the site is
absent it says `needs review` if the oracle passed, else which stage caught the omission.

## Validation (summary; the full record is sealed)

Run on the sealed bank `ca09313218be…` (= `BANK.sha256`) in a scratch worktree at base
`2090802a`, 2026-11-07 23:06–23:11 UTC, with `--old-oracle 67d391eb`:

- 40/40 tasks kept: every oracle fails without its reference (25 compile errors, 15 run-time
  failures); with the reference every package suite passes (1971 tests passed, 0 failed in total)
  and every oracle reports all of its tests executed and `ok` (96/96), its `--list` naming the
  same tests.
- Harness control (reference + `.cargo/config.toml` no-op runner): cargo exited 0 in all 40, the
  verdict is `not_executed` in all 40 (an exit-status rule would have scored 40 passes).
- Controls: all 18 known-bad controls fail the oracle (e4 omissions at the stages recorded in
  the sealed `meta.json`, 7/7); one new known-bad control passed the oracle at `67d391eb` and
  fails the current one; the one valid-variant control passes the oracle and the suite.
- Oracle audit for literal query and header order (all 40): five targets with two or more query
  pairs whose order no task prescribes now compare path + query pairs as a multiset; no oracle
  compares the order of different headers. Details in the sealed `VALIDATION_NOTES.md`.

## Instrument controls through `run.py` (replays)

`run.py --replay` / `--replay-shell` at base `2090802a`, 2026-11-08 (local), against the sealed
bank; result files in `<WS>/work/i4-runs/<task>-replay-<stamp>-ctl-*/out/`. All on
kraken-spot-rest/e4 except the last.

| Control | Outcome | Oracle (unscored when invalid) |
|---|---|---|
| reference | valid; 3/3 facts at the reference site; classified | passed, 3/3 tests executed |
| no change, right after the reference run (stale-build control) | valid; classification absent, caught at the caller's compile | compile_error |
| `.cargo/config.toml` no-op runner, no solution | **invalid** (tampering: `.cargo/config.toml`) | compile_error (`.cargo` left out of the copy) |
| reference + the same no-op runner | **invalid** (tampering) | passed, 3/3 executed (the runner was not used) |
| reference + `.git/config` `core.fsmonitor`, `diff.external`, `core.hooksPath` pointing at sentinel-writing scripts | **invalid** (4 `.git` changes beyond normal) | passed; **no sentinel file written** |
| reference + `git add -A`, `git commit`, `git checkout -b` | valid (16 `.git` files added or changed, all normal) | passed, 3/3 |
| a sealed weakness control of the task | valid | test_failed, 2/3 |
| notion/e4 valid variant (behaviour-preserving, spelled unlike the reference) | valid; classification fact and e4 row **needs review** | passed, 5/5 |

The previous measurement method (host `git add -N .` and `git diff` in the agent's tree),
re-enacted in a scratch directory with the same fsmonitor and external-diff settings, wrote both
sentinel files. Before the fresh-mtime copy, the no-op-runner control without a solution, run
right after the reference replay, scored the oracle 3/3 from the reference's build (runs kept in
`<WS>/work/i4-runs/superseded-20261108/`).

## Smoke runs

Smoke records (agent patches, logs, rollouts with the account identifiers of `session_meta`
withheld) are kept with the sealed bank in `<WS>/work/sealed/i4-bank/smoke/`, not here.

- `codex.kraken-spot-rest.e4.sealed-runner/` (2026-11-08 local / 2026-11-07 23:14–23:17 UTC, base
  `2090802a`, this runner, bank `ca09313218be…`): requested `gpt-6.1-sol` / `xhigh` /
  `danger-full-access` inside bubblewrap (owner `service_tier`); observed (rollout `turn_context`)
  `gpt-6.1-sol` / `xhigh` / `danger-full-access`. Isolation check passed before launch; the agent
  ran 2 min 12 s; oracle passed with 3/3 tests executed, existing tests (with the agent's own)
  52 passed / 0 failed; 3/3 facts at the reference site; classification classified; no `.git`
  change, no `.cargo` file, no exposure hit (status `valid`); 13 hunks in 6 files (diagnostic).
  One repetition: a smoke test, not a measurement.
- Earlier records, moved there unchanged: `codex.strava.e2.isolated/` (previous runner, base
  `2090802a`), `isolation-probe/` (`--deep` probe) and `codex.strava.e4/` (round 1, no sandbox).
- Claude: not run (the headless `claude -p` launch needs the owner's permission).

## Limits

- The agent's network is open (the session must reach the provider API) and egress is not
  restricted; the credential file bound into the provider home is readable inside the sandbox.
- The bank was committed in this repository's history before it was sealed: `88b23cf2`,
  `7f05872e` and `67d391eb` carry `tools/desirability/edits/tasks/**` (oracles, reference
  patches) and the validation notes, and they are reachable from the local branches `dz/l0-i3`,
  `dz/l0-i4` and `dz/l0-instruments` (none of which has a remote-tracking ref as of this change).
  Pushing those commits publishes the bank; sealing applies to the tree from this change on.
- The root file system is visible read-only: system files, `/etc`, installed tools.
- The Claude lane (`--provider claude`) uses the same sandbox with `CLAUDE_CONFIG_DIR` in the
  agent directory and the credential file bound through; it has not been run.
- Site and propagation checks are regexes over the final tree: an unrecognised behavioural fact
  with a passing oracle is reported `needs review`, never counted as stated or missing.
- The agent's own code (build scripts, proc macros, the SDK library) runs during the measurement
  build and tests, confined to the measurement sandbox without network. A library that prints
  forged test-harness lines from inside the oracle binary is not detected; `.cargo` runners and
  toolchain overrides are.
- One repetition per task and provider is a smoke test, not a measurement (A1 §3: ≥3 fresh
  repetitions where a decision rests on them).
