# Wave 2 synthesis (SDK26–30, messaging and CRM) and the changes made before Wave 3

Wave 2 froze Concord at `8c27cd1a` and built Postmark (simple control), Mailgun, Zendesk Support,
Intercom and HubSpot CRM, each with an author, a fresh docs-only consumer and an independent
maintainer (15 roles; ledger in [`../../MODELS.md`](../../MODELS.md)). 33 findings are in
[`FINDINGS.md`](FINDINGS.md). Every change below was compiled against the SDKs that motivated it.

## What Concord learned

**1. What wave 1 added was found without coaching — except the one thing only in rustdoc.**
Feature use across the five authors: `limit` 5/5, `Received::document` 4/5, `decode_with_meta`
2/5, `query(..)` 1/5 (Mailgun's multi-key time range: a third unlike witness, found from the
guide), `concord-auth` 4/5, `concord-pagination` 4/5, `concord-retry` 3/5 (two used only its
`Retry-After` parser; Zendesk is the first adopter of `Retry::receive`), `check_content_type` 1/5,
`concord-ratelimit` 0/5. The strict header readers (`ResponseHead::unsigned`) were used by
nobody: Postmark parsed `Retry-After` with `trim().parse()` (accepts `+5`) and Intercom wrote its own
strict parser and asked for "a shared unsigned-header projection". They existed, documented only in
rustdoc. Discoverability is part of a feature.

**2. The generated `send()` signature was the most common consumer friction.** Two more consumers
(SDK26, SDK28) read `<Policy as SendWith<T>>::Output` — seven witnesses over two waves, after the
wave-1 doc link had made it navigable but not readable. The macro cannot compute a policy's
associated output; the declaration can state it.

**3. Policies are now short; what remains is the provider's error vocabulary and write
ambiguity.** Intercom's whole policy is 12 lines (`document` + `decode_with_meta`). The repeated,
risky part is elsewhere: four of five slices require an uncertain create/send, and each SDK
decided "may this write have happened?" differently — Zendesk and Discord from
`transport().is_connect()`, Postmark and Mailgun by method, HubSpot by mapping every failure
(build errors included) to `CreateUncertain`.

**4. A connect error is not proof that nothing was sent.** The first `Error::delivery()` design
classified a connect failure as `NotSent`; the independent review showed a POST that the server
performed, answered with `303`, and whose redirect target refused the connection: reqwest returns
the connect error. Reproduced in `w2_delivery.rs`. Concord cannot see the client's redirect policy
or whether a connection error came from a nested source. So Concord states only what it can prove,
and the SDK keeps the rest as a documented rule. The adoption showed what that leaves: `NotSent`
arises only while building, so a policy that prepares first and then receives (Postmark, Mailgun)
gains nothing from it; HubSpot's `send()`-based create uses it as the readable form of its build
rule; Zendesk and Discord keep `is_connect()` because their clients disable redirects — a fact
about their client that Concord cannot see.

**5. Followed provider URLs, correlated inputs and read-modify-write of request values are real,
repeated costs** — but each needs a design that is not obvious (below, experiments).

**6. Capability crates.** `concord-auth` met its first real consumers (Basic for Mailgun, bearer for
Zendesk/Intercom, `TokenCache` single-flight refresh for HubSpot); its gap is refresh-token
rotation (provider state published atomically with the access token). `concord-pagination` was
adopted 4/5; four SDKs hand-wrote "refuse a repeated continuation" — but a shared opt-in guard,
built and tested, was adopted by none of them (below, B23-P). `concord-ratelimit`: six providers across two waves, zero adoptions — Zendesk's and
HubSpot's quotas are account-wide, Intercom's 429 has no reset to schedule, Discord's tested policy
classifies windows without budgets.

**7. Simple control.** Postmark: 322 source lines (declaration of 3 endpoints, a policy for error
documents and unknown outcomes, models). Its consumer had no prediction mismatch. Its maintainer
added batch sending in one declaration line plus result interpretation.

## Changes made before Wave 3 (`sdk50/b23-core`, reviewed)

