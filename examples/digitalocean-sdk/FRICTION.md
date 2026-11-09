# Concord costs and decisions

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Baseline `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`; no shared changes made.
Example assistance and exact files/snippets consulted are inventoried in
[PROVENANCE.md](PROVENANCE.md). Observable diagnostics and decisions follow; no private
reasoning transcript is included.

## F1 — Following a link while retaining generated send policy

Evidence: `src/pagination.rs::pages` uses generated `Sends::send()` on the first page;
later pages need `Prepared::follow(...).receive()` and the same `Policy::finish`.
`Prepared::send` applies declared decoding, not the client's generated send policy.
The first page is also prepared once for its trusted collection URL and again by
generated `send()`. No extra HTTP execution occurs; protocol/workflow request counts
verify exactly one per attempted page.

Handwritten cost: generic endpoint bounds, template preparation and the shared policy
finisher. This is generic mechanics; next-field extraction and collection-path trust
remain provider rules. Smallest possible Concord improvement: a policy-preserving
followed-operation adapter implementing `Sends`, with a safe way to inspect the prepared
destination. That could avoid dispatch branching and duplicate first-page preparation.
Current shared `Prepared::follow` already supplies origin enforcement/redacted URLs;
no native request mutation or custom HTTP execution was necessary.

## F2 — Safe transport and generated construction

Evidence: `DigitalOcean::connect`/`for_fixture` configure redirects, native retries,
Referer, proxy use, timeout and bearer syntax. Generated `new(http, config)` deliberately
accepts any caller transport; `with_base_url` deliberately permits arbitrary hosts.
Tests prove that SDK-constructed transports return all common redirects and never
contact their loopback trap. Invalid fixture destinations fail before I/O.

Handwritten cost: constructors plus fixture-origin validation; the shared auth crate
eliminates credential formatting/validation. Destination trust is SDK/provider policy;
transport wiring is generic. Smallest optional improvement: a constructor hook or a
documented transport-configuration recipe colocated with generated client rustdoc.
Changing Concord's global defaults is unnecessary. Advanced generated/native APIs keep
their documented caller-owned transport boundary.

## F3 — Provider envelopes and bounded typed failures

Evidence: `Policy::finish`, `ProviderError` and `HttpClass` retain status while
`Received::document::<Json, Problem>(ERROR_BODY_LIMIT)` reports malformed, missing,
wrong-media, oversized and interrupted documents. Protocol tests exercise every listed
stage, preserving 429 even when its body fails; secret problem fields never format.

Handwritten cost: one policy, provider error model/status mapping, error formatting and
response wrapper. The provider field names and HTTP classes stay in this SDK. Generic
body limiting, JSON checking, endpoint identity and safe metadata already come from
Concord. A small reusable typed success/error decoding helper could remove the
remaining branch/metadata wrapping; it must leave the provider's errors/status mapping
as supplied closures. No custom codec or raw body reader was needed.

## F4 — Nested optional body links and bounded page adapters

Evidence: `Links` → optional `PageLinks` → optional `next`; `pages` projects that chain
into concord-pagination's `Page`. Tests cover all omitted nesting levels, nulls,
prev-only links, empty intermediate pages, a failed second page, and a cap on a page
with continuation. Invalid present links fail before a second send; hostname trap
tests cover both droplet and action listings. `NextLink` formats no URL.

Handwritten cost: two small envelope projections and URL collection/origin checks.
These are provider semantics. Shared `Pages` owns scheduling, cap/failed endings,
prefix preservation, cancellation and redacted traversal diagnostics. No Concord core
change is warranted for DigitalOcean's field names; an optional generic example showing
deeply optional body continuation would reduce discovery cost. The shared Link-header
feature is irrelevant because these are JSON body links.

## F5 — Quota metadata versus admission/retry engines

Evidence: four `ResponseMeta` accessors use Concord's strict `field::unsigned` parser.
Tests distinguish absence, invalid signed values, repetition and overflow, and assert
that 401/429/5xx send only once per call. The permitted public API page describes
per-request expiries and a separate burst limit.

Handwritten cost: provider header-name/accessor selection. `concord-ratelimit`'s local
fixed-window admission cannot itself establish the provider's sliding/distributed
quota. The slice exposes observations and returns 429 without automatic waiting.
`concord-retry` was read but not needed because this slice makes one attempt per call;
its engine would add unused policy/clock machinery. Numeric-only Retry-After uses the
existing strict unsigned parser, not the broader HTTP-date parser. The smallest
Concord change is none; adding a provider quota adapter to the SDK later would be an
explicit new behavior. `concord-auth::bearer` is used; its token cache is unnecessary
because this slice accepts a preobtained static bearer. `concord-codecs` JSON via the
facade is used; form/multipart/streaming formats do not fit finite JSON documents.

## Diagnostics and research limits

The first offline compile rejected `path ""` on two endpoints with:
`path templates are relative: no leading '/', and no empty template`. Omitting the
path declarations expressed the inherited collection/single-resource routes correctly.
This was an author syntax correction, with a useful diagnostic, not a shared defect.

The two allowed external pages do not include the full droplet Actions endpoint/schema
reference. The action workflow therefore has explicitly synthetic contract fixtures;
no live verification or unauthorized extra page was substituted. This is a research
scope limitation, not a Concord defect. All four endpoints and both workflows remain
implemented.
