# SDK28 maintenance: safe ticket update

## Exposure and sequence

On 2026-11-07, before attempting the ticket, I read the workspace manifest, this SDK's
manifest, source modules, README, PROVENANCE, existing `first_use`, `protocol`, `workflows`
and test support, plus the local testserver API. I read the official Tickets, pagination,
rate-limiting and linked collision documentation listed below. I did not read this SDK's
`FRICTION.md` or `FIRST_USE.md` until after implementing the first attempt and passing its
three focused local tests. Those notes then provided prior observations about operation
metadata, retry plumbing, and the origin-only fixture base. No other example SDK or shared
crate source was used for the implementation. No live provider calls were made: **NOT_RUN**.

## Change and provider basis

`TicketUpdate::new(&ticket, status, comment)` copies `ticket.updated_at`; the generated
`api.ticket(ticket.id).update(...)` sends `safe_update: true`, the copied `updated_stamp`,
status, and comment in the `ticket` envelope. Use the id of the same ticket as the stamp:
the generated scope and update body are independent inputs and the SDK does not verify that
they match. A successful update returns `TicketResponse`. Only this endpoint's 409 maps to
`Error::Collision`, preserving the HTTP details; re-read and decide on a new change. It is
never automatically resent. A lost response, 5xx, or unreadable success is `Error::Uncertain`
without an idempotency key; investigate the result before choosing whether to try again,
since a repeated comment can duplicate it. Existing read and create handling is retained.
The update exposes the provider's default comment visibility; specifying visibility, other
update fields and retrieving comments are outside this change.

Read 2026-11-07: [Tickets: Update Ticket](https://developer.zendesk.com/api-reference/ticketing/tickets/tickets/#update-ticket)
documents the PUT endpoint, envelope, status/comment example and 409; [Protecting against
ticket update collisions](https://developer.zendesk.com/documentation/ticketing/managing-tickets/creating-and-updating-tickets/#protecting-against-ticket-update-collisions)
documents `safe_update`, the last-read `updated_at` as `updated_stamp`, and handling a 409
with a fresh read. Also read [Pagination](https://developer.zendesk.com/api-reference/introduction/pagination/)
and [rate-limiting best practices](https://developer.zendesk.com/documentation/api-basics/best-practices/best-practices-for-avoiding-rate-limiting/)
on that date; existing local tests exercise cursor pagination and Retry-After.

## Locations and duplication

- `src/lib.rs`: export `TicketUpdate`, declare PUT under the existing ticket scope and document
  the operation and limits.
- `src/models.rs`: serialize the update envelope and copy the prior timestamp.
- `src/policy.rs`: classify the update's 409 while retaining one-shot write behavior.
- `src/error.rs`: add collision error, its HTTP accessors and display, and distinguish recovery
  instructions for writes with and without keys.
- `tests/update.rs`: wire shape and collision classification, a consumer's read / collision /
  re-read / update workflow, and lost response, 5xx and unreadable-success cases.
- `README.md`: call, example, failure semantics, scope and transport guidance.
- `PROVENANCE.md`: exact update and collision sources and synthetic test descriptions.
- `MAINTENANCE.md`: this record. Existing `tests/first_use.rs` and all its assertions remain
  untouched. Root workspace membership and lock entry already existed at the frozen HEAD;
  no manifest, lockfile, shared crate, other example or script needed a change. No crate was
  added and no package version changed.

The operation identity appears in the declaration, `policy.rs`'s string comparison and tests;
the JSON shape appears in the model, tests and documentation. The rule that writes are sent
once and how an uncertain write is resolved appears in policy, errors and README. The compiler
caught no omitted edits in this attempt: the initial offline build and focused tests passed.
The focused tests exercise the operation-name comparison and exact wire fields, so an incorrect
name or omitted safe-update field would fail there; this is a coverage observation, not a
claim that such an omission actually occurred. Generic `Retry::receive` setup, Retry-After
parsing in both the classifier and error reader, and the model's JSON envelope machinery
remain duplicated. The separate ticket id and stamp are also a caller responsibility, as are
OAuth token lifecycle, collision review, comment reconciliation and provider-specific status,
body and error semantics.

Candidate Concord improvements (not implemented under the shared-crate freeze): provide a
stable typed endpoint identity or endpoint metadata usable by `SendWith`, so classification
does not depend on a repeated operation-name string; consider a policy-preserving way to
expose response metadata on success; and consider a Tokio convenience for the generic retry
clock/sleep plumbing. The provider must still decide which operation is safe to retry and what
409 means. The prior friction notes also discuss these boundaries; no shared edit is asserted
necessary for this ticket.

## Validation

- `CARGO_BUILD_JOBS=4 cargo build --offline -p concord-sdk-zendesk-support` passed before
  locked commands, using the default worktree target.
- `CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-zendesk-support` passed:
  31 integration tests (2 first-use, 19 protocol, 3 update at that point, 7 workflows) and
  one compile doctest.
- After adding the additional 5xx/unreadable-success test,
  `CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-zendesk-support --test update`
  passed all 4 tests. Cargo commands ran sequentially; no provider calls or commits.
