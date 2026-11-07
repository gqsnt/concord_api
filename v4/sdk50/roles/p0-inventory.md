# P0 inventory: obligations and residue across the existing Concord corpus (read-only)

You are an independent retrospective reviewer for the Concord Rust library (repository root is
your working directory; a full checkout at SDK20 head 90b69011 plus one registry fix). Read
`AGENTS.md`/`CLAUDE.md` if present. You may not modify files: produce your whole result as your
final message (Markdown). No builds are needed; this is a source inventory.

## Product question

Concord's product is core (`crates/concord-core`), macros (`crates/concord-macros`), declarations
and generated usage under a fixed client → scope → endpoint hierarchy. The SDKs under
`examples/*-sdk` are evidence. We want to know which obligations SDK authors and callers still
implement by hand, and which of those are generic mechanics rather than provider semantics.

## Scope

The 25 SDK directories: airtable, amazon-s3, clickhouse, distribution-registry, dropbox,
frankfurter, gitlab, google-drive, linear, microsoft-graph, nager-holidays, notion, ntfy,
open-meteo, openai, pokeapi, slack, snowflake, twilio, world-bank (SDK01–20) and the five older
controls github, stripe, shopify, salesforce, kubernetes (keep the controls separate). Also
inventory the auxiliary examples (native, extension, multipart, riot, quickstart, app, sdk,
shapes, renamed, compat, fixture, testserver, *-app) briefly: what Concord surface each one
exercises.

Read current source first (`src/`), then each SDK's README/FRICTION/MAINTENANCE/FIRST_USE records.
Records are historical; mark a record finding CURRENT only if current source still shows it, and
HISTORICAL(resolved by <symbol>) otherwise.

## Deliverable (final message, Markdown, compact tables)

1. **Obligation × SDK matrix.** Rows (at least): typed provider error body; metadata retention on
   success/failure; HTTP-200 application errors; status-selected success documents; Link header
   parsing; body next-URL / cursor / offset / page-number pagination; continuation context
   binding; cycle/did-not-advance guards; Retry-After / rate headers; static credential header;
   Basic auth encoding; bearer token acquisition/refresh/cache; request signing; replay /
   idempotency keys; challenge-and-replay; runtime request media (multipart/CSV dialect); lazy or
   one-shot uploads; finite non-JSON formats (CSV/XML/text); incremental streams (SSE/NDJSON/CSV/
   watch); provider completion distinct from EOF; head-only outputs; native escape use
   (`into_parts`, `into_response`, `Incoming::new`, `Decoder::execute`); send policy
   (`send X;`/`SendWith`) adoption; generated-builder vs wrapper-method surface.
   Each cell: one primary handling category — `D` direct declaration, `H` shared helper (name it),
   `A` provider adapter written in the SDK (give file:symbol), `N` native escape (symbol), `U`
   unsupported/excluded, `-` not applicable.
2. **Recurring generic residue.** For each repeated hand-written mechanic, list every witness
   (file:line or file:symbol), the shared control flow, what is provider semantics vs generic
   mechanics, and classify the cost as one of: provider semantics / generic mechanics / current
   API limitation / SDK mistake / documentation issue / harness issue.
3. **Deferrals to reconsider.** Records (v4/sdk20/DECISIONS.md and SDK FRICTION files) that
   deferred a mechanic because "a factory/closure/native workaround exists", "one consumer" or
   "both work": list them with the current cost visible in source.
4. **What became easier in SDK20** (concrete before/after evidence from source+records), **where
   complexity moved** (into SDK wrappers, closures, factories), and **what the exercise did not
   test**.
5. **Stale current documentation**: places in current docs (crate rustdoc in
   `crates/*/src/lib.rs`, `docs/`, `README.md`, `examples/*/README.md`, `v4/sdk20/READOUT.md`,
   `v4/sdk20/STATUS.md`) that misdescribe the current execution/streaming/response API
   (e.g. claims that something must be hand-written although a public type now exists, or text
   that predates `Received`/`Incoming`/`SendWith`/`-> stream`). Give file:line and the current
   truth. Distinguish current guides from historical records (which must be preserved, possibly
   with a pointer).
6. **Scenario denominator proposal.** A fixed list (aim ~60–90) of common SDK scenarios
   (request/response, error, pagination, credential, replay, admission, streaming, upload,
   maintenance, negative cases) each with an id, a one-line definition and which existing SDKs
   witness it. This is the denominator later results will be counted against; include failure
   scenarios, not only happy paths.

Be concrete: symbol names and paths, no generic advice. Keep it under ~2,500 lines. Do not claim
anything was compiled or run.
