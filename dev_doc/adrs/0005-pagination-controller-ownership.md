# ADR 0005 — Pagination controller owns page interpretation

- **Status:** Selected; trait and real-API gates pending

## Decision

The controller receives one complete `PageResponse<Page>` with exact decoded page and declared bounded metadata interests. It returns aggregate items plus completion/next state atomically.

## Property earned

Pagination supports envelopes, body cursors, short pages, and Link/header cursors without `Vec` inference, a separate `PageOutput` trait, or native response authority.

## Controls

- safe effect only;
- complete page output only;
- endpoint-owned scalar live bindings;
- declared headers/current URI only;
- cross-origin links rejected;
- independent aggregate limits;
- all-or-error local result;
- one aggregate deadline and send ceiling.

## Revisit

If several real controllers need another item collection or metadata role, add the smallest bounded semantic capability. Do not expose native response.
