# Values, defaults, and controls

Declare request inputs where they are owned, then let Concord generate a
small, consistent control surface. Required, absence-capable, and defaulted
values deliberately behave differently.

## Put each value at its owner

Public request shaping refers to values through three namespaces:

```text
vars.tenant       a client value
scope.region      a visible enclosing-scope value
ep.verbose        a value of the current endpoint
```

Client values are configured while building the client. Required scope values
are arguments when entering that scope, and required endpoint values are
arguments when constructing that endpoint. This keeps required configuration
visible at the call site.

Protected inputs use the separate `secret.*` namespace inside credential
definitions. A secret is not a public value and cannot be used to shape a
host, path, public query entry, public header, description, or safe request
plan.

## Read the declaration by capability

These declaration forms are independent:

```rust
tenant: String;                         // required
locale: Option<String>;                 // may be absent
page_size: u32 = 25;                    // has a default
tags: Vec<String> = default;            // uses Vec::default()
filters: Option<String> = default;      // uses Option::default()
label: Option<&'static str> = DEFAULT;  // absent or defaulted
```

Concord recognizes absence syntactically for `Option<T>` and the fully
qualified standard-library spelling. A type alias to `Option<T>` is opaque to
the macro and therefore remains required. Defaults may use only the bounded
forms accepted by the DSL: literals, const paths, and the `default` shorthand.
The shorthand emits a hygienic `Default::default()` call and lets Rust check
the declared type's `Default` implementation. A missing implementation is
reported at the `default` keyword.

## Use `set`, `clear`, and `reset`

The generated controls correspond directly to those capabilities:

- `set_name(value)` selects a value;
- `clear_name()` selects absence when the declaration uses `Option`;
- `reset_name()` restores a declaration default when one exists.

A defaulted `Option<T>` has all three. Its `set_name` accepts `T`, not
`Option<T>`. The same rule makes `set_tags` accept `Vec<T>` for
`Option<Vec<T>>`.

This compiled builder example exercises every client-value combination:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/full_dsl.rs:configured_client}}
```

Repeated controls are deterministic: the last selection wins. Here
`reset_page_size()` discards the earlier `50`, and `reset_client_label()`
discards both the override and explicit absence.

Default timing follows the value owner:

| Owner | Evaluation point |
|---|---|
| client value | each `build()` where the value is not overridden |
| scope value | construction of that scope snapshot |
| explicit endpoint value | explicit endpoint construction |
| direct request value | endpoint snapshot construction |

Resetting a client builder value removes its override, so a later `build()`
evaluates the declaration again. Scope, explicit-endpoint, and request values
are already snapshots; their `reset_*` controls restore the retained evaluated
default without another evaluation.

`Default::default()` is application-owned code. It may allocate, block, panic,
or have side effects. Concord's construction guarantee is that Concord itself
performs no managed I/O or request-lifecycle effects; it does not make
arbitrary application trait code inert.

The same rules apply to endpoint-owned values:

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/full_dsl.rs:request_controls}}
```

The shorthand is deliberately limited to ordinary client, scope, and endpoint
values. It is not a request-body default, pagination-controller initializer,
public shaping expression, credential clause, or rate-limit clause.

## Match values to their use

The macro does not assign meaning to arbitrary Rust type names. Trait checking
adds only the bounds required by the places where a value is used. A scalar
placed in a host label, path segment, query value, header value, or formatted
expression must support the required public formatting. Repeated query values
may come from vector-shaped values and preserve iteration order; vectors are
not accepted in every scalar-only position.

Rate-limit partition values are stricter: they must be required or defaulted
scalars, and their `Display` output must be stable and canonical because it
participates in local quota identity. Optional values, vectors, secrets, and
credentials cannot become rate-limit dimensions.

When a type or expression does not meet a position's contract, keep the
conversion in application code and pass a suitable public value to Concord.
The [DSL reference](../reference/dsl.md) owns the exact accepted forms and
diagnostics.

## Related

- [Shape the public request](request-shaping.md)
- [Build and share clients](client-construction.md)
- [Generated value-control matrix](../reference/generated-api.md#generated-value-controls)
- [Complete DSL reference](../reference/dsl.md#values-optionality-and-defaults)
- [Security and redaction](../concepts/security-and-redaction.md)
