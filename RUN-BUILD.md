# The build of Concord V5: what was done, what holds, what is yours

13 November 2026. You asked for Concord V5 built in code by agents, to the level of a release, and not published (D267, D268). This page is the report. The plan was `PLAN-BUILD.md`. The live notes are in `build/` (`STATE.md`, `ANSWERS.md`, every brief and every report).

**Where it is:** the branch `build` of this repository, in `api/`. Nothing was pushed. Nothing was uploaded. No real API was called.

## 1. What exists

Five crates, version `5.0.0-rc.1`, each with `publish = false`:

| Crate | What it is | Lines of source |
|---|---|---:|
| `concord` | the facade a user depends on | 294 |
| `concord-core` | the engine: build a request, send, try again, read, pages, errors | 17,615 |
| `concord-macros` | `concord::api!`: parse, names, resolve, check, flat list, write | 8,268 |
| `concord-extras` | json, form, multipart, the readers, the pagers, the `tower` adapter | 2,978 |
| `concord-testkit` | a local server and a scripted transport with virtual time, for tests | 2,409 |

Beside them, never published: `tour` (the guide's five clients written by hand: the model), `tour-gen` (the same five as descriptions: the macro must write the same thing), `conformance` and `conformance-gen` (the tests of the contracts, run against both), `real-gen` (the eleven real API descriptions of `docs/apis/`: ten compile; Telegram's first page is kept as a failing case, and the page is the one that is wrong).

## 2. What holds, and how it is known

One command, `just release --with-sockets` from `api/`, runs everything below and stops at the first failure. The result of its last run is in section 7.

| Check | What it shows |
|---|---|
| `just gate` | format, check, clippy with warnings denied, all tests, doc tests, docs with warnings denied, Rust 1.96 and stable, features, no `todo!()` |
| the contracts | 99 contracts hold; 93 have a test named with their number; 6 have none yet (`build/MATRIX.md` lists them) |
| the same tests on both | every test file of `tour` and `conformance` also runs on the clients that the macro wrote |
| `just panics` | 154 places that could panic in the three library crates, each with its reason at the place; clippy denies the rest |
| `just package` | each of the five crates builds from its own package, outside the workspace, with default features, none, and all |
| `just features-all` | 554 checks: every one of the 256 sets of features, each crate alone, stable and 1.96 |
| `just mutants` | a catalogue of 147 small wrong changes of the engine: each one must make a test fail |
| `just bench` | a plain `GET` through `api!` (two path inputs, three query inputs) adds 1.07 microseconds and 23 allocations over the same request built by hand. Budget set before measuring: 2 and 25 |

Compile time through the real macro, measured: 100 endpoints check in 2.5 s; 500 in 12.8 s with 1.06 GiB; a debug build of 500 takes 18.3 s and 1.74 GiB.

## 3. Who did what

You set the teams (D268). I wrote no production code and no test: briefs, decisions, gates, merges. Three exceptions, each in its commit message: merge conflicts (three, both sides kept), two type annotations in a test that followed a decision (A62), and the removal of an `#[ignore]` when the engine caught up with an answer (three times).

- **Writers:** Sol 6.1 `xhigh` (the engine and the macro, two Codex threads resumed package after package), Opus 5.5 `high` (the tests from the paper, the test kit, `tour`, the pages, the packaging, the rustdoc).
- **Readers:** Astra `max` (probes and mutations against the real libraries), Opus 5.5 `xhigh` (the other provider than Sol: the review of a maintainer), Fable 5.2 `high` (the paper against the code, sentence by sentence), Sol 6.1 `max` (the stranger of section 5).
- Astra and Sol are one provider. A reading of Sol's code by Astra is not counted as the other provider; each package of Sol was also read by Opus or Fable.
- Model and effort of every lane, as the harnesses recorded them: `build/LANES.md` (made by `build/lanes.py`).

The readings of the last day, each fixed or decided afterwards:

| What was read | By | Faults |
|---|---|---:|
| Step 5 of the macro | Astra `max` | 6 |
| Step 5 of the macro, as an author | Opus `xhigh` | 14 |
| The engine at the end of M2 | Astra `max` | 7 |
| The engine at the end of M2 | Opus `xhigh` | 11 |
| The fixes of those 18 | Opus `xhigh` | 14 fixed, then the rest |
| The whole macro against the whole paper | Fable `high` | 17 |
| A user who gets things wrong, a server that misbehaves (75 cases) | Opus `xhigh` | 4, all wording; no leak, no panic of ours, no hang |

## 4. What changed in the design while building

