# Internals

Concord compiles tokens through `RawAst`, normalized API trees, resolved
semantic IR, facade IR, and code generation. Generated output consumes only
resolved facts; parser structures never enter runtime code.

Generated clients target the single current `concord_core::__private`
integration namespace. It contains descriptors, typed request/response
adapters, authentication bindings, pagination support, and narrow runtime
entry points required by macro expansions. It has no numeric namespace,
migration alias, or public stability promise. Normal application code must not
import it.

The macro hard-codes the semantic `ReqwestNativeGeneratedContract` marker and
checks core's `GENERATED_API_COMPATIBILITY` value with
`assert_macro_core_compatibility`. The unversioned assertion has no runtime
cost. When the generated contract changes, the semantic marker name changes,
so an incompatible macro/core pairing fails at expansion time without package
version inspection or a numeric private namespace.

Every production call uses the managed Reqwest client. Core owns request
planning, one bounded authentication recovery, rate-limit and hook ordering,
response limits, and terminal decoding. Reqwest owns protocol or configured
status retries within each visible execution. Generated code declares facts;
it does not implement execution loops, credential-cache sequencing, body
polling, response collection, or retry sequencing.

The public `advanced` module is limited to supported extension points: codecs
and content types, native streaming and multipart inputs, credential-provider
integration, sanitized debug and runtime hooks, pagination controllers,
rate-limit interfaces, retry modes, and safe managed-client configuration.
Runtime state containers, transport errors, response collectors, generated
plan adapters, and credential-cache internals are not public extensions.

Concord's deterministic loopback machinery exists only in test support and a
development-build seam. It cannot replace production execution in generated
clients.
