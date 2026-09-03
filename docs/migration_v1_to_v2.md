# Migrating Concord v1 declarations to v2

Concord v2 is a major-language replacement, not a compatibility mode. Update a
contract as one semantic unit: declaration, generated call sites, runtime
configuration, tests, plans, and operational integrations.

## Mechanical vocabulary

| v1 | v2 |
|---|---|
| children after `client` | children nested inside `client { ... }` |
| `vars { ... }` | `values { ... }` |
| `vars.x` | `client.x` |
| `scope.x` | `actual_scope_name.x` |
| `ep.x` | `endpoint.x` |
| optionality inferred from `Option<T>` | explicit `name?: T`; written type remains opaque |
| `= default` | `#[default]` |
| vector repetition inferred from `Vec<T>` | explicit `each endpoint.values` at placement |
| `fmt[...]` | `concat [...]` |
| replacing `host` labels | composable `subdomain`, `subdomain replace`, or `subdomain off` |
| query append-only `=` | replacement `=`, append `+=`, `remove`, `replace`, `off` |
| public `Accept` | endpoint `accept` / `accept off` |
| `#[body] payload: BodySpec` | `body payload: Input using BodySpec` |
| `-> ResponseSpec` | `-> Output using ResponseSpec` |
| `ResponseBodySpec` | `ResponseSpec` |
| repeated `auth ...;` clauses | grouped `auth { ... }` |
| `challenge ...` | `recover on ...`; omission means no recovery |
| old pagination `bind` syntax | directional `controller -> endpoint.slot` |
| old rate `rule/by/use` syntax | profile formals, `key`, `allow ... per ...`, direct attachment |
| `response_bytes()` | endpoint-declared `accepted_bytes()` |
| `SafeRequestPlan` | `CredentialFreeRequestPlan` |
| `PageLimit::Take/HardCap` | `Prefix/CompleteWithin` |
| implicit retry semantic permission | resolved `repeatability` |

## Type and value traps

The written type now means exactly itself. `x: Option<T>` is a required payload:
the caller must supply `Some` or `None`. Write `x?: T` for an omittable value and
`x?: Option<T>` when omission, explicit clearing, and explicit setting are three
different domain states. The last form stores `Option<Option<T>>`.

Likewise, `Vec<T>` no longer means repeated placement. A scalar query assignment
formats the vector payload only if that payload implements the required scalar
contract. Use `"tag" = each endpoint.tags;` to emit repeated pairs.

`#[default]` calls `Default::default()` once when the owning snapshot is made.
An explicit `= CONST` initializer remains allowed. `clear_x()` selects outer
absence; `reset_x()` restores the retained declaration default. They are not
aliases.

## Query and ownership traps

Query assignment removes every inherited/current exact-key occurrence before
emission. If an optional assignment is absent, inherited values stay removed.
Append with `+=` when absence should leave inherited values unchanged.

Every scope reference now names its actual lexical owner. Replace `scope.region`
with, for example, `regional.region`. Duplicate scope identifiers in one
ancestor chain must be renamed.

## Runtime semantic changes

- A pagination timeout covers the complete aggregate terminal, including all
  pages, waits, retries, recovery, and controller work. It is not reset per page.
- Policy retry is stricter: endpoint repeatability, dispatch knowledge, body
  replayability, classifier, attempt limit, shared budget, and deadline must all
  permit another send.
- A contract with an effective rate policy cannot build without an explicitly
  selected limiter.
- `Prefix(n)` intentionally returns a successful prefix; `CompleteWithin(n)`
  errors without partial output if continuation remains.
- Plans may redact declared public values. They guarantee credential absence,
  not universal sensitivity safety.
- Completion observers cannot reject or rewrite an already-fixed result;
  request/response gates own pre-outcome rejection.

## Body, response, and terminal migration

Move the caller input type out of the request spec and the caller output type
out of the response spec. The macro emits associated-type equality assertions,
so a mismatch points back to the endpoint declaration. Add `expose
accepted_bytes;` only where a buffered/discard response may deliberately bypass
conversion while preserving status and negotiation semantics.

Streaming still returns a lazy bounded stream; receiving it does not mean the
remote body was fully consumed. Dropping any request future only stops local
polling and does not prove remote rollback.

## Audit workflow

Run `cargo run -p concord-xtask -- migration-audit` to find retired spellings
and type sites that need human review. Its optional `--write` rewrite can move
children and add an explicit root timeout. For old spec-shaped signatures it
emits `__ConcordV2BodyInputTypeRequiresReview` and
`__ConcordV2OutputTypeRequiresReview`; replace those markers with the exact
domain types. The tool deliberately cannot infer associated types, whether an
old `Option<T>` meant a required domain choice or omission, or whether a
`Vec<T>` should repeat. Review every reported site, then run `just verify`.
