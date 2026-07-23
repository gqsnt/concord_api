# Concord manual

Concord turns a remote HTTP contract into predictable typed Rust and executes
it through one finite managed lifecycle. The manual follows that same path.

## Start here

1. [Represent an API](guide/01-represent-an-api.md)
2. [Understand generated Rust](guide/02-understand-generated-rust.md)
3. [Build a client and make requests](guide/03-build-a-client-and-make-requests.md)

Continue with authentication, bodies, pagination, and explicit or runtime
endpoints in the remaining [guides](SUMMARY.md).

## Choose a section

- [Guides](guide/01-represent-an-api.md) teach one complete path and link to
  importable, tested examples.
- [Concepts](concepts/contract-model.md) explain the contract compiler,
  lifecycle, finite behavior, security model, and extension boundary.
- [Recipes](recipes/regional-apis.md) assemble capabilities for concrete jobs.
- [Reference](reference/dsl.md) owns exact syntax, generated signatures,
  features, limits, errors, compatibility, and dangerous capabilities.
- [Contributor documentation](contributor/architecture.md) owns internal crate,
  compiler, test, documentation, and release architecture.
- The root [migration guide](../MIGRATION.md) and
  [changelog](../CHANGELOG.md) describe the `0.1.0` architecture reset.

Exact facts have one owner. Guides link to reference pages instead of copying
feature tables, limits, or complete grammar. Internal bridge details appear
only in contributor documentation.
