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
classification with its check and its omission evidence, the controls, the variant notes, the
oracle's test names). The validation records (`validation.json`, `VALIDATION.md`,
`VALIDATION_NOTES.md`), the smoke-run records (`smoke/`), the runner-control patches
(`runner-controls/`) and snapshots of earlier banks for `--old-oracle` (`previous/<hash>/`) sit
next to it, outside the hashed listing.

This repository keeps only what the agent may see or what is needed to run the instrument:

- `tasks/<sdk>/<kind>/task.md`: the changelog entry, the public names the oracle calls and the
  *Limits of this change* (no build-time code, no harness change; the agent's prompt is built
  from it);
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

- `validate.py --scratch WT --all [--old-oracle REV|DIR] [--bank DIR]`: in a throwaway worktree at
  the base commit, for every task: the oracle does not pass without the reference; with it, the
  package's whole suite passes and the oracle's section reports every oracle test of its source
  as executed and `ok`; a harness control (the same tree with a `.cargo/config.toml` no-op runner)
  must not score as a pass; every known-bad control fails the oracle (with the stage that catches
  it, and for an e4 omission the stage its classification-specific evidence points to) and every
  valid-variant control passes. `--old-oracle` runs the controls against an earlier oracle too (a
  commit that still carried the bank, or a bank snapshot directory such as
  `<bank>/previous/<hash>`). Records go to the sealed bank (`validation.json`, `VALIDATION.md`).
  `--write-hash` rewrites `BANK.sha256`.
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
  and any cargo target directory (`CACHEDIR.TAG`); only regular files and directories are copied:
  a **symbolic link or special file** anywhere in the tree is left out, recorded
  (`tampering.left_out_of_measurement`) and makes the session **invalid**, so the copy holds no
  link that a later host-side write could follow; every copied file gets a fresh modification
  time, because the measurement target is shared between runs and cargo decides freshness by
  mtime (with preserved times, a tree whose sources are older than an earlier run's build reused
  that build: observed, see the controls);
- the diff (`agent.patch`, diagnostics) is `git diff --no-index --no-renames --no-ext-diff
  --no-textconv` between the immutable baseline and the copy, inside bubblewrap (network
  unshared, only the two trees and a scratch `HOME` visible) with `GIT_CONFIG_NOSYSTEM=1`,
  `GIT_CONFIG_GLOBAL=/dev/null`, `-c core.fsmonitor=false -c diff.external=
  -c core.hooksPath=/dev/null`;
- cargo runs on the copy inside bubblewrap (network unshared, its own target directory
  `work/targets/i4-measure` that no agent sandbox mounts) with the owner's `CARGO_HOME` (read-only
  in every agent sandbox; no `config.toml`) and the baseline's toolchain pinned by
  `RUSTUP_TOOLCHAIN`; the copy is bound **read-only** for every build and test (only the
  measurement target directory is writable), so build scripts, proc macros and tests cannot
  rewrite the sources, the injected oracle or the harness crate; one `cargo metadata --offline`
  before them sees the copy writable, to refresh `Cargo.lock` (it runs no build script or macro);
  the propagation reference counts apply `reference.patch` to a copy of the baseline, also in the
  sandbox.

### The oracle passes only if its tests ran

After the existing tests, the host writes the oracle to `examples/<sdk>-sdk/tests/oracle_<kind>.rs`
in the measurement copy without following anything the agent could have placed
(`run.install_oracle`): each parent directory is opened relative to the previous one with
`O_NOFOLLOW|O_DIRECTORY` and checked with `lstat` to be a real directory inside the measurement
root, and the file is created with `O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW` (mode 0444). An existing
file or link at that path, or a parent that is not a real directory, refuses the installation:
the oracle is not run (`not_run`) and the session is **invalid**. The oracle is then run with
`cargo test -p <pkg> --test oracle_<kind> -- --format pretty` and listed with `-- --list` (copy
read-only); afterwards its SHA-256 must still equal the sealed source's
(`oracle_installation.unchanged`), and the copy must still hold no link or special file, or the
session is invalid. `run.oracle_verdict` passes the oracle only if the exit status is 0, **every**
test function of the sealed oracle source is reported as `test <name> ... ok`, no other test ran,
nothing failed, was ignored or filtered out, the single summary line counts exactly those tests,
and the binary's listing names exactly those tests. Exit 0 without that is `not_executed` (a no-op
runner, a harness replaced by the agent). `validate.py` applies the same verdict to the oracle's
section of the whole-suite run.

