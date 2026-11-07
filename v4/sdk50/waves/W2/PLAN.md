# Wave 2 — SDK26–30 (messaging and CRM)

Frozen library baseline: the integration head after the between-waves changes of Wave 1 and their
review fixes, gated before the wave starts (identity and gate in `STATUS.md`). During the wave no
shared crate changes; each role owns only its SDK directory. Findings go to `SYNTHESIS.md`.

| SDK | Provider (size) | Author | Fresh consumer | Maintainer | Hypotheses (cohort contract) |
|---|---|---|---|---|---|
| SDK26 | Postmark (small) | Terra | Astra | Opus | server-token header; count/offset listing (`next_offset`); unknown send outcome after a timeout; **simple control** |
| SDK27 | Mailgun (medium) | Astra | Opus | Sol 6.1 | HTTP Basic `api:key` (`concord-auth`); regional base; multipart with a non-rewindable attachment (never replayed); provider paging URLs |
| SDK28 | Zendesk Support (medium) | Opus | Sol 6.1 | Terra | OAuth bearer; cursor vs offset per endpoint; `Retry-After` honoured for safe reads only (`concord-retry`); uncertain create |
| SDK29 | Intercom (medium) | Sol 6.1 | Terra | Astra | bearer + version header; 429 without reset headers (no busy loop, no invented reset: second rate-limit witness) |
| SDK30 | HubSpot CRM (complex) | Terra | Astra | Opus | OAuth refresh with replacement refresh token under concurrency (`concord-auth` single-flight); search continuation `paging.next.after`; ambiguous create |

Rotation continues the ROUTING table from Wave 1 (author one family later per SDK; consumer =
author + 1, maintainer = author + 2). Requested effort xhigh. Codex roles run with loopback
network; Opus roles are Claude Code subagents. Maintainer tickets are written after the consumer
finishes, from the provider documentation, never from the author's friction notes.

What this wave must tell Concord:

1. **Why policies remain (B12-K).** Every Wave-1 policy left after `limit` maps errors or status
   documents. Five providers with different error bodies (Postmark `ErrorCode`, Mailgun message
   text, Zendesk `error`/`details`, Intercom `errors[]`, HubSpot `category`/`correlationId`): what
   does each author write between `Received` and the SDK error? Is it the same mechanic?
2. **New features used unprompted?** `limit`, `query(..)`, `ResponseHead::unsigned`,
   `check_content_type`, `next_offset`, parameter groups, `Sends<O>`. Record who found what.
3. **Capability crates with second and third providers.** `concord-auth` (Basic, bearer, OAuth
   refresh with replacement tokens), `concord-retry` (Retry-After for safe reads; never replay an
   ambiguous write), `concord-ratelimit` (Intercom/Zendesk/HubSpot 429s). Acceptance requires
   adoption that subtracts; a capability that grows an SDK is reported as such (B12-G precedent).
4. **Ambiguous writes.** Four of five slices require an uncertain-outcome create/send. Is the
   same SDK code written five times?
5. **Simple control.** SDK26 Postmark stays a short declaration plus models; no new concept is
   needed for it.
