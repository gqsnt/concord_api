# Test Ownership

- `concord_core` unit tests cover private algorithms, body limits, native
  request/response handling, credential generations, and redaction helpers.
- `concord_core/tests` covers maintained runtime ordering, bounded auth
  recovery, response limits, pagination, retry modes, and the final public
  extension surface through deterministic loopback execution.
- `concord_macros` unit tests cover parsing, semantic resolution, generated
  source assertions, and documentation generation.
- `concord_macros/tests/trybuild` owns focused diagnostics for removed syntax
  and unavailable compatibility paths.
- `concord_examples` compiles consumer-facing usage without private integration
  imports.
- `perf` compiles and exercises only maintained public and benchmark surfaces.

Executable suites run through Nextest; doctests remain under `cargo test --doc`.