Every expected value in an oracle is computed by the oracle itself or written as a literal, never
by SDK code the agent can edit: the Kraken signatures are recomputed with `hmac`/`sha2`/`base64`
(HMAC-SHA512 over `path + SHA256(nonce + body)` with the decoded secret, as the SDK's canary and
`first_use` tests do), constants the task leaves unchanged are compared with their literal value,
and SDK enums are compared with `matches!` rather than their (editable) `PartialEq`.

### Build-time code, the harness and forged verdicts → invalid

The oracle's assertions run inside the candidate's test binary, next to the agent's library code.
Three layers keep the verdict out of the agent's reach; the second and third are checked on the
host from the diff and the two trees (no agent code runs for them):

1. **Read-only sources** (above): no build script, macro or test can rewrite the oracle or the
   harness crate during the measurement (control: a `build.rs` that rewrites the oracle).
2. **No build-time code, no other harness** (`run.build_time_code`): a session whose diff adds or
   changes a `build.rs`, adds or changes `[build-dependencies]`, a `build`, `links` or `metabuild`
   key, `[patch]`/`[replace]`, declares a proc-macro crate or changes a file inside an existing one
   (`crates/concord-macros`), changes the harness crate `examples/testserver/`, or redirects a
   harness crate (the test server, `serde_json`, `tokio`, `base64`, `hmac`, `sha2`, `bytes`) to a
   `path`/`git`/other-registry source the baseline did not give it, is **invalid**. No task needs
   any of these; every `task.md` says so under *Limits of this change*.
3. **Static scan of the added lines** (`run.harness_scan`) — **invalid**: libtest-shaped text
   (`test result:`, `... ok|FAILED|ignored`, `running N tests`) in a code line or a non-prose
   file; `process::exit`/`abort` (and `libc` equivalents) added in library code (`src/`, not
   `src/main.rs`/`src/bin/`); code that runs before `main` or replaces a symbol (`link_section`,
   `.init_array`, `#[used]`, `no_mangle`, `export_name`, `asm!`); writes to stdout/stderr from
   library code when the diff also carries libtest-shaped text. **needs review** (scored, read
   `agent.patch` before using the scores; `outcome.status = "needs review"`): stdout/stderr
   writes in library code without such text (a debug print is the usual cause), exit/abort in
   test or other code, libtest-shaped text in a comment or a prose file.

**Residual risk.** The oracle's assertions still run in the candidate's process, so a deliberately
adversarial library can subvert them with code the scan does not recognise (output assembled from
fragments or bytes, `include_bytes!` of a data file, raw pointer writes into the test harness,
behaviour that only differs under the oracle). The layers above make that require deliberate,
reviewable code in the agent's diff; they do not make it impossible. The measurement target
directory is writable during the measurement and shared between runs (build speed); code run by an
agent's tests can write there and could, with knowledge of cargo's fingerprints, affect a later
run (pass `--measure-target` with a fresh directory to rule that out for a decision run).

### Exposure and tampering → invalid

The transcript, the agent's diff, its changed paths and every `.git` file it created or changed
(loose objects decompressed) are scanned for paths only the hidden bank has (`run.LEAK`). Any
exposure hit or tampering signal sets `outcome.status = "invalid"` and nothing is scored (the
measurements stay under `unscored`). Tampering signals (`tampering.signals`): a `.git` change
beyond normal, a `.cargo` file, a changed toolchain file, a nested `.git`, a symbolic link or
special file in the tree, a refused oracle installation, an oracle changed during the measurement,
a link or special file in the copy after the build, build-time code or a harness change, and a
harness-forging pattern (previous section).

### Scoring of edit sites

