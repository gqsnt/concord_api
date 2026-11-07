# Wave 4 — SDK36–40 (social and media, public data, infrastructure)

Frozen library baseline: the integration head after the between-waves changes of Wave 3 (B34-D,
B34-R, B34-O) and their reviews, gated before the wave starts (identity and gate in `STATUS.md`).
During the wave no shared crate changes; each role owns only its SDK directory.

| SDK | Provider (size) | Author | Fresh consumer | Maintainer | Hypotheses (cohort contract) |
|---|---|---|---|---|---|
| SDK36 | Spotify Web API (medium) | Opus | Sol 6.1 | Sol 6.1 | OAuth refresh where a missing replacement refresh token must not erase the stored one (rotation: fourth provider after B34-A was not accepted); next-link paging; 429 `Retry-After`; quota-mode 403 |
| SDK37 | Strava (medium) | Sol 6.1 | Opus | Opus | OAuth refresh; two usage windows (short, daily) — `concord-ratelimit`'s next provider; one-shot multipart upload never replayed; poll the upload id |
| SDK38 | Mastodon (medium) | Opus | Sol 6.1 | Sol 6.1 | instance-dependent host (base per instance: generated `with_base_url` vs ownership, B34-M); SSE framing across chunks, heartbeats, disconnect surfaced without invented replay |
| SDK39 | MediaWiki Action API (small) | Sol 6.1 | Opus | Opus | the `continue` object: every field carried forward; caller-set User-Agent; **simplest slice (control)** |
| SDK40 | Cloudflare Zones (small) | Opus | Sol 6.1 | Sol 6.1 | bearer token; per-endpoint pagination envelopes; HTTP 2xx with `success: false` (second witness after QuickBooks' fault-in-2xx: B34 candidate C); rate-limit headers |

Rotation R-1: the author alternates Sol/Opus (continuing from SDK35 Sol); consumer and maintainer
are the other family, separate fresh sessions. Capsules forbid reads outside the worktree (P-1);
temporary files under `<worktree>/target/tmp`.

What this wave must tell Concord:

1. **Declared values (B34-V).** Does any policy or SDK re-derive a declared value (continuation
   fields, zone id, upload id)? Count it against the getters/facts experiment.
2. **Ownership and `send` placement (B34-M, B34-S).** Mastodon's per-instance host and
   Cloudflare's account/zone: do generated mutators or a client-wide `send` cost code again?
3. **Errors inside a success (C).** Cloudflare `success: false` with HTTP 200, after QuickBooks'
   `Fault` in a 2xx: one mechanic or two vocabularies?
4. **Capability crates.** `receive_with` (B34-R) found unprompted? Rotation without a shared
   crate (Spotify). `concord-ratelimit` with two windows (Strava). SSE (Mastodon) and one-shot
   multipart (Strava) from `concord-codecs`.
5. **Simple control.** MediaWiki stays a short declaration plus a continuation adapter.
