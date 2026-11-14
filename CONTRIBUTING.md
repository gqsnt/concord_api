# Contributing to Concord

For someone who wants to change Concord: what you need, the one command that checks everything, how the tests are organised, and how a release is made.

## What you need

- Rust stable, and Rust 1.96 (`rustup toolchain install 1.96`): the oldest version that Concord supports is checked by every gate.
- [`just`](https://github.com/casey/just), to run the checks.
- [`cargo-nextest`](https://nexte.st), which runs the tests.
- Python 3.11 or later: the checks behind `just` are small scripts in `tools/`.

Every check is offline (`cargo --offline`), so fetch the dependencies once first: `cargo fetch`, and `cargo fetch --manifest-path tools/measure/Cargo.toml` for `just bench`. Some tests open a socket on `127.0.0.1`.

## The one command

```sh
just gate
```

It runs, in order, and stops at the first failure:

| Step | What it checks |
|---|---|
| `props` | The property tests of `concord-core` and `concord-extras` (256 cases each; `PROPTEST_CASES=4096 just props` for more). |
| `fmt` | `cargo fmt --check`. |
| `check` | The workspace and every target, with and without the features `tower` and `multipart`. |
| `lint` | Clippy on every target; a warning is an error. |
| `test` | Every test, with `cargo nextest`, then the doc tests. |
| `doc` | The documentation builds. |
| `msrv` | The workspace builds on Rust 1.96. |
| `features` | `concord-api` with no default feature, then with each feature alone, then with all. |
| `todo` | Counts the `todo!()` left in each crate (Clippy refuses them). |
| `matrix` | Lists the tests of each contract, by the number in their names, in `target/contracts-matrix.md`. It only lists: a contract with no test does not stop the gate. `just matrix --strict` fails on a contract with neither a test nor a reason in [`tools/NO-TEST.md`](tools/NO-TEST.md). |
| `links` | Every link between the Markdown files leads to a file that exists, and every link to a heading leads to a heading of that page (`tools/links.py`). A missing file or a missing heading fails. |

Each step is also a recipe of its own: `just test`, `just lint`, `just matrix`, `just links`. `just` alone lists every recipe.

Where local sockets cannot be opened, `CONCORD_SKIP_SOCKETS=1 just gate` skips the tests that need one, and prints how many it skipped.

## The slower checks

| Command | What it does |
|---|---|
| `just release` | Everything a release needs, in order: `gate`, `docs-audit`, `doc-examples`, `panics`, `package`, `features-all`, `mutants`. |
| `just mutants` | Puts known bugs into the engine, one at a time, and checks that a test catches each one. |
| `just package` | Each published crate as a user receives it: what its package holds, and that it builds alone, outside this workspace. |
| `just features-all` | Every subset of the features of `concord-api`, on stable and on 1.96. |
| `just panics` | Lists every place in the library code that could panic; each one needs a `PANIC-SAFETY` comment that says why it cannot. |
| `just docs-audit` | The documentation of `concord-api`, `concord-core`, `concord-extras` and `concord-testkit`, built with strict `rustdoc`, and a list of doc comments that only repeat a name. |
| `just doc-examples` | The whole descriptions of the guide, of the reference overview, of this repository's `README.md` and of the example pages are the same as their copies in `tests/real-apis`, and those compile. `just test` runs it too. To refresh the copies, `python3 -B tools/doc_examples.py` writes candidates under `target/doc-examples/`; review them and copy them to the paths shown there. |
| `just bench` | Opt-in measurements: the time and the allocations of a call. |

The tests of the tools themselves have no recipe; run them by hand: `python3 -B -m unittest discover -s tools`.

## The layout

```text
concord-api/      the crate a user depends on (library name `concord`): the facade, and examples/
concord-core/     the engine and the traits
concord-macros/   api!; its documentation is src/reference.md
concord-extras/   json, form, multipart, the readers, the pagers, basic, tower
concord-testkit/  for tests: a scripted local server, a transport with virtual time
tests/            workspace crates that are never published (see tests/README.md)
tools/            the scripts behind `just`
docs/             the guide, the reference, real APIs, internals, design
```

## How the tests are organised

**A contract has a number.** [The contracts](docs/reference/contracts.md) say what the engine and the generated code must do, line by line: `C5.4` is the line about a body read whole and its limit. A test that holds a contract carries its number at the start of its name: `c5_4_an_announced_size_over_the_limit_is_a_mismatch_before_anything_is_read`. `just matrix` lists the tests of each contract.

**The same tests run twice.** The test clients (`Tasks` and `Riot` of [the tour](docs/guide/tour.md), and `Files`, `Feed`, `Shapes`) exist twice: written by hand, as the macro writes them, in `tests/tour`, and written by the macro in `tests/tour-gen`. The test files live in `tests/tour/tests` and `tests/conformance/tests`; each `-gen` crate includes the same files with `#[path = …]`, so one test runs on both clients. A difference between the macro and the hand-written model is a failing test. [`tests/README.md`](tests/README.md) says what each crate holds.

**To add an endpoint to a tour client:**

1. Add it to the description in `tests/tour-gen/src/<client>.rs`.
2. Add the same lines to the description that stands, as a comment, at the top of `tests/tour/src/<client>.rs`. A test (`c1_20_…`) compares the two.
3. In `tests/tour/src/<client>.rs`, write by hand the code that the macro writes for it, as the endpoints beside it are written.
4. Write its test once, in `tests/tour/tests` or `tests/conformance/tests`. It runs on both sides.

## The rules of the code

The lints of the workspace hold them, and `just gate` fails on any of them:

- No `unsafe` (`unsafe_code = "forbid"`).
- No `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, `dbg!`, no printing, and no indexing that can panic, outside tests (`clippy.toml` allows them in tests).
- Every public item is documented (`missing_docs = "deny"`).
- A message of the macro is part of the interface: the 46 messages of [the messages](docs/reference/messages.md) are compared with the macro, word for word.

## How a release is made

The five published crates share one version, in `[workspace.package]` of the root `Cargo.toml`, and pin each other with an exact version (`=5.0.0-rc.1`). The switch is `publish = false`: today every crate says it, through `[workspace.package]`, so nothing can be uploaded by mistake.

1. Set the new version in `[workspace.package]` and in every `=` pin between the crates; add its entry to [CHANGELOG.md](CHANGELOG.md).
2. Run `just release`. It must pass whole.
3. Remove `publish = false`, then publish in the order of the dependencies: `concord-core`, `concord-macros`, `concord-extras`, `concord-api`, `concord-testkit`.

## Licence

A contribution is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at the user's option, like the rest of Concord.
