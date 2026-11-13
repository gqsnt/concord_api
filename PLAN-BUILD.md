# Plan for the build: Concord V5 in code, at the level of a release

2026-11-13 · written by Claude, as architect · it follows your word (D267): "reflect on the plan to implement concord v5 completly in code. you wont implement/code it yourself, you define architect / senior / system engineer part, they are your coding dev/subagents, a final version that is publishable (dont publish)". **You then set the teams (D268):** two main writers, Sol `xhigh` and Opus `high`, and one more of either when it helps; four readers, Fable `high`, Astra `max`, Sol `max`, Opus `xhigh`. **The build started on your word, 2026-11-13.**

## 1. The goal, and what "publishable" means

Every statement of the language, every public item of the surface, every contract and every message, in code, tested, documented, packaged. Not uploaded.

I will call it done when all of these are **observed by me, with commands**, not reported by an agent:

| | Done means |
|---|---|
| **Complete** | No `todo!()`. The macro is written. The 12 statements, the declaration, the 46 messages at compile time and the 25 texts at run time exist in code. |
| **Correct** | Each of the 99 contracts has a test that carries its number, or a written reason why no test can show it. The 11 real descriptions compile through the macro. The clients of `api/tour`, written with `api!`, pass the same tests as the ones written by hand. |
| **Safe** | No `unsafe`. The library never panics on any input or any answer (lints forbid it; property tests attack every parser and writer). No secret is printed by anything. No redirect is followed; origins are pinned. |
| **Clean** | `cargo fmt`, `clippy` and `rustdoc` with warnings as errors. Every public item has a doc comment with an example that compiles. |
| **Portable, as far as this machine shows** | Builds and tests on Rust 1.96 (the oldest version installed here that builds it today; 1.85 does not, because of a dependency) and on 1.99. Every combination of features builds. Linux only: Windows is "not yet", by your word. |
| **Measured** | Compile time of 500 endpoints through the real macro. Time and allocations of one call. Written down, against a budget set at the start. |
| **Packaged** | Each of the four crates builds from its own package (`cargo package`), with README, CHANGELOG and metadata. Nothing is uploaded. |
| **Read** | Every package of code was read by an agent of the other family. At the end, a fresh agent writes a client from the `rustdoc` alone. |

**Not in this build:** the list "can come later" of chapter 8; keys in another case (D266); Windows; a call to a real API; the upload itself.

## 2. What the build starts from

Observed today, in `api/`:

- `concord-core`: 6,361 lines, **72 bodies empty**. Five pieces run (the decision to try again, the chain of middlewares, `Http`, the comparison of two requests, the values of a body).
- `concord-extras`: 3,011 lines, real code. `concord`: the facade. `concord-macros`: **a stub of 26 lines**.
- `api/tour`: 5,958 lines, 17 endpoints written by hand as the macro would write them.
- 229 tests pass. No warning.
- **Everything the build needs is already on this machine**: `syn`, `quote`, `trybuild` (tests of compile errors), `hyper` and `axum` (a local server), `proptest`, `criterion`, `tracing`, `prettyplease`; `clippy`, `miri`, `nextest`, `sccache`; Rust 1.96, 1.99 and nightly. So no download is needed.
- 20 cores, 30 GB, 723 GB free: five lanes can compile side by side.
- From before: an older macro in `proto/` (2,135 lines, for the design of 11 November) and a parser written from the grammar alone as a probe (3,927 lines, with 180 attack inputs). They are material to read, not a base to trust.

## 3. What I learned, and the five ideas of this plan

The last run showed three things about these agents. A worker does not see its own faults: the reader of the other family found something in six of seven readings. Reading the same thing again does not always converge: for `Base`, a list of what to refuse never ended, and a round trip ended it at once. And **the hidden interface between generated code and the core did not converge on paper at all**: four readings, four new blocking questions.

So the plan rests on five ideas.

1. **Build the hidden interface from both sides first, by hand.** Before the macro exists, the engine is written until the clients of `api/tour` (already written by hand) run for real against a local server. What they call is then proven by code, not by a page.
2. **The hand-written clients become the model that the macro must reproduce.** The macro's last step has one test that cannot be argued with: the same description through `api!` passes the same test files as the client written by hand.
3. **Tests come from the paper, and from another lane than the code.** The surface already compiles, so tests of the contracts can be written today, before the bodies exist. The developer who writes a body may not change an expected value: on a conflict they stop and report, and I decide.
4. **Machines before opinions.** A gate is a list of commands that I run. A model's "all green" is a claim. Where a rule can be a property (a round trip, "never panics", "prints no secret"), it is tested as a property over many generated inputs, not reviewed line by line.
5. **Two rounds of fixes, then change the approach.** A third patch of the same kind is the sign that the design of that piece is wrong.

