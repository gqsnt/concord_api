# real-apis: every description shown in the docs compiles

A workspace crate, never published. Descriptions compile, and recipe tests send requests only to scripted transports; no test calls a real API.

- `src/<api>/` for each page of [`docs/examples/`](../../docs/examples/README.md): each file is a
  block of the page, word for word (`description.rs`, `beside.rs`, `calling.rs`), except
  `stand_ins*.rs`: the data types that the page only names, cut to the fields its code uses.
  The first whole description uses `description.rs`; later ones use `description_002.rs`,
  `description_003.rs`, and so on, in page order (helpers and fragments do not count).
  Each whole description needs its own fenced block. Include each additional file in
  its own module in `src/<api>/mod.rs` and re-export its client there.
- `s3/wire.rs` also compiles in the library's test configuration, with dependencies
  confined to this crate's dev-dependencies. Ordinary library builds use `stand_in_wire.rs`.
- `src/guide/`: current whole descriptions from the guide, reference overview, and root README,
  with stand-in types. `fixtures/descriptions/` retains their exact page text and inventory.
- `fixtures/recipes/` holds complete Rust blocks from the guide. `tests/writer_recipes.rs`
  and `tests/stream_recipes.rs` include those same files, run them with `Script`, and
  check each block against its page word for word. The latter covers Docker/AWS frames,
  multipart responses and batches, separate service hosts, and the caller's upload loop.
  These files use dev-dependencies and are not library implementations.
- `fixtures/examples/` holds a block of an example page that needs a dev-dependency (the
  token store of the GitHub page uses `tokio::fs`). `tests/example_values.rs` includes it,
  checks the blocks of the Anthropic and Google Drive pages and that one word for word, and
  runs the streamed reply, the rotating token, the `multipart/related` upload and the
  compressed bulk write against scripted responses.
- `src/regressions/`: earlier descriptions retained as independent compilation and request tests.
- `tests/pages.rs` checks every whole description against its page and its compiled source,
  and checks the helper and calling blocks of the first eight example pages word for word.
  Refresh candidates with `python3 -B tools/doc_examples.py` under `target/doc-examples/`;
  review and copy them to the paths shown there. `--check` reports drift without writing.
- `tests/helps_compile.rs`: each help of the macro's messages, applied as written, compiles.
- Run: `cargo check -p real-apis --all-targets`, then `cargo nextest run -p real-apis`.
