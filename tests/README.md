# tests/: the crates that test Concord

For someone who changes Concord: what the five crates here are, and why there are two of each. None of them is ever published.

| Crate | What it holds |
|---|---|
| [`tour`](tour) | The test clients: `Tasks` and `Riot` of [the tour](../docs/guide/tour.md), and `Files`, `Feed`, `Shapes`, written by hand as the macro writes them, each with its description as a comment at the top. The model of the generated code. |
| [`tour-gen`](tour-gen) | The same clients, written by the macro from the same descriptions. |
| [`conformance`](conformance) | The tests of [the contracts](../docs/reference/contracts.md), on the hand-written clients. |
| [`conformance-gen`](conformance-gen) | The same test files, on the macro's clients. |
| [`real-apis`](real-apis) | The descriptions of [the real APIs](../docs/examples/README.md), compiled. Nothing is sent. |

**Why two of each.** The hand-written clients say what the macro must write, in code that a person can read and step through. The macro's clients must behave the same. So each test is written once, in `tour/tests` or `conformance/tests`, and the `-gen` crate includes the same file with `#[path = …]`: one test, run on both. A test in `tour-gen` also checks that the descriptions on both sides are the same.

**Names.** A test that holds a contract starts with its number: `c8_4_…` holds C8.4. `just matrix` lists the tests of each contract.

How to add an endpoint on both sides: [CONTRIBUTING.md](../CONTRIBUTING.md).
