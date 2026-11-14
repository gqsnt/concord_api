# real-apis: every description shown in the docs compiles

A workspace crate, never published. It sends no request: it only has to compile.

- `src/<api>/` for each page of [`docs/examples/`](../../docs/examples/README.md): each file is a
  block of the page, word for word (`description.rs`, `beside.rs`, `calling.rs`), except
  `stand_ins*.rs`: the data types that the page only names, cut to the fields its code uses.
- `s3/wire.rs` and `riot/quota.rs` also compile in the library's test configuration,
  with dependencies confined to this crate's dev-dependencies. Ordinary library builds
  use `stand_in_wire.rs` and `stand_in_quota.rs`.
- `src/guide/`: current whole descriptions from the guide, reference overview, and root README,
  with stand-in types. `fixtures/descriptions/` retains their exact page text and inventory.
- `src/regressions/`: earlier descriptions retained as independent compilation and request tests.
- `tests/pages.rs` checks every whole description against its page and its compiled source,
  and checks the helper and calling blocks of all eight example pages word for word.
  Refresh candidates with `python3 -B tools/doc_examples.py` under `target/doc-examples/`;
  review and copy them to the paths shown there. `--check` reports drift without writing.
- `tests/helps_compile.rs`: each help of the macro's messages, applied as written, compiles.
- Run: `cargo check -p real-apis --all-targets`, then `cargo nextest run -p real-apis`.
