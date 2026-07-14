# Macro Parser

Parser code lives under `concord_macros/src/parse/` and is organized by syntax area. The public syntax reference is `docs/dsl.md`; this document describes parser responsibilities.

## Ownership

- `parse/mod.rs` coordinates the top-level `api! { ... }` and `client` block.
- Auth parsing handles `secret`, `credential`, and auth use clauses.
- Policy parsing handles `headers`, `header`, `query`, inline query and header operations, and `fmt[...]` values.
- Rate-limit parsing owns rate-limit declarations and local attachments.
- Profile parsing owns profile declarations and profile use syntax.
- Endpoint and item parsing own scopes, endpoint lines, response-last structure, and pagination.
- The parser enforces a supported maximum DSL scope nesting depth of 64. Over-depth scope trees fail with a controlled diagnostic instead of recursing indefinitely.

## Client Parsing

The client parser accepts grouped config:

- `auth { ... }`
- `policies { ... }`
- `profiles { ... }`
  - `default { ... }`

It also accepts flat forms for `secret`, `credential`, `rate_limit`, `profile`, and `default { ... }`.

Grouped blocks flatten into the same raw storage used by flat declarations. Duplicate profile names are resolved later by sema.

## Scopes And Endpoints

Scopes parse route fragments, parameters, inherited policy, auth, and profile attachments, plus nested items. `host [...]` is scope-level syntax.

Endpoint parsing keeps response-last structure:

```text
METHOD Name(args...)
-> Codec<T>
```

Normal endpoint policy and profile clauses must appear before `->`.

Request bodies are endpoint signature arguments named `body`; there is no `body ...` endpoint clause.

Inline `query` and `header` clauses are accepted alongside block forms. Query assignment is the only query set syntax; repeated values use a resolved `Vec<T>` field and ordinary `=` assignment.

## Rejected Syntax

The parser intentionally rejects:

- `body ...` endpoint clause
- `params { ... }`
- `part[...]`
- auth use clauses inside `auth { ... }`
- default attachments inside `policies { ... }`
- invalid items inside `profiles { ... }`
- profile declarations inside `policies { ... }`
- retry or rate-limit default attachments inside `policies { ... }`
- empty `profile []` and `rate_limit []`
- duplicate names inside one `profile [...]` or `rate_limit [...]` list

Parser diagnostics should point at the offending token or list site when practical. Diagnostics PRs should update trybuild stderr snapshots intentionally.

Same-site duplicate profile attachments across multiple `profile` clauses are rejected in sema rather than the parser, because a site is represented as a collected `Vec<ProfileUseSpec>`.
