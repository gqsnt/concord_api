# Unsupported forms

Concord deliberately does not provide:

- native request or response escape to extensions;
- exact authenticated/native request previews;
- unbounded body, response, stream, status-body, or provider modes;
- parallel pagination, partial pagination results, or runtime pagination
  without a coherent typed controller contract;
- a second executor for runtime endpoints, providers, pagination, raw
  responses, or downstream integrations;
- unlimited authentication recovery;
- detached correctness completion work;
- secret expressions in public URL/header shaping;
- macro recognition of concrete codec, provider, limiter, or pagination
  controller names;
- trait-import-dependent ordinary request methods;
- positional secret/provider client construction;
- arbitrary Rust expressions in URL/header/auth shaping;
- distributed strict quota enforcement;
- WebSocket management, an SSE reconnection framework, or multipart response
  parsing.

Alternatives are semantic extensions that remain inside the existing bounded
lifecycle: define a body/response specification, record decoder, credential
provider, pagination controller, limiter, observer, or hook. If an extension
requires native transport ownership or an unbounded acquisition mode, it is
outside Concord's managed-runtime contract.

Runtime-defined endpoints are appropriate for dynamic public request shape,
but they still require a typed response specification and use the same
executor. Use an application-level loop when a dynamic multi-request protocol
cannot be represented by the existing typed pagination contract.
