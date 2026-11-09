# P3.3.1 target files — verdicts

Each target lane (Codex `gpt-6.1-sol` xhigh, workspace-write, no compiling) wrote the *after*
version of a witness, a counterexample and a simple control. Glue counts are the lane's hand counts
(net non-blank lines); "reads better" is the lane's own answer, checked by the coordinator.

| Target | Witness | Counterexample | Control | Reads better? | Verdict |
|---|---|---|---|---|---|
| A1-error-document | Registry −45 | S3 −2 | Nager 0 (identical) | yes: one line shows the failure shape | **spike** (largest glue family; witness gain is lower than lanes estimated, the spike measures more SDKs) |
| A2-derive-error | Salesforce −55 | Discord +3 | Frankfurter 0 | declaration identical; Rust shorter | **spike** (cheap library derive; alternative to A1) |
| B1-response-core | Notion −15 | Slack 0 | Nager 0 | declaration identical | **fold** into the response direction (a prerequisite for F2/F3, too small alone) |
| B2-response-headers | Stripe −44 | Discord +21 | PokeAPI 0 | yes: the response contract is visible | **direction only** (mixed: exact-grammar headers cost more Rust) |
| C1-fixed-unless-mut | Mastodon 0 | QuickBooks 0 | Nager 0 | yes, narrowly: `mut` makes replacement explicit | **direction** (no glue alone; closes FM1, FM2, FM4; needs C2 + G1 to delete wrappers) |
| C2-declared-base | Salesforce −1 | Datadog +31 | Frankfurter 0 | yes: a real instance replaces a fictitious host | **direction only** (alone it adds Rust; with C1 + G1 it deletes wrappers) |
| D1-derive-debug | Salesforce −126 (net −100) | Twilio −27 | PokeAPI 0 | declaration identical; Rust secrecy marked on fields | **spike** (clearest glue win) |
| D2-secret-wiretext | Shopify −33 | S3 −12 | Nager 0 | yes: a typed secret instead of `String` | **spike later** (modest; pairs with D1) |
| B3-envelope | Slack −110 | Kraken −23 | Frankfurter 0 | yes: envelope stated once | **direction** (response: needs A1 + B1; alone +1/−2/0) |
| E1-operation-effect | Strava −10 | Kraken 0 (signed POST reads stated once) | PokeAPI 0 | yes | **spike** (removes `operations Name;`, a statement about Concord) |
| E2-idempotency-key | PayPal −20 | Adyen 0 | Frankfurter 0 | yes, narrowly | **direction** (operations) |
| F1-pages-starting-at | GitHub −7 | Discord −4 | PokeAPI 0 | declaration identical | **direction** (operations/paging; small, no DSL change) |
| F2-declared-cursor | Stripe −13 (+11 DSL/explanation: net −2) | Mailgun 0 | PokeAPI 0 | — | **dies at target**: two lines net for seven new concepts |
| F3-next-url | GitHub −133 | DigitalOcean −23 | PokeAPI 0 | yes: the next-page source is visible | **spike** |
| G1-read-accessors | Datadog −23 | QuickBooks 0 | Nager 0 | declaration identical | **direction** (construction: lets wrappers go with C1 + C2) |
| G2-from-url | Salesforce +15 | Registry 0 | PokeAPI 0 | no | **dies at target**: grows glue on its own |
| H1-request-line | FRED −8 doc lines | ClickHouse 0 | Nager −6 doc lines | generated docs shorter; declaration identical | **spike** (owner: generated docs are fixed in this phase; the budget is the measure) |
| I1-wire-display | GitHub −48 | Stripe −88 | Frankfurter 0 | declaration identical; value code shrinks | **spike** |
| I3-exact-inputs | Cloudflare 0 | renamed 0 (one `.clone()`) | Nager 0 | no | **gallery only** (no glue; caller inference measured by H03b: +18.6 points) |
| K1-declared-ranges | GitHub −31 | Mailgun −22 | Nager 0 | mixed: GitHub yes, Mailgun's `NonZeroU16` range no | **spike** |
| L2-inline-path | Kubernetes 0 | MediaWiki 0 | Nager 0 | yes, slightly | **gallery only** (no glue, no lines saved here) |
| J1-concord-logs | GitHub app 0 | S3 −12 (`Debug` 17 → 5) | Nager 0 | declaration identical | **spike** (owner's criterion: Concord logs itself without leaking; a capability, not glue) |
| K2-route-facts | Mastodon −5 | Discord −1 | PokeAPI 0 | — | **dies at target**: six lines for about eight new concepts |
| M1-concord-test | GitHub tests −49 | S3 tests −12 | Frankfurter tests −41 | declaration identical | **spike later** (test code only; production glue 0) |
| M3-declared-origin-tests | Zendesk tests −11 | S3 −2 | PokeAPI tests +14 | declaration identical | **direction** (construction: the fixture story that lets the base be fixed) |
| N1-delete-unused | consumers 0 | first_page_then / Error::meta users 0 | Nager 0 | declarations unchanged | **spike** (removal; usage check + review only) |

Spike order: D1, A2, A1 (running/done) → I1, F3 → E1 (+N3), H1, J1, K1, N1, D2 → directions
(response A1+B1+B3+B2; construction C1+C2+G1+M3; values and secrets D1+D2+I1+K1; operations and
paging E1+E2+F1+F3).
