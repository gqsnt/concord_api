# Wave 3 — SDK31–35 (payments and accounting)

Frozen library baseline: the integration head after the between-waves changes of Wave 2, their
review fixes and records, gated before the wave starts (identity and gate in `STATUS.md`). During
the wave no shared crate changes; each role owns only its SDK directory. Findings go to
`SYNTHESIS.md`.

| SDK | Provider (size) | Author | Fresh consumer | Maintainer | Hypotheses (cohort contract) |
|---|---|---|---|---|---|
| SDK31 | Adyen Checkout (complex) | Sol 6.1 | Opus | Opus | `X-API-Key`; versioned base URL; `Idempotency-Key` on POST; asynchronous payment states (`action` → `/payments/details`) |
| SDK32 | PayPal (complex) | Opus | Sol 6.1 | Sol 6.1 | client-credentials token cache (`concord-auth` single-flight, no refresh token); `PayPal-Request-Id` only where an endpoint allows it; HATEOAS `links` (followed-URL witness, B23-F) |
| SDK33 | Square Payments (medium) | Sol 6.1 | Opus | Opus | bearer + `Square-Version`; `idempotency_key` in the body; opaque cursor (`concord-pagination`); **simplest slice of the wave (control)** |
| SDK34 | QuickBooks Online (medium) | Opus | Sol 6.1 | Sol 6.1 | OAuth refresh with replaced refresh token (rotation hook, B23-X: second witness); realm in the path — a scope parameter |
| SDK35 | Xero Accounting (complex) | Sol 6.1 | Opus | Opus | rotating refresh token saved atomically (third witness); `xero-tenant-id` header — a scope; XML default, JSON opt-in (`concord-codecs`); varying paging metadata; 429 |

Rotation R-1 (owner instruction 2026-11-07, `DECISIONS.md`): Codex roles are `gpt-6.1-sol` only;
the author alternates Sol/Opus per SDK and the consumer and maintainer are the other family, each a
separate fresh session. Requested effort xhigh. Codex roles run with loopback network; Opus roles
are Claude Code subagents. Maintainer tickets are written after the consumer finishes, from the
provider documentation, never from the author's friction notes.

What this wave must tell Concord:

1. **Idempotent writes (three providers).** Adyen (header), PayPal (header, per endpoint) and Square
   (body field) all say "a retry reuses the same key and the identical request". Is that one
   mechanic the SDKs write three times — key generation, replay with the same bytes, refusing a
   changed body — and where does it belong (declaration, `concord-retry`, SDK)? `delivery()` tells
   them when a write may have happened; does anyone use it, and is `NotSent`-only enough?
2. **Tenant and realm (fixed hierarchy test).** QuickBooks's realm in the path and Xero's tenant
   header are scope parameters. Does client → scope → endpoint express "never cross a tenant"
   without SDK code, or does each SDK guard it by hand?
3. **Refresh-token rotation.** HubSpot was the first witness; QuickBooks and Xero make three.
   Decide the `concord-auth` rotation hook (publish the new refresh token atomically with the
   access token) at the synthesis.
4. **Followed URLs and request values (B23-F, B23-F3-3).** PayPal's `links` and Adyen's `action`
   are provider-supplied next steps; record who follows them and how.
5. **Feature discovery.** `send P -> Result<_, Error>`, `Error::delivery()`, `concord::field`,
   `limit`, `query(..)`: who finds what unprompted (they are now in the guide and cheatsheet).
6. **Simple control.** Square stays a short declaration plus models; no new concept is needed.