| ID | Change | Placement | Witnesses / evidence | Review |
|---|---|---|---|---|
| B23-A2 | `send Policy -> Result<_, Error>;`: the declared result of every generated `send()` (`_` = the endpoint's output); exact equality with the policy's output (E0271 at the template); plain `send P;` unchanged | MACRO | 7 consumers; adopted by 10 earlier SDKs (NASA rustdoc: `<Bounded as SendWith<Asteroid>>::Output` → `Result<Asteroid, Error>`) | findings 2–4 fixed: lifetime/const `_` untouched, forwarded fragments kept, coercion refused |
| B23-R | `Error::delivery()` → `NotSent` (Concord refused before sending) / `Unknown` (client failed, no head — a refused connection included) / `Answered` (a head arrived) | CORE | 5 SDKs classifying ambiguous writes by hand | finding 1 (P1) reproduced and fixed: no `NotSent` for transport failures |
| B23-C | `concord::field::{single, unsigned}` on any `HeaderMap`; `ResponseHead` methods delegate; documented in the guide and cheatsheet | CORE + DOCS | GitHub invented a head to read fields; SDK26 and SDK29 re-implemented the parser; adopted in 8 earlier SDKs (−110/+85 lines) | semantics unchanged |
| B23-Q | generated endpoint docs name the type that documents a `query(..)` value's keys | MACRO (docs) | SDK27 consumer | uses `type_text` |
| B23-G | root `.gitattributes`: every SDK fixture byte-exact on checkout | repository | native Windows CI failed on FRED/USGS CSV fixtures | — |

## Dispositions of the other candidates

| # | Candidate | Disposition | Why |
|---|---|---|---|
| K | Declared error documents | **KEEP_CORE** (no new statement) | Policies are 12–40 lines; the error vocabulary is the provider's. Revisit if wave 3 shows a mechanic, not a vocabulary. |
| P | Repeated-continuation guard | **Not accepted** (B23-P; prototype `86cd55df`, not merged) | 5 witnesses hand-wrote it; the guard (10 public items) was adopted by 0/4: each SDK pins its provider's error and timing, and the check itself is a set insert |
| F3-3 | Read-modify-write of request values (Intercom's page factory edits `op.search` through a private field) | **EXPERIMENT for W3** | 1 current + 3 earlier witnesses; getters add API to every generated type — needs a compiled comparison |
| F | Followed provider URLs through the policy (Mailgun, Zendesk: `Endpoint::operation()` takes no `self`) | **EXPERIMENT for W3** | needs a design: e.g. "same endpoint, provider URL" on the generated type |
| N | Correlated values across owners (Zendesk's ticket id + body stamp) | **EXPERIMENT** (with SDK23/K8s projection) | 3 witnesses, different shapes |
| X-auth | refresh-token rotation hook | **EXTERNAL, wait** | 1 witness (HubSpot) |
| X-ratelimit | `concord-ratelimit` | **pilot, no further investment** | 0 adoptions in 6 providers; candidate for OMIT at the final retrospective |
| X-retry | a `receive_tokio` convenience | **wait** | 1 witness (Zendesk) |
| codecs | streamed multipart | **EXTERNAL, wait for an unlike witness** | Mailgun only (author + a real maintainer defect in the duplicated native branch) |
| D | decode diagnostics (missing JSON field) | **DOCS** | the detail is in `CodecError::source_unredacted()`; the consumer did not find it |

## Not done, and why

- Live provider calls: NOT_RUN. Native Windows: the CRLF fix is verified only by the next CI run.
- Wave-2 adoption (`b23-w2adopt`, Sol): declared results, field readers and HubSpot's `delivery()`
  merged (production −63/+32 lines over five SDKs; no assertion changed). The lane started before
  the `delivery()` fix; its three new tests pinned "a refused connection is not sent" and were
  rewritten by the coordinator to pin the safe outcome (still uncertain) — see `a46f490a`.
- The follow-up independent review of the fixes and both adoption lanes: `../../roles/b23-review2-report.md`.
