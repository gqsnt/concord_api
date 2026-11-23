# Contributor tools

For contributors checking or releasing Concord: these tools run the checks and measurements listed by `just`.

*[Concord](../README.md) · contributor tools*

Cargo work is offline; generated reports and scratch copies stay under `target/`.

Set `CONCORD_SKIP_SOCKETS=1` for `just test`, `just gate`, `just mutants --jobs 1`,
or `just release` when local TCP sockets are unavailable. The gate reports the
number skipped. Only the explicit cases in `socket-tests.txt` and the test kit's
server doctest are excluded; the example that needs no server still runs. With sockets
available, leave the variable unset so every case and mutation runs.

- `gate.py`: run workspace checks and print a final pass/fail line for each step.
- `matrix.py`: map contract numbers to tests in `target/contracts-matrix.md`.
- `links.py`: check every tracked Markdown file and local heading anchor; anchors are strict in the gate.
- `doc_examples.py`: verify every whole guide, overview, README, and API-example description against its compiled source; prepare reviewed replacements under `target/doc-examples/`.
- `mutate.py`: apply one catalogue mutation to a scratch copy and require a test or compilation failure.
- `mutations/`: source faults and runner tests; see its README to add a mutation.
- `sockets.py` and `socket-tests.txt`: shared explicit socket selectors and server-doctest locations.
- `package.py`: inspect, unpack, and build the five release archives without uploading them.
- `features.py`: check the feature flags passed to reqwest individually and together, and check detached crates on stable and Rust 1.96.
- `panics.py`: inventory panic-prone production operations and check their local safety reasons.
- `docs.py`: check public API documentation and rendered facade pages.
- `surface.py`: generate and compare `public-surface.txt` from nightly rustdoc JSON.
- `bench.py` and `measure/`: run benchmarks and report timing and allocation comparisons; `just bench --expansion [FILE]` measures macro expansion and writes `target/expansion-measurement.json`.
- `release.py`: run `surface`, the gate, `docs-audit`, `doc-examples`, `call-doc-search`, `call-editor`, safety, packaging, feature isolation, and mutation checks in order.
- `NO-TEST.md`: explain contracts checked by the compiler or by tests carrying other numbers; `just matrix --strict` requires a test or a reason.
- `test_gate_matrix.py`, `test_links.py`, `test_panics.py`, `test_features.py`, `test_doc_examples.py`, `test_public_surface.py`, `test_surface.py`, `test_extensions.py`, and `test_release.py`: Python regressions; run `python3 -B -m unittest discover -s tools`.

`just docs-audit` checks public documentation and its rendered API. `just doc-examples` compiles every whole documentation description and checks its page text. `just call-doc-search` queries the rustdoc index with Node. `just call-editor` checks completion and hover with `rust-analyzer` and `rust-src`. All four run in `just release`. Timing measurements stay opt-in because they depend on the machine.

`just surface` uses rustdoc JSON format 61 and the
`x86_64-unknown-linux-gnu` target. The snapshot records all-feature public
signatures, including re-exports, trait implementations and bounds, and lists
the public paths available with default features. It records third-party
re-exports such as `concord::reqwest` at the re-export itself and omits blanket
implementations of traits from third-party crates. Other feature combinations,
`Send` inferred from an `async` function body, and the `api!` grammar are
outside its scope.
Paths under `__private` and items marked `#[doc(hidden)]` contribute to the
hidden counts. Fingerprints cover each hidden path and signature. Rustdoc JSON
is built offline under `target/surface-snapshot/` with `nightly-2026-10-13`.
Set `CONCORD_SURFACE_TOOLCHAIN` to compare with another installed toolchain.

Name changed public paths in inline code under `## Unreleased` in the changelog.
Run `just surface` to see the diff and verify the entries. `just surface --update`
refreshes the snapshot once the public entries are present. Hidden changes
require only a snapshot refresh. The release runs `just surface --release`
and requires an exact match. See [Changing the public API](../CONTRIBUTING.md#changing-the-public-api).
