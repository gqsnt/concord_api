# P3.2 → P3.3 selection (coordinator, 2026-11-09 10:40)

Inputs: 12 independent lanes (6 Claude, 6 Codex) → 111 designs → merged and screened into
`merged/<letter>.md` by three merge lanes: ≈82 entries, 41 KEEP, ≈30 KILL (each with its charter
rule), the rest MERGED. 41 KEEPs exceed what targets and spikes can carry, so the coordinator folds
overlapping KEEPs into one target and defers the smallest ones to the gallery as one-liners (not
killed: they stay pickable).

## Targets (26), each gets three hand-written "after" SDKs (P3.3.1)

| Slug | From | Folded in | Why it is targeted |
|---|---|---|---|
| A1-error-document | A1 | B4 (`status` list) as an option | largest glue category (error.rs ≈11.7k lines; friction A1, 38 SDKs) |
| A2-derive-error | A2 | — | alternative to A1 that keeps each SDK's error enum |
| B1-response-core | B1 | — | 21 `Response<T>` copies; friction B1, 18 SDKs; prerequisite of F2/F3 |
| B2-response-headers | B2 | J2 (`request_id`) | request ids, rate-limit and paging headers as declared facts; feeds E, J |
| B3-envelope | B3 | A's in-2xx refusal | Slack, Kraken, Cloudflare, MediaWiki: the hardest A/B counterexamples |
| C1-fixed-unless-mut | C1 | — | closes FM1, FM2, FM4 without a negation statement |
| C2-declared-base | C2 | M3 (tests reach the declared origin) as the fixture story | closes FM3; the base as a provider fact |
| D1-derive-debug | D1 | D4 (`#[public]` default) as a variant | 1,516 redaction lines; friction D1, 18 SDKs |
| D2-secret-wiretext | D2 | N2 (header values are text), N4 (declared auth scheme) | one text contract for every placement; secrecy carried by the value |
| E1-operation-effect | E1 | N3 (always generate the operation enum, remove `operations Name;`) | read/write as a fact; removes a statement that configures Concord |
| E2-idempotency-key | E2 | — | uncertain-write story for PayPal, Adyen, Square, QuickBooks, Zendesk |
| F1-pages-starting-at | F1 | F4 (one method takes the start) | 18 `_from` twins, 14 SDKs, no DSL change |
| F2-declared-cursor | F2 | F5 (numbered positions) | the cursor location as a fact; friction F1, 17 SDKs |
| F3-next-url | F3 | — | provider-issued next links followed through the policy (GitHub, DigitalOcean, Mailgun, PokeAPI) |
| G1-read-accessors | G1 | G8 (publish the generated tree) | typed read access to declared values; friction E1, 16 SDKs; lets wrappers go |
| G2-from-url | G2 | — | returned URLs recognized as declared endpoints (Salesforce, PokeAPI, Registry) |
| H1-request-line | H1 | K6 (query order is declaration order) | owner: generated docs are fixed in this phase; hard word budget |
| I1-wire-display | I1 | — | 89 delegating `Display` impls + 11 spelling macros, 32 SDKs |
| K1-declared-ranges | K1 = I2 | — | 463 lines of bounded-integer newtypes in 15 SDKs |
| I3-exact-inputs | I3 | I6, I7 | inference for callers; the owner may accept enumerated breakage |
| J1-concord-logs | J1 | J4 (span variant), J3 (`summary()`) | owner's criterion: Concord logs itself without leaking |
| K2-route-facts | K2 | H3, M4 (composition snapshot) | one route fact feeding docs, logs and snapshot tests |
| L2-inline-path | L2 | L1 (one line per short endpoint) | declaration lines per endpoint |
| M1-concord-test | M1 | M2 (shared failure scenarios), M5 | 1,750 + 1,300 copied test lines |
| M3-declared-origin-tests | M3 | — | tests without `with_base_url`; makes C2 possible |
| N1-delete-unused | N1 | — | `concord-extras`, `first_page_then`, `Error::meta()`, V3 diagnostics; usage check only |

## Deferred to the gallery as one-liners (KEEP, not targeted this round)

B4 `status 200, 202;` (folded into A1 as an option), D3 `concord::Link` (overlaps F3/J1), G3 readable
build refusals (`Error::invalid_value()`, ≈50 lines), G4 nest by context (an SDK refactor, no
Concord change), I4 initialized bodies (≈10 lines), J3 `summary()` (folded into J1), K3
multi-segment path values (Registry, ≈25–30 lines), L1 layout-only one-liners (folded into L2), M4
composition snapshot (folded into K2).
