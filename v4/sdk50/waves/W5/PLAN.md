# Wave 5 — SDK41–45 (infrastructure and observability, public data)

Library baseline: `c3fd4f1e` — the same as Wave 4 (owner instruction R-2: no between-wave
changes from Wave 4 on). Each role owns only its SDK directory; capsules under P-1.

| SDK | Provider (size) | Author | Fresh consumer | Maintainer | Contract pressure |
|---|---|---|---|---|---|
| SDK41 | DigitalOcean (small) | Sol 6.1 | Opus | Opus | bearer; `links.pages` in the body; a missing next link is completion; never forward the token to another host |
| SDK42 | Datadog Events (small) | Opus | Sol 6.1 | Sol 6.1 | site-specific host; API key + application key; wrong-region / missing-key errors without secrets |
| SDK43 | Sentry (medium) | Sol 6.1 | Opus | Opus | bearer; `Link` header with cursor and `results`; frequency and concurrency limits |
| SDK44 | Backblaze B2 Native (complex) | Opus | Sol 6.1 | Sol 6.1 | account authorization returning API/download URLs; per-upload URL and token; 503 needs a new upload URL; SHA-1; ambiguous uploads |
| SDK45 | arXiv (small) | Sol 6.1 | Opus | Opus | public Atom XML; `start`/`max_results`; namespaced extensions; usage limits; **simple control** |

Rotation R-1 continues (SDK40 Opus → SDK41 Sol). Findings are logged per role for the cross-50
analysis after SDK50; nothing changes in shared crates during or after this wave until then.