## 4. Who does what

| Role | Who | What |
|---|---|---|
| Architect | Claude (me) | I cut the work into packages and write each brief: the files it owns, the signatures it must keep, the tests that must turn green, what it must not touch. I decide when code and paper disagree. I run every gate. I merge and commit (a Codex agent cannot). **I write no code and no test.** |
| Writer | **Sol 6.1, `xhigh`** (Codex) | A main writer. The engine and the macro: the worker that I have measured. |
| Writer | **Opus 5.5, `high`** (Claude agent) | A main writer. The tests from the paper, the local server for tests, the messages, the docs and the examples; code too, once M0 has shown how it codes. |
| Writer | One more Sol or Opus | Only when a package can run beside the two others with its own files. |
| Reader | **Astra `max`** | Breaks real code with probes against the real libraries. |
| Reader | **Sol `max`** | Asks the compiler: the macro, types, Rust versions, features. |
| Reader | **Opus `xhigh`** | The side of who writes a description and who calls a client: the feel, the docs, the messages. |
| Reader | **Fable `high`** | The paper against the code, exactly: contracts, cases, counts. |

Three writers at most and four readers at most at one time (I read "we should have 4 reader" so; tell me if you meant four kinds and two at a time).

**Who reads what.** Every package is read by at least one model of the other provider than its writer: what Sol wrote, by Opus or Fable; what Opus wrote, by Astra or Sol. Code that matters gets a second reader, chosen for what it is good at. At the end of a milestone, all four read. A reader is read-only and works with its own probes outside the project.

**One thing I do not know yet:** how Opus codes. I measured it on reading and writing pages. So in M0 it writes one real package of code (the server for tests), a reader attacks it, and I assign the rest after that.

**A word on words.** In the last run I wrote that "the other provider" read Sol's code. It was Astra: another model, the same provider. The pages are corrected.

### The path of one package

