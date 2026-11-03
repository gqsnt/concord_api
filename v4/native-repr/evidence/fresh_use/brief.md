# Fresh-use trial (agent simulation, not a human study)

You are a Rust developer who has never used Concord. You want a typed client for the "Atelier"
API below. Concord lives in a checkout at `/home/canarit/projects/concord_v4_native_work/snapshots/fresh-34bc6b6` (do not modify it). Create your own crate
under `/home/canarit/projects/concord_v4_native_work/fresh/trial` that depends on it by path.

## How to learn (this is what is being measured)

1. Start with `/home/canarit/projects/concord_v4_native_work/snapshots/fresh-34bc6b6/README.md`, then `/home/canarit/projects/concord_v4_native_work/snapshots/fresh-34bc6b6/docs/QUICKSTART.md`. Read nothing else up
   front.
2. After that, look things up **on demand only**, when a task needs it: the generated rustdoc of
   your own crate (`cargo doc --no-deps` in your crate, then read the generated HTML/text under
   `target/doc/`), then the guides in `/home/canarit/projects/concord_v4_native_work/snapshots/fresh-34bc6b6/docs/` (open the section you need, not the whole
   file), compiler errors, `docs/DIAGNOSTICS.md`.
3. Do not read Concord's source (`crates/*/src`), tests or examples' source, and do not expand the
   macro — unless you are stuck for real; then record it as a lookup with the reason.
4. Keep a lookup log as you go: for every lookup, the file + section (or rustdoc item), why, and
   roughly how many lines you read. Also log every **incorrect prediction** (what you expected the
   declaration/API to do or accept vs what happened) and every **workaround**.

## The API (base `https://atelier.example.test/v3`)

- Every request carries `authorization: Bearer <token>` (a secret the app supplies at startup)
  and `accept: application/json`. The app reuses one prebuilt `HeaderValue` for the accept header.
- Most bodies and responses are JSON.
- `GET /artworks?artist=&page=` (both optional) → JSON list of artworks.
- `GET /artworks/{id}` → JSON artwork. `DELETE /artworks/{id}` → 204 No Content.
- `POST /artworks` with a JSON new-artwork body → JSON artwork.
- `PATCH /artworks/{id}` with a JSON merge patch, sent as `application/merge-patch+json` → JSON.
- `GET /artworks/{id}/notes` → plain text (`text/plain`), to be returned as a `String`.
- `PUT /artworks/{id}/image` with raw PNG bytes (`image/png`) → 204.
- `GET /artworks/{id}/image` → raw bytes.
- `POST /search` with an **optional** JSON query body (absent means "everything": no body at all)
  → JSON results.
- `POST /session` with a form-urlencoded `username`/`password` body → JSON token response.
- `GET /export` → CSV (`text/csv`) that the app wants parsed into a `Vec<Row>` by its own small
  parser.
- Some requests need a per-request signature header `x-signature` whose value the app computes at
  runtime as a `HeaderValue` it has already marked sensitive (`set_sensitive(true)`); it must never
  show up in logs or `Debug` output.
- Error responses carry a JSON problem document; the app wants to read it on a 404.

## Tasks

T1 declare the client (token, accept header) and point it at a local test server for tests.
T2 list and fetch artworks (optional query parameters).
T3 create, patch (merge-patch media type) and delete (204) an artwork.
T4 notes as raw text; image upload/download as bytes.
T5 search with and without a body.
T6 log in with the form body (enable whatever is needed).
T7 the CSV export through your own format.
T8 the signature header: per-request, natively sensitive, never in `Debug`.
T9 a per-request timeout on one call only.
T10 on a 404, read the problem document.

Verify each task with an executable check: either `prepare()` assertions (URL, headers, body
bytes, `Content-Type`, `Debug` output) or a small loopback server in your tests. Use
`--offline --locked` where possible (the dependencies are in the local cargo cache; your crate may
need its own lockfile: copy `/home/canarit/projects/concord_v4_native_work/snapshots/fresh-34bc6b6/Cargo.lock` into your crate first).

## Report

Write `/home/canarit/projects/concord_v4_native_work/reports/fresh_use.md`: your model (and whether your effort setting is observable), the final
declaration, the lookup log (ordered, with line counts), incorrect predictions, workarounds, which
tasks passed which checks, the three most confusing points, and what in the README/QUICKSTART or
the generated rustdoc would have prevented each lookup. State clearly that this is an agent
simulation, not a human study. Final message: ≤300-word summary.