Each named provider fact is classified by where the agent's final tree states it: at the
reference site, at a declared alternative, elsewhere in the SDK (an undeclared location: read
`agent.patch`; the oracle stays authoritative), or not recognised. A behavioural fact (every role
but `docs`) is stated only by code: its pattern is matched with the comments and rustdoc blanked
(`run.code_text`), so a comment such as `// TODO: admit /0/private/BalanceEx` earns no credit
(the file is listed under `new_fact_only_in_comment_or_doc_in`); `docs` facts are matched on the
raw text. The comments are found by a scan that follows the Rust lexer (`run.rust_comment_spans`):
line comments `//`, `///`, `//!`; block comments `/* */`, `/** */`, `/*! */`, nested; `#[doc ..]`
and `#![doc ..]` attributes whole, across lines. String contents are never comments (facts are
often string literals): normal strings with escapes, byte strings `b".."`, C strings `c".."`, raw
strings `r#".."#`, `br#".."#`, `cr#".."#`, strings spanning lines; character literals (`'"'`,
`'\''`, `'\u{..}'`, `b'"'`) are told from lifetimes and labels (`'a`, `'static`). An unrecognised behavioural
fact (authoritative, classification or propagation role) is **`needs review`** when the oracle
passed (the behaviour is there; the pattern may not know its spelling) and **`not stated`** (fact
missing) when it did not; documentation and test-expectation facts are `not stated` when
unrecognised. Classification and propagation facts are about one place, so for them only the
reference file and the declared alternatives count. `outcome.facts` counts these classes;
`facts_stated` is the number of facts stated anywhere without the old fact left at the reference
site; `facts_needing_review` and `facts_not_stated` name the others. An old fact that remains at
the reference site only inside a comment or rustdoc (a historical note such as "before v5 the SDK
requested `/api/v4/..`") does not count against the fact when the oracle passed; it is listed
under `old_fact_only_in_comment_or_doc` as a review candidate.

Missed propagation compares the old-pattern matches with the reference solution's, split into
code and comments/documentation (`run.rust_comment_spans` in Rust, as above; the whole text of
prose files such as `*.md`). When the oracle passed and the matches beyond the reference are all in
comments or documentation, the check is not missed but listed under
`propagation_review_candidates` (read `agent.patch`: a stale doc line and a historical note look
the same to a pattern); with a failing oracle the old count rule stays. For a check over prose
files (the `docs` class, `*.md`) every match is documentation, so with a passing oracle such a
check is a review candidate, never missed propagation.

For e4, the classification row says whether the reference site is present in code (the sealed
`classification.check`, matched with comments and rustdoc blanked; a match only in a comment is
recorded as `classification_check_only_in_comment_or_doc` and counts as absent) and whether the
oracle passed; when the site is absent it says `needs review` if the oracle passed. Otherwise the
detecting stage is attributed only on **classification-specific evidence naming the new
operation**: each e4 task's sealed `classification.omission_evidence` holds a `regex`, a pattern
of the omission's own failure (Kraken `SigningInput`, Sentry `unsupported operation`, the caller's
`ListOperation` bound, ...), and a `subject`, a term naming the new operation, type or path
(Kraken `BalanceEx` / `extended_balances`, Sentry `IssueHash` / `hashes`, Kubernetes
`StatefulSet`, ...). Both must match inside **one failure block** of a failed stage
(`run.failure_blocks`: one rustc diagnostic in cargo's own output, or one failed test's output
from its `---- <test> stdout ----` header, which names the test), not merely somewhere in its
log; the first failed stage (SDK build, existing tests, the caller's compile, the oracle at run
time) with such a block is the detecting stage (validated against the omission control).

A test's captured output may contain lines shaped like libtest's own delimiters, so a target's
failure framing is read against its **failed-test inventory** (`run._test_section_blocks`): the
target's last `test result:` line, the last `failures:` line before it (the final list of failed
names), the first `failures:` line (where the failure blocks begin); the final list must equal the
run list's `test <name> ... FAILED` names (test modes such as ` - should panic` removed) and the
summary's failed count. Only a header naming a test of that inventory is a block boundary; a
header-shaped line naming anything else stays inside the block it occurs in and is dropped from the
text the evidence is matched on; a captured `failures:` line ends nothing. Two headers naming the
same test, output before the first header, an inventory that disagrees, or failure lines without a
summary (e.g. a forged `Running` line) make the target **ambiguously framed**: it yields no block,
and a failed stage whose output is ambiguous ends the search with `cannot be assessed`
(`ambiguous_framing`) **before** any of its blocks is accepted as evidence (review 13: captured
output holding a complete fake target section followed by another `Running` line otherwise
produced a matching block next to the framing problem), since captured output may have
manufactured the block and the stage may have detected the omission unseen. Control:
`work/i4-runs/fix5b-controls/framing_controls.py` ("review 13: …"; the round-5 rule credits it).