`brief` (me) → `tests from the paper` (another writer than the code's; read by a reader) → `code` → `gate` (I run it) → `reading` (the other provider, read-only) → `fix` (same thread, two rounds at most) → `merge` (me) → `the table of contracts is made again from the tests`.

### The three things that everything is checked against

| | What it is | Where |
|---|---|---|
| **The conformance suite** | One test or more for each contract, named with its number (`c4_3_a_wait_over_60_s_means_no_other_try`). It runs against the public surface and a local server. | a crate for tests only, not published |
| **The model of the expansion** | `api/tour` by hand, once it passes the suite on the real engine. | `api/tour` |
| **The catalogue of messages** | 46 tests of compile errors with the exact three lines of [messages](docs/messages.md), and a test that compares the macro's own table with that page, so that the two cannot drift. | `concord-macros`, with `trybuild` |

## 5. How the code is cut, so that lanes do not collide

**The engine.** Today most of it is one file (`__private.rs`, 1,762 lines, 35 empty bodies). The first package splits it, with no change of behaviour, into parts that one lane each can own:

| Part | It holds | Contracts |
|---|---|---|
| `build` | The request being built: base, path, query, headers, body; how a value is written; the marks of a secret; Build errors | C2 |
| `run` | The clocks, the tries, the waits, the chain to the transport | C3, C4, C7, C9 |
| `answer` | Reading a body with its limit, a refusal, the arms' helper, the error and its 25 texts | C5, C6 |
| `page` | The machine of pages, the items, `at_url` | C8 |

**The macro.** One crate (you fixed four crates; I add none), in the five steps of [the macro page](docs/macro.md): `parse` → `check` → `resolve` → `lower` → the messages beside all of them. The first three know nothing of the engine, so they are written **at the same time as the engine**. Only `lower` waits for the model.

**The server for tests.** A small crate, not published: a local HTTP server that a test scripts (this status, these headers, this delay, a body that breaks, silence) and that records every request it gets. Time in tests is paused or faked: **no test sleeps**.

## 6. The milestones

| | What | It is passed when (observed by me) |
|---|---|---|
| **M0. Ground** | The split of the engine's file. The server for tests. The gate as one command. The script that makes the table of contracts from the tests. The first tests (C2), by both models: the calibration. | The gate is green on code that does nothing new. I have one comparable package from each model. |
| **M1. One call, end to end, by hand** | `build`, `run`, `answer`, and the two hidden halves of a send joined. | The clients of `tour` that have no pages pass the suite against the local server. Every one of the 25 texts is produced by code. |
| **M2. Pages, and the rest of a call** | The machine of pages; `send_raw`, `read_response`, `send_if_none_match`; the one log event; the `tower` adapter. | All 17 endpoints pass. **No `todo!()` is left in the core.** Each run-time contract has its test or its reason. The hidden interface is now code: `tour` by hand is the model. |
| **M3. The macro, first half** (beside M1 and M2) | `parse`, `check`, `resolve`, and the 46 messages. | Every description of the paper (41), the 11 real ones and the 180 attack inputs give what the paper says: accepted, or the right message with its exact words. |
| **M4. The macro, second half** | `lower`: types, handles, calls, setters, the request, the answer, pages, the generated docs. | `tour` through `api!` passes the same test files as `tour` by hand. The 11 real descriptions compile. A wrong use that must not compile does not, with Concord's words. |
| **M5. Hardening** | Property tests; the "never panics" audit; features and Rust versions; the measures; the docs, examples, README, CHANGELOG; packaging; the states in `docs/contracts.md` made from the tests. | Every line of section 1 but the last. |
| **M6. Release candidate** | A fresh agent writes a client for a real API from the `rustdoc` alone. Another tries to misuse the library. Fixes. The last gate. | Section 1, whole. Then I hand you the list of section 9. |

M3 runs beside M1 and M2. That is where three writers pay: one on the engine, one on the macro, one on the proof.

## 7. The rules

1. One owner for a file at a time. A lane works in its own copy of the tree (a git worktree) on its own branch; build output is beside the project, never in `/tmp`.
2. An expected value is never changed to pass. A test and the paper disagree: the paper wins. The paper and Rust disagree: section 8.
3. Nobody certifies their own work. No answer, or a timeout, is not approval.
4. A new dependency only from what is on this machine. Anything else needs your word first.
5. For every lane, the model and the effort that were asked for and the ones that its log shows, in a table made by a script, as in the last run.
6. Restart-safe: each lane has its brief, its branch and its thread id on disk, and one file says the exact next action. After a restart I check the real state first.
7. Nothing is pushed. Nothing is published. No real API is called.
8. A milestone has a ceiling of time (section 10). At the ceiling I commit a checkpoint and report; I do not call it done.

## 8. When code and paper disagree

The line of the last run decides.

| The disagreement is in | What I do |
|---|---|
| The hidden interface (`__private`, the shape of generated code) | I change it, and bring the two pages in line. Expected: this layer was never frozen. |
| The wording of a message | I change it inside its three rules. |
| The public surface, a contract, a promise | A decision with its number and its reason. Listed for you at the end of the milestone. |
| The language (a form, a rule) | I stop that package and ask you. It is yours. |

## 9. What only you can do

- **The editor.** What completion and "go to definition" do inside `api! { }`. No agent can see it.
- **A real call**, with a key of yours (Riot is your ground). The build calls nothing real.
- **Before any upload:** the names of the crates, the licence, the repository, the version number. I leave these fields for you, and I do not guess them.
- Windows, when you want it.

## 10. Time, and what could go wrong

About 38 packages. In the last run a worker's pass took 2 to 10 minutes and a reading 10 to 15, but those were small; a package of the engine or of the macro is larger. **My estimate, inferred and not measured: 15 to 25 hours of lanes running, in five to eight sittings.** Ceilings: M0 2 h, M1 5 h, M2 3 h, M3 4 h (beside M1 and M2), M4 6 h, M5 4 h, M6 3 h. M0 will tell how good the estimate is, and I will correct it then.

| Risk | What the plan does |
|---|---|
| The hidden interface moves again | It is built by hand before the macro (M1, M2), and the macro copies what works. |
| The paper says something that Rust or `reqwest` refuses | Section 8. The last run found several (a signature that did not compile, a helper that Rust's coherence refused). |
| Opus codes less well than I assume | M0 shows it on one package. Then it keeps to the proof and the docs, and Sol gets a second lane. |
| A review that never ends | Two rounds, then another approach. |
| `lower` is larger than I think | It is the one step with a model to copy, and it is cut by kind (types, request, answer, pages, docs). |
| The compile time of a large client | Measured in M4 on 500 endpoints, not left to the end. |
| The machine restarts | Rule 6. |
| Cost | Up to seven lanes for many hours is a lot of tokens. I report what each milestone took. |

## 11. What you answered

"i agree with your 3 write / 2 reader, but we should have 4 reader": the teams of section 4. The three things of section 9 you will do when every milestone is passed. I read this as your go, and started M0. The state of the build, at any moment, is in `build/STATE.md`.
