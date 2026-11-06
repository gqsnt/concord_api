# Author observations, in order

1. **Easy — Concord.** `src/lib.rs:65` declares two JSON GET operations with
   path and optional query inputs. `tests/protocol.rs` checks the resulting
   methods, trailing slashes, parameter order, headers and empty bodies against a
   loopback server.
2. **URL following — Concord / SDK adapter.** `src/lib.rs:176` prepares a list
   operation, parses and validates a server URL, replaces the generated request
   URL via public `Prepared::into_parts`, and invokes the original decoder at
   `src/lib.rs:219`. `tests/workflows.rs:45` starts at limit 2 and follows the
   returned URL containing limit 1 and reordered query parameters. The existing
   GitHub SDK's `meta::send_edited` supplied this pattern; no new paginator was
   assumed. Preparing an unrelated placeholder detail and a list just to follow a
   detail URL (`src/lib.rs:185`) reconstructs operation setup.
3. **Head and error retention — Concord / SDK adapter.** Generated `send()`
   returns the decoded value. At `src/lib.rs:225` the SDK captures the head before
   `decode` and wraps the unchanged Concord error, so callers retain metadata on
   decode and HTTP failures. Reproduce with `tests/protocol.rs` tests
   `status_error_keeps_head_and_unread_body` and
   `failed_body_keeps_head_and_transport_failure_has_no_head`. The GitHub SDK's
   `meta` module supplied this public response-boundary pattern.
4. **Nullable but required — SDK / Serde.** `src/lib.rs:49` uses an explicit
   deserializer so a missing `next` is not silently interpreted as end of list.
   `tests/protocol.rs` test `missing_next_is_not_mistaken_for_end` reproduces the
   distinction. Provider docs show null in the envelope example but do not state
   missing fields are valid.
5. **Traversal and partial outcome — application.** `tests/workflows.rs:84`
   owns the first page and follows its URL; the caller retains it after a later
   request fails. There is no automatic traversal, cycle detection or bounded
   pagination abstraction for this slice. `next: null`, including on an empty
   final page, is the completion signal.
6. **Detail projection — SDK.** `src/lib.rs:21` types common scalar fields and
   retains the provider's large nested body in `extra` as JSON. Consumers must
   interpret nested structures themselves; this preserves data without claiming a
   complete typed schema for sprites and other evolving substructures.
