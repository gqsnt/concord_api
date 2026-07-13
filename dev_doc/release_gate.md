# Maintained Release Gate

`just release` is the canonical repository gate. It composes formatting,
all-feature workspace checks, strict Clippy, Nextest, doctests, rustdoc,
default-feature checks, no-default and all-feature MSRV checks, supply-chain
validation, performance-package checks and deterministic tests, and benchmark
compilation.

Focused diagnosis starts with `just test-core`, `just test-macros`, and
`just test-ui`. The separate performance package is checked with
`just perf-check`, `just perf-test`, and `just bench-check`.

The final source audit rejects numeric private namespaces, the removed
`internal` alias, generated reliance on runtime implementation modules, public
transport polymorphism, duplicate retry execution, automatic
`Retry-After` resend, manual multipart framing, and direct Hyper/Tower-family
manifest dependencies.

The source and compile-fixture audits also require the hidden development seam
to be absent from ordinary debug builds, explicitly gated by
`dangerous-dev-tools`, narrow enough to avoid runtime-engine re-exports, and
unused by generated clients and normal examples.