Failures without evidence are listed (`failed_without_evidence`;
`evidence_outside_a_block_naming_the_operation` when the failure kind appeared, but not in a
failure naming the new operation, e.g. `SigningInput` from a test of the existing `Balance`
operation), not attributed; with no matching stage the row says `cannot be assessed`.

## Validation (summary; the full record is sealed)

Run on the sealed bank `e7d963b9e837…` (= `BANK.sha256`) in a scratch worktree at base
`2090802a`, 2026-11-08 00:34–00:39 UTC (fix round 5: the comment scan and the failure framing;
the bank is unchanged), with `--old-oracle <bank>/previous/ca09313218be` (the round-3 bank, kept
as a snapshot next to the hashed listing). Fix round 4 changed only the seven e4
`omission_evidence` entries (a `subject` added); the bank before it, `af768919e1da…`, is kept as
`<bank>/previous/af768919e1da` (same oracles, reference and control patches):

- Unchanged against the fix-4 run (2026-11-08 00:10–00:16 UTC, same bank) and, through it, the
  fix-3 run on `af768919e1da…` (2026-11-07 23:43–23:48 UTC), compared per task (verdict, oracle
  before/after, oracle tests ok, suite counts, harness control, every control's stage, oracle
  status and old-oracle status, evidence stage and texts, oracle hash): 0 differences in 40 tasks
  each time.
- 40/40 tasks kept: every oracle fails without its reference (25 compile errors, 15 run-time
  failures); with the reference every package suite passes (1971 tests passed, 0 failed in total)
  and every oracle reports all of its tests executed and `ok` (96/96), its `--list` naming the
  same tests.
- Harness control (reference + `.cargo/config.toml` no-op runner): cargo exited 0 in all 40, the
  verdict is `not_executed` in all 40.
- Controls: all 20 known-bad controls fail the oracle; the 7 e4 omissions are caught at the stages
  recorded in the sealed `meta.json` (7/7), and in all 7 the evidence (its `regex` and its
  `subject` in one failure block) points to that same stage (github `unrecognized-collection` with
  `list-milestones`, kraken `SigningInput` with `extended_balance`, kubernetes `ListOperation`
  with `StatefulSet`, mastodon ``no method named `status_context` ``, notion the `Ok`
  `Notion.comments` response with `list_comments`, sentry `unsupported operation` with `hashes`,
  strava `ambiguous: false` with `Strava.activities.create`). Kraken-spot-rest/e4 control `signature-strips-ex`
  (`Credentials::signature` signs BalanceEx over `/0/private/Balance`): it **passed** the round-3
  oracle (whose expected signature came from the same editable function) and fails the current
  one at run time; `signs-balance-path` (the same wrong path chosen in the policy) fails both. The
  one valid-variant control passes the oracle and the suite.
- Oracle audit for expectations computed by editable SDK code (all 40, every assertion's expected
  side): kraken-spot-rest/e1, e2, e4 recomputed `API-Sign` with `Credentials::signature`; notion/e2
  and e4 took the unchanged version from `NOTION_VERSION`; github/e4 compared operation ids with
  `operation::MILESTONES`/`LABELS`; mastodon/e4 built its expected id with `StatusId::new`;
  nager-holidays/e4, notion/e4 and e5 compared SDK enums through their `PartialEq`. All now use
  independent computations, literals or `matches!`. Details in the sealed `VALIDATION_NOTES.md`.

## Instrument controls through `run.py` (replays)

### Fix round 5: token-aware comment scan, failure framing against the failed-test inventory

`run.py --replay` at base `2090802a`, 2026-11-08 01:39–01:41 (local; 00:39–00:41 UTC), bank
`e7d963b9e837…`, measurement target `<WS>/work/targets/base`; result files in
`<WS>/work/i4-runs/<task>-replay-2026110801[34]*-ctl5-*/out/`, the four new runner-control patches
in `<WS>/work/sealed/i4-bank/runner-controls/`. *Round-4 rule*: the round-4 code evaluated on the
same tree (the patch applied to the base) and the same run's cargo logs
(`<WS>/work/i4-runs/fix5-controls/old_rule_on_replays.json`; "-" = not evaluated). All on
kraken-spot-rest/e4 unless named.