The paper was right nearly everywhere. Where it was silent or wrong I decided, and the pages of `docs/` now say what was built.

- **Five decisions on the frozen surface** (`DECISIONS.md`, D269 to D273): `not_applied_on` never panics; `<client>::Retry`; an arm on a non-2xx whose body cannot be read is a refusal; a failed request with no head is "no connection"; `ByteStream::try_next` gives `concord::read::StreamError`.
- **77 answers** on details (`build/ANSWERS.md`, A1 to A77): exact texts of errors, edges of encoding, what is reserved in memory, what a help may say. The ones you may want to look at: A59 and A72 (a server never decides how much memory is reserved), A60 (one copy of the call methods), A62 (a body writer of the author uses the same public door as `json`), A64 (names that begin with `__` are Concord's), A68 (what is fixed in a message), A75 (a head that cannot be read is not "no connection"), A77 (Concord runs on Tokio: the one documented panic).
- New public names that the paper did not have: `concord::body::Bounded`, `concord::body::WriteError`, `concord::read::details`, `concord::page::fragmentless`, `concord::read::StreamError`, `concord::sample` (docs only).

## 5. The stranger's test

The plan asked: can someone write a client from the rustdoc alone? Two runs, each with a made-up API, no access to `src/`, `tests/` or `docs/`.

1. **Sol `max`, before the docs were fixed:** nine points done, 19 tests, in 15 minutes and 9 compile runs. It listed 14 places where it had to guess. The largest: the page of `api!` held two sentences and no grammar.
2. Then the docs were written from that list: a reference of the language on the page of `concord::api!` (about 500 lines, every example compiled as a doc test), a full example `bookshop`, a sample generated client in the docs (`concord::sample`), the boundaries in one place.
3. **Fable `high`, after:** ten points of another API, 23 tests. The description compiled on the first try, and so did the tests. It still listed 12 smaller guesses; those were closed too (multipart, a worked `Retry` example, the order on the wire).

Names both strangers looked for and did not find. I added none: each is your call.

- `bearer(token)` beside `basic` (today: the template `"Bearer {token}"`).
- A fixed wait on `Retry` without writing a `when` rule.
- `Tried::method()` to say "only GET" inside a rule.
- A ready-made middleware that only observes requests.
- A reader for an empty answer (today: no arrow, or an arm `204 => ()`).

## 6. What is yours now

1. **The editor.** Open a file with `concord::api! { … }` in your editor: completion, go-to-definition and errors under the right tokens. No agent can see that.
2. **A real call with your key.** `api/real-gen` holds Riot and the others as descriptions that compile. Nothing here ever called a real server.
3. **Before any upload:** the licence (`MIT OR Apache-2.0` is the usual choice; no `license` line is set), the `repository` line, whether the crate names are free, and `publish = false` in `api/Cargo.toml` and in each crate that sets it itself.
4. **The branch.** `main` is still the paper as it was before the build. `build` holds everything. Moving `main` is a merge that I did not do: say the word.
5. **The size of the history.** By my mistake three late commits tracked the build output of the small bench crate (`api/tools/measure/target`, 640 MB). It is untracked and ignored now, but those commits still hold it, so `.git` is 400 MB. Removing it means rewriting those commits of `build`: your call.
6. **The six contracts with no named test** (`build/MATRIX.md`) and the ideas of section 5.

## 7. The last run

`just release --with-sockets`, from `api/`, on the branch `build` at commit `9a56e0b`, 13 November 2026, 11:48Z. From `build/raw/final-release.log`:

```text
     Summary [  43.706s] 1587 tests run: 1587 passed, 1 skipped
contracts: 99 that hold, 93 with a test, 6 with none
RELEASE gate: passed
PANICS: 154 sites; 0 without reason
RELEASE panics: passed
RELEASE package: passed
FEATURES: 554 checks; 0 failed
RELEASE features-all: passed
mutants: 147, caught: 147, survived: 0, stale: 0, socket-only: 0
RELEASE mutants: passed
RELEASE: passed
```

The one skipped test is a measure that runs only under `just bench`. The commits after `9a56e0b` change only this report, two sentences of the pages, and the notes in `build/`.

## 8. One incident

Around 04:10Z the system killed the Claude app three times for memory. Cause: every build of every lane ran inside the app's own group, with 20 jobs each and no ceiling; five lanes reached 26 GB of 30. Since then every lane runs through `build/limited.sh`: its own group, 8 GB each, 18 GB for all, 4 jobs. No kill after that. The machine also restarted once at 10:31Z; nothing was lost, since every lane's work was committed before.
