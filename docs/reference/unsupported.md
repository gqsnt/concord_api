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
- complete custom `Authorization` schemes such as `Signature`,
  `AWS4-HMAC-SHA256`, or `MAC`; generic private-header placement rejects
  `Authorization`, while dedicated Bearer and Basic placements remain
  supported;
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

Do not work around the custom-`Authorization` limitation with public headers.
Supporting another complete authorization scheme requires a dedicated
authorization or request-signing design that retains one private owner,
collision prevention, limiter-before-authentication ordering, and redaction.

Runtime-defined endpoints are appropriate for dynamic public request shape,
but they still require a typed response specification and use the same
executor. Use an application-level loop when a dynamic multi-request protocol
cannot be represented by the existing typed pagination contract.