| Control | Outcome (current rule) | Round-4 rule, same tree and logs | Oracle |
|---|---|---|---|
| omission + the agent's own test of `extended_balances`, whose captured output prints `failures:` before the `Local(SigningInput)` panic (new) | valid; caught by **existing tests (or the agent's own tests)** (`SigningInput` with `extended_balance` in `---- extended_balances_are_signed stdout ----`) | hidden oracle at run time (the block ended at the captured `failures:`, the evidence was cut off) | test_failed, 0/3 |
| omission + the `Balance` test failing with `Local(SigningInput)`, whose captured output prints `---- extended_balances stdout ----` (new) | valid; existing tests failed **without** evidence (the fake header is no boundary and not evidence); caught by the hidden oracle at run time | **existing tests** (the fake header opened a block naming the operation) | test_failed, 0/3 |
| omission + `let _quote = '"'; // TODO: admit /0/private/BalanceEx` in `prepare_signed` (new) | valid; classification check only in a comment → **absent**; allowlist fact **not stated**; hidden oracle at run time | classification **present**, allowlist fact at the **reference site** | test_failed, 0/3 |
| reference + `let _open = "/*";` before the allowlist and `let _close = "*/";` after it (new) | valid; 3/3 facts at the reference site; **classified** | classification not recognised, allowlist fact not stated (blanked between the two strings; with the passing oracle: `needs review`) | passed, 3/3 |
| reference | valid; 3/3 facts; classified | same | passed, 3/3 |
| no change, right after the reference run | valid; cannot be assessed | - | compile_error |
| total omission | valid; hidden oracle at run time | same | test_failed, 0/3 |
| omission + `// TODO: admit /0/private/BalanceEx` in the allowlist | valid; classification absent, fact not stated; hidden oracle at run time | same (fixed in round 4) | test_failed, 0/3 |
| omission + the `Balance` test failing with `Local(SigningInput)` | valid; existing tests without evidence; hidden oracle at run time | same (fixed in round 4) | test_failed, 0/3 |
| omission + an unrelated failing test / an unrelated compile error | valid; hidden oracle at run time / cannot be assessed | same | test_failed / compile_error |
| sealed weakness controls `drops-rebase-multiplier` / `signature-strips-ex` | valid; 3/3 facts; classification present, the oracle fails | - | test_failed, 2/3 / 1/3 |
| notion/e4 valid variant | valid; classification fact and e4 row **needs review** | - | passed, 5/5 |
| nager-holidays/e1 reference + a historical comment | valid; 2/2 facts; missed propagation none, one review candidate; old path fact under `old_fact_only_in_comment_or_doc` | - | passed, 3/3 |

Without cargo (`<WS>/work/i4-runs/fix5-controls/`):

- `scanner_controls.py`: 36 Rust snippets, the reviewer's four probes and at least one per token
  class (a fact in a string next to comment-like text must be credited; a fact in each comment
  form must not): 36/36 as expected; the round-4 scanner read 10 of them wrongly, including all
  four reviewer probes.
- `audit_sites.py` (edit sites, classification check and propagation counts with their
  code/comment split, on the baseline, the reference and every sealed control of all 40 tasks,
  101 trees): **no reading changed** against round 4.
- `corpus_blanking.py` (all 1782 `.rs` files of the base tree): 61 files are blanked differently
  from round 4. Everything the current scan blanks and round 4 kept lies in a multi-line
  `#[doc = ..]` attribute or in one line comment after `b'"'`. The rest is code round 4 erased:
  786 lines, 653 of them whole lines inside a fake block comment that a `/*` in a string
  (`"text/*"`, `"*/*"`) opened, 97 with `//` in a raw string with inner quotes (JSON with
  `https://` URLs).
- `framing_controls.py`: a captured `failures:` inside a genuine block keeps the evidence
  (attributed); a captured fake header naming the operation in an unrelated failing test is not
  attributed; a duplicate header of another failed test, a forged `Running` line, a final list that
  disagrees with the run list, and output before the first header are each `cannot be assessed`
  (ambiguous framing); a `should panic` test frames normally; the four round-4 synthetic controls
  are unchanged; the 7 e4 omission logs of this validation give the recorded stage and texts, 7/7.
  Every cargo log kept from earlier runs (1112 logs, 2627 test target sections, 184 with failure
  headers): no framing problem; the blocks equal round 4's except cargo's `error: test failed`
  line, which round 4 merged with the next `Running` line (6 logs), and the two round-5 delimiter
  controls' logs.

