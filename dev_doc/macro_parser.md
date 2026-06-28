# Macro Parser

Parser code lives under `concord_macros/src/parse/` and is organized by syntax area. The public syntax reference is `docs/dsl.md`; this document describes parser responsibilities.

## Ownership

- `parse/mod.rs` coordinates the top-level `api! { ... }` and `client` block.
- Auth parsing handles `secret`, `credential`, and auth use clauses.
- Policy parsing handles `headers`, `header`, `query`, inline query and header operations, and `fmt[...]` values.
- Retry and rate-limit parsers own profile declarations and local attachments.
- Behavior parsing owns behavior declarations and behavior use syntax.
- Endpoint and item parsing own scopes, endpoint lines, response-last structure, pagination, and map clauses.

## Client Parsing

The client parser accepts grouped config:

- `auth { ... }`
- `policies { ... }`
- `behaviors { ... }`
- `defaults { ... }`

It also accepts flat compatibility forms for `secret`, `credential`, `retry`, `rate_limit`, `behavior`, and `default { ... }`.

Grouped blocks flatten into the same raw storage used by flat declarations. Duplicate profile names are resolved later by sema.

## Scopes And Endpoints

Scopes parse route fragments, parameters, inherited policy, auth, and behavior attachments, plus nested items. `host [...]` is scope-level syntax.

Endpoint parsing keeps response-last structure:

```text
METHOD Name(args...)
-> Codec<T>
map Type { expr }
```

Normal endpoint policy and behavior clauses must appear before `->`. `map` is allowed after response because it transforms the decoded response.

Request bodies are endpoint signature arguments named `body`; there is no `body ...` endpoint clause.

Inline `query` and `header` clauses are accepted alongside block forms. `+=` is query-only; parser diagnostics reject header append.

## Rejected Syntax

The parser intentionally rejects:

- `body ...` endpoint clause
- `params { ... }`
- `part[...]`
- auth use clauses inside `auth { ... }`
- default attachments inside `policies { ... }`
- invalid items inside `behaviors { ... }`
- behavior declarations inside `policies { ... }`
- retry or rate-limit default attachments inside `policies { ... }`
- empty `behavior []` and `rate_limit []`
- duplicate names inside one `behavior [...]` or `rate_limit [...]` list

Parser diagnostics should point at the offending token or list site when practical. Diagnostics PRs should update trybuild stderr snapshots intentionally.

Same-site duplicate behavior attachments across multiple `behavior` clauses are rejected in sema rather than the parser, because a site is represented as a collected `Vec<BehaviorUseSpec>`.
