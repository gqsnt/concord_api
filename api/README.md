# api/: the code of Concord V5

A Cargo workspace. Its design is the paper in [`docs/`](../docs); [`docs/5.0.md`](../docs/5.0.md) is the whole contract on one page.

## The crates

| Crate | To be published | What it is |
|---|---|---|
| [`concord`](concord) | yes | The facade: the crate that a user depends on. |
| [`concord-core`](concord-core) | yes | The traits and the engine. No format, no scheme, no pager. |
| [`concord-macros`](concord-macros) | yes | `api!`. |
| [`concord-extras`](concord-extras) | yes | `json`, `form`, `multipart`, the readers, the pagers, `basic`, `tower`. |
| [`concord-testkit`](concord-testkit) | yes | For tests: a scripted local server, a transport with virtual time. |
| [`tour`](tour) | never | The clients of `docs/tour.md`, written by hand as the macro would write them. |
| [`tour-gen`](tour-gen) | never | The same clients, written by the macro. |
| [`conformance`](conformance), [`conformance-gen`](conformance-gen) | never | The tests written from `docs/contracts.md`, on both. |
| [`real-gen`](real-gen) | never | Descriptions of real APIs, compiled. |

The five published crates have one version, `[workspace.package]`, and name each other with an exact
version (`=5.0.0-rc.1`). For now every crate says `publish = false`: nothing is uploaded from here.

## Checks

All offline. Run the heavy ones through `../build/limited.sh`, which caps their memory.

| Recipe | What it checks |
|---|---|
| `just gate` | fmt, check, clippy, tests, docs, MSRV (1.96), the features of `concord`, the matrix of contracts |
| `just package` | What each published package holds, and that each builds alone, outside this workspace |
| `just props`, `just mutants`, `just features-all` | Properties, mutations, every feature subset on stable and 1.96 |

The pages as docs.rs will build them (each published crate with every feature on, as its
`[package.metadata.docs.rs]` asks), in `target/doc`:

```text
cargo doc --offline --no-deps --all-features -p concord -p concord-core -p concord-macros -p concord-extras -p concord-testkit
```

`just doc` builds them with the features of the gate (`tower`, `multipart`), and so does
`cargo doc` with `--features concord/tower,concord/multipart`. Without a feature, its items
(`multipart`, `Multipart`, `concord::tower`) have no page.