### Fix round 4: edit-site matching, classification check, omission stage

`run.py --replay` / `--replay-shell` at base `2090802a`, 2026-11-08 01:16–01:17 (local; 00:16–00:17
UTC), bank `e7d963b9e837…`, measurement target `<WS>/work/targets/base`; result files in
`<WS>/work/i4-runs/<task>-replay-2026110801*-ctl4-*/out/`, the two new runner-control patches in
`<WS>/work/sealed/i4-bank/runner-controls/` (`kraken-e4-omission-plus-classification-comment`,
`kraken-e4-omission-plus-balance-test-signinginput`). *Round-3 rule*: the previous code evaluated
on the same tree (the patch applied to the base) and on the same run's cargo logs
(`<WS>/work/i4-runs/fix4-controls/old_rule_on_replays.json`; "-" = not evaluated). All on
kraken-spot-rest/e4 unless named.

| Control | Outcome (current rule) | Round-3 rule, same tree and logs | Oracle |
|---|---|---|---|
| omission + `// TODO: admit /0/private/BalanceEx` in the policy allowlist (new) | valid; classification check matches only in a comment → **absent** (`classification_check_only_in_comment_or_doc`); allowlist fact **not stated** (`new_fact_only_in_comment_or_doc_in: src/policy.rs`); caught by the hidden oracle at run time (`SigningInput` with `extended_balance` in `---- oracle_extended_balances_is_signed_and_decoded stdout ----`) | classification **present**, allowlist fact at the **reference site**, verdict "present but the oracle fails" (no omission stage) | test_failed, 0/3 |
| omission + the agent's own test of the existing `Balance` operation, failing with `Local(SigningInput)` (new) | valid; existing tests failed **without** evidence (`SigningInput` there, but in no failure naming the new operation: `evidence_outside_a_block_naming_the_operation`); caught by the hidden oracle at run time | **existing tests** | test_failed, 0/3 |
| reference | valid; 3/3 facts at the reference site; classified | same | passed, 3/3 |
| no change, right after the reference run | valid; classification absent: **cannot be assessed** (the caller's compile fails without the evidence) | - | compile_error |
| total omission (`omission.patch`) | valid; caught by the hidden oracle at run time | same | test_failed, 0/3 |
| omission + an unrelated failing test of the agent's own | valid; caught by the hidden oracle at run time; existing tests listed without evidence | same | test_failed, 0/3 |
| omission + an unrelated compile error in `src/trades.rs` | valid; **cannot be assessed** | same | compile_error |
| sealed weakness controls `drops-rebase-multiplier` / `signature-strips-ex` | valid; 3/3 facts; classification present, the oracle fails | - | test_failed, 2/3 / 1/3 |
| notion/e4 valid variant | valid; classification fact and e4 row **needs review** | - | passed, 5/5 |
| nager-holidays/e1 reference + a historical comment naming `GET /api/v4/Holidays/..` | valid; 2/2 facts; missed propagation none, one propagation review candidate; old path fact under `old_fact_only_in_comment_or_doc` | - | passed, 3/3 |

Without cargo (`<WS>/work/i4-runs/fix4-controls/`):

- `evidence_controls.py` (synthetic kraken-spot-rest/e4 logs as the failing existing-tests stage):
  a failure of a test of the existing `Balance` operation containing `SigningInput` → **cannot be
  assessed** (round-3 rule: existing tests); a failure naming `BalanceEx` with `SigningInput` →
  attributed to existing tests; `SigningInput` in a `Balance` test's block and `BalanceEx` in
  another test's block → **cannot be assessed** (round-3 rule: existing tests); a test named after
  the new operation whose output prints an `error:` line before `SigningInput` → attributed (the
  block is not split). The fix-3 logs of the 7 e4 omission controls → the recorded stage, 7/7.
- `audit_sites.py` (edit sites and classification check on the baseline, the reference and every
  sealed control of all 40 tasks, 101 trees): identical under both rules except strava/e5's
  baseline, whose rustdoc ("summary, detail and update responses") the round-3 rule credited with
  the new `ProblemField.detail` fact "elsewhere"; now `not stated`. No reference tree changed.

The tampering and isolation controls below do not depend on the changed rules and were not re-run.

### Fix round 3: isolation, measurement and tampering controls

`run.py --replay` / `--replay-shell` at base `2090802a`, 2026-11-08 00:49–00:51 (local), bank
`af768919e1da…` (round-3 site and evidence rules); result files in
`<WS>/work/i4-runs/<task>-replay-2026110800{49,50,51}*-ctl-*/out/`, the runner-control patches in
`<WS>/work/sealed/i4-bank/runner-controls/`. All on kraken-spot-rest/e4 unless named.

| Control | Outcome | Oracle (unscored when invalid) |
|---|---|---|
| reference | valid; 3/3 facts at the reference site; classified | passed, 3/3 tests executed |
| no change, right after the reference run (stale-build control) | valid; classification absent: **cannot be assessed** (the caller's compile fails because the endpoint is missing, not with the classification's evidence; round 2 reported "caught at the caller's compile") | compile_error |
| `.cargo/config.toml` no-op runner, no solution | **invalid** (`.cargo/config.toml`) | compile_error |
| reference + the same no-op runner | **invalid** | passed, 3/3 (the runner was not used) |
| reference + `.git/config` `core.fsmonitor`, `diff.external`, `core.hooksPath` pointing at sentinel-writing scripts | **invalid** (4 `.git` changes beyond normal) | passed; **no sentinel file written** |
| reference + `git add -A`, `git commit`, `git checkout -b` | valid | passed, 3/3 |
| sealed weakness control `drops-rebase-multiplier` | valid | test_failed, 2/3 |
| notion/e4 valid variant (behaviour-preserving, spelled unlike the reference) | valid; classification fact and e4 row **needs review** | passed, 5/5 |
| reference + a symbolic link at `tests/oracle_e4.rs` pointing at a sentinel file outside the run directory | **invalid** (symbolic link, not copied); **sentinel unchanged** (same SHA-256 and mtime) | passed, 3/3 (installed with `O_EXCL`) |
| reference + a test that plants that link while the existing tests run | valid; the planting test fails with `Read-only file system`; no link in the copy afterwards | passed, 3/3 |
| reference + a `build.rs` that rewrites `tests/oracle*` with a passing stub | **invalid** (build script added); its write failed (`Read-only file system`, cargo warning in the log); oracle **unchanged** (SHA-256 equal to the sealed source) | passed, 3/3 |
| reference + `transport()` writing forged libtest lines (the three oracle test names and a summary) to stdout and `process::exit(0)` | **invalid** (harness scan: libtest-shaped text, exit and stdout writes in library code) | **passed, 3/3**: the forged output satisfies the verdict; only the scan catches it |
| reference + a comment line in `examples/testserver/src/lib.rs` | **invalid** (measurement harness crate changed) | passed, 3/3 |
| sealed weakness control `signature-strips-ex` | valid | test_failed, 1/3 |
| total omission (`omission.patch`) | valid; caught by the hidden oracle at run time (evidence `SigningInput`) | test_failed |
| omission + an unrelated failing test of the agent's own | valid; caught by the hidden oracle at run time; existing tests listed as failed **without** classification evidence (the previous rule would have said "existing tests": read from its code, not re-run) | test_failed |
| omission + an unrelated compile error in `src/trades.rs` | valid; **cannot be assessed** (SDK build and the caller's compile failed without classification evidence; the previous rule would have said "compiler (SDK build)": read from its code) | compile_error |
| nager-holidays/e1 reference + a historical comment naming `GET /api/v4/Holidays/..` in `src/lib.rs` | valid; 2/2 facts; missed propagation **none**, one propagation review candidate; the old path fact listed under `old_fact_only_in_comment_or_doc` (the previous rule would have counted one missed propagation and the fact as not stated: read from its code) | passed, 3/3 |

The previous measurement method (host `git add -N .` and `git diff` in the agent's tree),
re-enacted in a scratch directory with the same fsmonitor and external-diff settings, wrote both
sentinel files. Before the fresh-mtime copy, the no-op-runner control without a solution, run
right after the reference replay, scored the oracle 3/3 from the reference's build (runs kept in
`<WS>/work/i4-runs/superseded-20261108/`). The round-2 replays of the first eight controls (bank
`ca09313218be…`, previous runner) stay in `<WS>/work/i4-runs/*-replay-2026110800[01]*`.

## Smoke runs

Smoke records (agent patches, logs, rollouts with the account identifiers of `session_meta`
withheld) are kept with the sealed bank in `<WS>/work/sealed/i4-bank/smoke/`, not here.

- `codex.kraken-spot-rest.e4.fix3/` (2026-11-08 local / 2026-11-07 23:51–23:54 UTC, base
  `2090802a`, this runner, bank `af768919e1da…`, the task text with its *Limits of this change*):
  requested `gpt-6.1-sol` / `xhigh` / `danger-full-access` inside bubblewrap (owner
  `service_tier` `"priority"`); observed (rollout `turn_context`) `gpt-6.1-sol` / `xhigh` /
  `danger-full-access`. Isolation check passed before launch; the agent ran 2 min 46 s; oracle
  passed with 3/3 tests executed (installed with `O_EXCL`, unchanged after the runs), existing
  tests (with the agent's own) 54 passed / 0 failed; 3/3 facts at the reference site;
  classification classified; no missed propagation; no `.git` change, no `.cargo` file, no link
  or special file, no build-time code or harness change, no harness-scan hit, no exposure hit
  (status `valid`); 14 hunks in 8 files (diagnostic). One repetition: a smoke test, not a
  measurement.
- `codex.kraken-spot-rest.e4.sealed-runner/` (2026-11-08 local / 2026-11-07 23:14–23:17 UTC, base
  `2090802a`, the round-2 runner, bank `ca09313218be…`): requested `gpt-6.1-sol` / `xhigh` /
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
- Diagnostic attribution (edit sites, classification row, detecting stage) is lexical; these edge
  cases are known and not handled (the oracle verdict does not depend on any of them):
  - *Code only* means "not in a comment or doc attribute", not "effective": a fact in a string
    literal that is never used (`let _unused = "/0/private/BalanceEx";`) is credited.
  - The comment scan knows `#[doc ..]` / `#![doc ..]` only: `#[cfg_attr(.., doc = "..")]`, a `doc`
    attribute written with whitespace between `#` and `[`, and documentation generated by a macro
    from its arguments count as code. Unterminated literals or comments (invalid Rust) run to the
    end of the file. Non-Rust files (`Cargo.toml`, fixtures) have no comment detection.
  - The harness-forging scan (`run.harness_scan`) still classifies comment lines line by line
    (`run.comment_flags`), separately from this scan: a line of a multi-line string that starts
    with `//` or `*` counts as a comment there (`needs review` instead of `invalid` for
    libtest-shaped text on it).
  - Failure framing trusts what libtest writes outside a test's captured output. Output that
    bypasses the capture (a child process, a direct write to file descriptor 1 or 2) can forge a
    complete, self-consistent target section (run list, both `failures:` lines, summary) that is
    not told apart from cargo's own; inconsistent forgeries are ambiguous (`cannot be assessed`).
  - A rustc diagnostic is trusted by its shape only: code the agent writes can put chosen text into
    one (`compile_error!(..)`, names, quoted source lines), so the SDK-build stage can be credited
    on text the agent chose.
  - Evidence is lexical: a failed test named after the new operation whose output shows the failure
    kind for another reason is attributed; other captured text naming the operation counts as
    naming it (only header-shaped lines are dropped); a failed test with no captured output has no
    block. Only libtest's human format is framed (the runner never passes `--nocapture` or a JSON
    format).
- The agent's library code and tests run during the measurement, confined to the measurement
  sandbox without network and with the sources read-only; build scripts, proc macros, `.cargo`
  runners, toolchain overrides and a changed or redirected harness crate make the session invalid.
  A library that forges the oracle's verdict is caught when the static scan recognises it
  (libtest-shaped text, exit/abort, stdout writes, pre-`main` code); deliberately obfuscated
  forging is not (*Residual risk* above).
- One repetition per task and provider is a smoke test, not a measurement (A1 §3: ≥3 fresh
  repetitions where a decision rests on them).
