# Modeling APIs in Concord v2

Start from domain ownership, then add wire placement and runtime capabilities.

## Choose the tree

Put stable build-time inputs on the client, reusable selected context on a
named scope, and operation input on the endpoint. Use nested scopes only when
their values or attachments genuinely apply to descendants. One invocation
contains one complete client tree.

Required non-defaulted scope/endpoint values become positional arguments in
source order. Optional/defaulted values become consuming controls on the owning
snapshot. If an endpoint accumulates too many required values, introduce a
meaningful domain input type rather than an adaptive generated parameter bag.

## Model presence explicitly

Do not use a type path to communicate omission. Write `value?: T` for an outer
omission slot and keep `value: Option<T>` when `None` itself is a required domain
decision. Use `#[default]` for the payload's `Default` and `= CONST` for a stable
declaration value.

Place repetition where it belongs. A `Vec<Tag>` remains one payload; a query
statement using `each endpoint.tags` turns that payload into repeated pairs.

## Keep ownership visible

Use the exact owner in expressions. A client value remains `client.tenant` even
inside nested scopes. A scope value uses its declared identifier, such as
`regional.region`. Endpoint inputs use `endpoint.id`. Duplicate scope identifiers
in one ancestor chain are forbidden so references never become contextual.

## Design wire patches locally

Choose assignment when a descendant owns the effective query/header field;
choose query append when inherited multiplicity must remain. Use replace/off
only when the descendant intentionally resets that attachment family. Remember
that absent optional assignment removes inherited values, while absent append
does nothing.

Keep media ownership with specifications: request specs own `Content-Type`,
response specs own default `Accept`, and endpoint `accept` syntax is the only
override.

## Choose durable identity

Give externally coordinated endpoints and rate policies explicit stable IDs.
Derived IDs are deterministic but rename-sensitive. Do not reuse one rate ID for
different dimension schemas or windows.

Rate profile formals make dependency visible at attachment sites. Profiles do
not capture lexical values. Rate dimensions should represent stable quota
identity, never cursors, secrets, credentials, or optional policy presence.

## Separate semantic and mechanical permission

Endpoint `repeatability idempotent` permits a resend only after every runtime
gate agrees. Body replayability says bytes can be reconstructed; it does not say
the operation is semantically repeatable. Declare both honestly.

Pagination live bindings grant mutation authority only over named endpoint
slots. Stable controller fields copy state without granting authority. Prefer
the smallest set of live bindings a controller needs.

## Review generated experience

Before accepting a contract, read it as each persona:

1. Can a caller predict constructor arguments and exact control signatures?
2. Can a reviewer reconstruct effective route, auth, rate, timeout, and
   repeatability with origins?
3. Can an operator distinguish public/redacted/private data and correlate
   operation, attempt, page, and policy IDs?
4. Does every fluent name match what retries, recovery, pagination, and
   cancellation really guarantee?

Use `Client::contract()` and semantic snapshots in review. See
[generated clients](generated_client.md) and [planning](planning_and_observation.md).
