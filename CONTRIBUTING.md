# Contributing to Concord

This page is for anyone who wants to change Concord. It covers what you need, the one command that checks everything, how the tests are organised, and how a release is made.

## What you need

- Rust stable, and Rust 1.96 (`rustup toolchain install 1.96`): every gate checks the oldest version that Concord supports.
- [`just`](https://github.com/casey/just), to run the checks.
- [`cargo-nextest`](https://nexte.st), which runs the tests.
- Python 3.11 or later: the checks behind `just` are small scripts in `tools/`.

Every check is offline (`cargo --offline`), so fetch the dependencies once before you start: `cargo fetch`, and `cargo fetch --manifest-path tools/measure/Cargo.toml` for `just bench`. Some tests open a socket on `127.0.0.1`.

## The one command

```sh
just gate
```

It runs these steps in order and stops at the first failure:

| Step | What it checks |
|---|---|
| `props` | The property tests of `concord-core` and `concord-extras` (256 cases each; `PROPTEST_CASES=4096 just props` for more). |
| `fmt` | `cargo fmt --check`. |
| `check` | The workspace and every target, with and without the `tower` and `multipart` features. |
| `lint` | Clippy on every target; a warning is an error. |
| `test` | Every test, with `cargo nextest`, then the doc tests. |
| `doc` | The documentation builds. |
| `msrv` | The workspace builds on Rust 1.96. |
| `features` | `concord-api` with no default features, then with each feature alone, then with all of them. |
| `todo` | Counts the `todo!()` left in each crate (Clippy rejects them). |
| `matrix` | Lists the tests of each contract, found by the number in their names, in `target/contracts-matrix.md`. It only reports: a contract with no test does not stop the gate. `just matrix --strict` fails on a contract with neither a test nor a reason in [`tools/NO-TEST.md`](tools/NO-TEST.md). |
| `links` | Every link between the Markdown files points to a file that exists, and every link to a heading points to a heading on that page (`tools/links.py`). A missing file or a missing heading fails. |

Each step is also a recipe of its own: `just test`, `just lint`, `just matrix`, `just links`. `just` alone lists every recipe.

Where local sockets cannot be opened, `CONCORD_SKIP_SOCKETS=1 just gate` skips the tests that need one, and prints how many it skipped.

## The slower checks

| Command | What it does |
|---|---|
| `just release` | Everything a release needs, in order: `gate`, `docs-audit`, `doc-examples`, `panics`, `package`, `features-all`, `mutants`. |
| `just mutants` | Puts known bugs into the engine, one at a time, and checks that a test catches each one. |
| `just package` | Checks each published crate as a user receives it: what its package contains, and that it builds on its own, outside this workspace. |
| `just features-all` | Checks every subset of the features of `concord-api`, on stable and on 1.96. |
| `just panics` | Lists every place in the library code that could panic; each one needs a `PANIC-SAFETY` comment that explains why it cannot. |
| `just docs-audit` | Builds the documentation of `concord-api`, `concord-core`, `concord-extras` and `concord-testkit` with strict `rustdoc`, and lists the doc comments that only repeat a name. |
| `just doc-examples` | Checks that the complete descriptions in the guide, the reference overview, this repository's `README.md` and the example pages match their copies in `tests/real-apis`, and that those copies compile. `just test` runs it too. To refresh the copies, `python3 -B tools/doc_examples.py` writes candidates under `target/doc-examples/`; review them and copy them to the paths shown there. |
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

**A contract has a number.** [The contracts](docs/reference/contracts.md) say what the engine and the generated code must do, line by line: `C5.4` is the line about a body read whole and its limit. A test that checks a contract starts its name with the contract's number: `c5_4_an_announced_size_over_the_limit_is_a_mismatch_before_anything_is_read`. `just matrix` lists the tests of each contract.

**The same tests run twice.** The test clients (`Tasks` and `Riot` of [the tour](docs/guide/tour.md), and `Files`, `Feed`, `Shapes`) exist in two versions: written by hand in `tests/tour`, exactly as the macro would generate them, and generated by the macro in `tests/tour-gen`. The test files live in `tests/tour/tests` and `tests/conformance/tests`; each `-gen` crate includes the same files with `#[path = …]`, so one test runs on both clients. Any difference between the macro and the hand-written model makes a test fail. [`tests/README.md`](tests/README.md) describes what each crate contains.

**To add an endpoint to a tour client:**

1. Add it to the description in `tests/tour-gen/src/<client>.rs`.
2. Add the same lines to the description that appears as a comment at the top of `tests/tour/src/<client>.rs`. A test (`c1_20_…`) compares the two.
3. In `tests/tour/src/<client>.rs`, write by hand the code that the macro generates for it, in the same way as the endpoints around it.
4. Write its test once, in `tests/tour/tests` or `tests/conformance/tests`. It runs on both sides.

## The rules of the code

The workspace lints enforce these rules, and `just gate` fails on any violation:

- No `unsafe` (`unsafe_code = "forbid"`).
- No `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, `dbg!`, no printing, and no indexing that can panic, outside tests (`clippy.toml` allows them in tests).
- Every public item is documented (`missing_docs = "deny"`).
- The macro's messages are part of its interface: the 46 messages in [the messages](docs/reference/messages.md) are compared with the macro, word for word.

## How a release is made

The five published crates share one version, in `[workspace.package]` of the root `Cargo.toml`, and pin each other with an exact version (`=5.0.0-rc.1`). Publishing is blocked by `publish = false`: today every crate sets it, through `[workspace.package]`, so nothing can be published by mistake.

1. Set the new version in `[workspace.package]` and in every `=` pin between the crates; add its entry to [CHANGELOG.md](CHANGELOG.md).
2. Run `just release`. Every step must pass.
3. Remove `publish = false`, then publish in the order of the dependencies: `concord-core`, `concord-macros`, `concord-extras`, `concord-api`, `concord-testkit`.

## Licence

A contribution is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at the user's option, like the rest of Concord.
