# Developer Documentation

These documents describe how Concord is built internally. Public DSL usage lives in `docs/`.

`dev_doc/` is for maintainers changing parser, semantic analysis, code generation, runtime behavior, or release process. It should explain boundaries and invariants rather than repeat the public syntax reference. Link to `docs/dsl.md` when syntax details matter.

## Index

- [architecture.md](architecture.md): workspace boundaries and the end-to-end architecture.
- [dsl_pipeline.md](dsl_pipeline.md): macro pipeline from tokens to generated client and runtime request plans.
- [macro_parser.md](macro_parser.md): parser modules, grouped config, endpoint parsing, and parser diagnostics.
- [sema.md](sema.md): semantic resolution, profile inheritance, policy merging, and profile lowering.
- [codegen.md](codegen.md): generated Rust shape, endpoint wrappers, policy emission, pagination, auth acquisition, and rustdoc.
- [core_runtime.md](core_runtime.md): runtime execution order and invariants in `concord_core`.
- [profiles_and_policies.md](profiles_and_policies.md): policy declarations, attachments, profiles, and why profiles are resolved before runtime.
- [auth_runtime.md](auth_runtime.md): secrets, credentials, auth state, endpoint-backed auth, refresh, and redaction boundaries.
- [pagination_and_codecs.md](pagination_and_codecs.md): codec and pagination extension points.
- [endpoint_io.md](endpoint_io.md): endpoint I/O expansion contract, reserved families, and runtime behavior rules.
- [testing.md](testing.md): test strategy and the checklist for adding DSL features.
- [release_gate.md](release_gate.md): release gate and invariant checklist.
- [release_checklist.md](release_checklist.md): release verification commands and manual audit list.
