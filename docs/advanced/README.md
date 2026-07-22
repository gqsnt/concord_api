# Advanced and extensible usage

Advanced APIs let applications supply semantic behavior while Concord retains the native HTTP boundary, finite acquisition, status classification, deadlines, cancellation, completion, and automatic redaction.

- [Endpoints, bodies, codecs, and factories](endpoints-bodies-and-codecs.md)
- [Response specifications, streams, and record decoders](responses-records-and-streams.md)
- [Credential providers, provider HTTP, hooks, and redaction](authentication-hooks-and-debug.md)
- [Custom pagination, limiters, observers, and explicit keys](pagination-and-rate-limits.md)
- [Dangerous raw-response and retry features](dangerous-features.md)

Start with the smallest extension point that solves the problem. An `EncodeBody` or `DecodeBody` implementation is simpler than a full body specification; a prepared endpoint is preferable to constructing a parallel request runtime.
