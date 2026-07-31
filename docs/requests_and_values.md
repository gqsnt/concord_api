# Requests and values

- **Audience:** API authors and callers
- **Purpose:** concept guide
- **Normative reference:** [DSL specification](spec.md)

## Values are domain inputs; placement is a separate decision

A parameter may be used in a host label, path segment, query entry, public
header, rate dimension, or pagination binding. Its Rust declaration defines
the caller-facing value and generated controls. Its attachment clause defines
where it appears or which policy consumes it.

This separation keeps endpoint signatures domain-oriented while leaving wire
mapping explicit for endpoint authors.

## Required, absent, and defaulted values

- Required client values use builder setters.
- Required scope and endpoint values are constructor arguments in declaration
  order.
- Direct `Option<T>` spellings are absence-capable.
- Declaration defaults use `default`, supported literals, or const paths.
- Absence and default are independent capabilities.

Default evaluation occurs when the owning client, scope, or endpoint snapshot
is created. Attempts and pagination pages use the retained snapshot rather than
reevaluating defaults.

## Public expression namespaces

- `vars.*` refers to client public values.
- `scope.*` refers to one visible scope value.
- `ep.*` refers to endpoint-local values.
- `fmt[...]` constructs one formatted public atom from restricted direct atoms.

The restricted language keeps value dependencies visible for planning,
validation, rate-policy analysis, and security review.

## Base and host

`base` is one absolute HTTP or HTTPS origin with an optional base path. It
belongs to the client contract. HTTP configuration such as TLS roots, client
identity, proxy, or protocol selection belongs to `configure_http`.

`host [...]` selects generated DNS labels and replaces inherited generated
labels. `host off` removes them. Labels are validated independently and cannot
inject schemes, ports, paths, or delimiters.

## Path

`path [...]` appends ordered atoms. `path replace [...]` replaces inherited
generated atoms. `path off` removes them. Each atom is exactly one
percent-encoded URL segment; dynamic values cannot smuggle `/`, `\`, `.`, or
`..` across segment boundaries.

Domain containment and wire path are related but distinct. A scope may exist
for ownership or policy without adding a path; a path may contain literal
protocol structure that does not deserve a public scope type.

## Query

A query entry maps a domain value to a remote wire name:

```text
start_time: Option<i64>
→ "startTime" = ep.start_time
```

The caller sees `start_time`; the endpoint author sees both names; core owns
formatting and encoding.

- absent optional values append nothing;
- an empty present string remains a present empty value;
- vectors append one pair per element in iteration order;
- source order is retained;
- `query` modifies inherited entries;
- `query replace` replaces them;
- `remove "name"` removes one inherited name;
- `query off` removes all generated query entries.

Public query values may appear in safe plans and hooks. Private credential
query placement is a separate `auth query` operation that exists only in the
native URL after limiter admission.

## Public headers

Headers use case-insensitive identity and the same modify/replace/remove/off
model as query entries. Transport-owned, cookie, proxy, and authentication
headers are rejected as public headers. Authentication placement has separate
private syntax because it must not enter public planning or observation.

## Timeout selection

A DSL timeout has three states:

- omission: inherit;
- `timeout DURATION`: explicit selection;
- `timeout off`: explicit disablement.

Generated request controls can override, disable, or re-inherit the endpoint
selection. Retry is application runtime policy and therefore belongs to the
client builder, not the endpoint grammar.

## Guardrails produced from request information

Structured placement enables:

- exact percent encoding and bounded construction;
- static validation for literal hosts, paths, names, and collisions;
- runtime validation for dynamic formatted values;
- public/private information separation;
- safe request planning;
- canonical pagination identity;
- pagination/rate dependency checks;
- deterministic source-order preservation.

## Unsupported shapes

Concord deliberately does not expose arbitrary raw URL mutation or native
request callbacks. APIs requiring nonstandard query separators, raw
pre-encoded path bytes, or signing over the final native serialization may
need a lower-level transport or a separately designed semantic extension.
