# SDK50 closing record (P3a)

Written 2026-11-07 at close, before PR #5 merges. Organized by concept, not by wave. Source of
every claim: `DECISIONS.md` (decision IDs), the source tree of this branch, and the commits named.
This record is the smaller **P3a** form of the mission's P3 retrospective. **P3b** (adoption and
consolidation) moved to the desirability loop, because deciding adoption is exactly what the
loop's pre-registered, measured hypotheses do (owner instruction 2026-11-07: "finish → merge
PR #5 → loop"). Live provider tier: NOT_RUN throughout (local fixtures only).

## What SDK50 established

- **The corpus.** 55 SDK packages under `examples/*-sdk`: SDK01–50 of the SDK20 and SDK50
  campaigns plus GitHub, Kubernetes, Salesforce, Shopify and Stripe from the 4.4 extensions. Their
  declarations hold 63 clients, 91 scopes and 316 endpoints. Each campaign SDK had an author, a
  fresh docs-only consumer and an independent maintainer (`waves/W*/`, `MODELS.md`). They are
  pinned reference slices of real APIs, not complete SDKs.
- **The core held.** No domain needed a new core concept. Client → scope → endpoint, declared
  values, `SendWith`, `Prepared`/`follow`, `Received::document`, `Error::delivery()`, the codec
  traits and the optional capability crates covered all 55 (`cross50/PROPOSALS.md`, "What Concord
  learned"). Every larger abstraction proposed during the waves failed on a counterexample from
  another domain (below).
- **The largest remaining cost is how the generated interface explains itself.** Fresh consumers
  working from rustdoc mispredicted the base/path boundary (6), the error owner (12) and the send
  result (7), several on the current wording (`cross50/PROPOSALS.md` Tier 1).
- **Shipped capabilities are under-adopted.** `Retry::receive_with`, inherent `operation()`,
  declared result templates, `Error::kind()` and `document::<Binary, Bytes>` already do what
  several SDKs still hand-write (Tier 2).

## Accepted Concord changes

| Concept | Change | Decision |
|---|---|---|
| Response metadata survives failures | Errors raised after a head keep it: `Error::head()`, `Error::head_meta()`; `Incoming::chunk`/`drain` too. `Error::meta()` keeps its narrower contract | F2-1 |
| Value with metadata | `Received::decode_with_meta()` / `decode_limited_with_meta()` | F2-2 |
| Typed document of any status | `Received::document::<F, D>(limit)`: the format's head checks and context, a bounded read, the status kept on every failure | F2-3 |
| Declared bound on finite bodies | `limit EXPR;` on client, scope or endpoint; honoured by generated `send()`, `Received::decode()` and `prepare()?.send()`; refused on streams | B12-A |
| Parameter groups | `params Name(..);` on the client, `..Name` in an endpoint; a generated setter trait per group | F3-1 |
| Run-time query pairs | `query(..) x?: T` with `T: QueryPairs`, pairs at their declared position | B12-F3-4 |
| Generic code through the policy | `concord::Sends<O>` on every endpoint of a client with a send policy | F3-2 |
| The declared send result | `send Policy -> Result<_, Error>;` names every generated `send()` result; plain `send P;` unchanged | B23-A2 |
| Was it sent? | `Error::delivery()` → `NotSent` (refused while building) / `Unknown` (no head) / `Answered` | B23-R |
| Header field readers | `concord::field::{single, unsigned}` on any `HeaderMap`; `ResponseHead::single`/`unsigned`, `FieldError` | B23-C, B12-C |
| Endpoint identity | Inherent `operation()` on generated endpoint handles (a reserved generated name) | B34-O |
| Generated documentation | Policy type, groups, `limit`, `prepare()` bypass named; the `query(..)` key type named; wire shape first | B12-B, B23-Q, B34-D |
| Codecs packaging | One feature-gated `concord-codecs` package; `concord-extras` kept as a compatibility facade; `check_content_type` + `Charset` | X-1, B12-D |
| Pagination helpers | Strict RFC 8288 `Link` parser (feature `link`); `next_offset` + `OffsetError`; `PagesError::provider_error()`/`into_provider_error()`; opt-in `Pages::item_events()` | X-2, B12-E, X-4 |
| Retry | `Retry::receive_with` (async fallible before-attempt hook) | B34-R |
| Repository | Root `.gitattributes`: SDK fixtures byte-exact on checkout (native Windows) | B23-G |

`Encode<T>` (F1-1), per-call format configuration in the payload (F1-2), one-shot bodies
(F1-3), `Decode::Context` (F2-4) and head-only outputs (F2-6) were re-examined and **kept**.

## Rejected proposals and the counterexample that decided each

| Proposal | Decided by |
|---|---|
| Single-entry `EncodeOnce` successor to `Encode` (F1-1) | Compiled comparison: identical bytes; saves one refusing method in 3 formats, costs every format author a second concept; the two-method shape keeps runtime media from being dropped |
| `concord-ratelimit` adoption by Discord (B12-G) | Grew Discord's `limiter.rs` 505 → 625 lines: the crate charges admissions where Discord classifies windows |
| Repeated-continuation guard (B23-P) | 0 of 4 witnesses could adopt it: Mailgun, Zendesk, Intercom and HubSpot each pin a provider-specific error and timing |
| `send P -> Template` inside macro invocations (B23-RV) | Four review rounds: no token rule tells a macro input's type placeholders apart; no witness uses one |
| Stateful `TokenCache` rotation (B34-A) | QuickBooks −10, but Xero +4 and HubSpot +11 lines |
| Declared values readable by policies, getters or `facts` (B34-V) | Getters cannot reach a policy through `Endpoint<T>`; final-wire checks (Adyen encoded segment, Xero tenant header, signing) are not original-value equality |
| Universal host/URL-trust abstraction | Datadog changes parent domains; S3 binds the signing region; B2 issues role-specific hosts |
| Universal list endpoint / generated pagination | Completion, snapshots, links and streamed rows differ; `Sends` + group traits already serve 5 generic walkers |
| Shared provider error enum, "an error field means failure" | Cloudflare keeps diagnostics on `success:true`; MediaWiki rejects warnings; Linear/Shopify keep partial data |
| `bytes_limited_with_meta`, a `DocumentError` stage enum | Already expressible (`document::<Binary, Bytes>`, `Error::kind()`) |
| Universal rotating-token transaction | PayPal uses client credentials; Registry realm challenges; Xero needs durable fail-closed state |
| Rate limits merged into retry, or a default send layer | Discord, Intercom, Shopify and NASA disagree on what a quota is |
| Nested bracket forms in codecs | One SDK (Stripe); would change `Form`'s flat contract |
| Default `Encode::encode` refusal | Saves 8 lines and lets an incomplete encoder compile |
| `SendWith::Output` split into success/error types | Migrates every implementation, saves nothing; envelope projections need the flexibility |

The last ten come from the cross-50 analysis (`cross50/PROPOSALS.md` § Rejected, reports in
`cross50/analysts/`).

## Capability crates: status at close

Adopter counts are `examples/*-sdk` packages whose `Cargo.toml` depends on the crate directly
(re-checked against source at close; the a6 sweep counted at `af6840c9`).

| Crate | Status | SDK adopters |
|---|---|---|
| `concord-pagination` | pre-SDK50 (from the 4.4 extensions, merged to `main` with PR #3); SDK50 added the `Link` parser, `next_offset`, provider-error access and item events | 39 |
| `concord-codecs` | accepted (X-1) | 55 in total: 53 enable it through `concord`'s features, 5 depend on it directly (overlapping; arXiv and USGS use only the direct dependency) |
| `concord-retry` | **pilot** (X-3); its `receive_with` entry point was accepted (B34-R) | 16 |
| `concord-auth` | **pilot** (X-5); merged, no acceptance decision recorded in SDK50; stateful rotation not accepted (B34-A) | 20 |
| `concord-ratelimit` | **pilot, not accepted** (B12-G, B23-RL "no further investment") | 4 (arXiv, Sentry, Strava, Xero) |
| `concord-extras` | compatibility facade over `concord-codecs` | 0 |

Whether the pilots should be accepted, kept as pilots or reduced is loop work (seed H14,
extended to retry and auth).

## Correctness items found at the end, and their resolution

| Item | Resolution |
|---|---|
| HubSpot and Strava kept their client alive through an abandoned token refresh (`Arc<Inner>` captured by the future their `TokenCache` stores) | Fixed `d982f84d` (tests fail before, pass after) |
| Strava held its token mutex across the refresh request; a cancelled refresh blocked every later call | Fixed `18113449` |
| Europe PMC's mandatory 20-page ceiling could be replaced by `.max_pages(n)` | Fixed `18113449` |
| Xero's token journal failed on native Windows (`File::open` of a directory) | Fixed `af48bd1c` in this close; verified only by native Windows CI |
| `Prepared::follow` does not reject same-origin userinfo or fragments | Open: a tightening is a compatibility choice (loop input) |
| QuickBooks `Error::status()` returns only Status-stage codes | Open: needs the intended contract before calling it a defect |

The first three were merged with `448151db`.

## Known limits

- **Model readers, not people.** Every consumer, maintainer and reviewer was a model session
  (Codex `gpt-6.1-sol`/`-terra`/`gpt-6-astra`, Claude Opus 5.5; ledger in `MODELS.md`). Their
  predictions are evidence about the documentation, not human usability results.
- **No live provider calls.** Fixtures are synthetic and labelled (`PROVENANCE.md` per campaign
  SDK; `PROVIDER.md` for the five earlier GitHub, Kubernetes, Salesforce, Shopify and Stripe SDKs).
- **Native Windows.** CI failed on Windows from `2ee1fb2f` to `c1b75b95`; the causes found
  (CRLF fixtures, Xero Unix-only code, `cargo fmt` command length, Xero directory sync) are fixed
  forward. Only the CI run on the final head counts.
- **Open experiments** carried forward without a decision: runtime-selected response
  representation (F2-5), value getters (F3-3), followed provider URLs through the policy (B23-F),
  correlated values (B23-N), redirect visibility to policies (B23-RD).
- **Open SDK items:** ClickHouse drops a received head on a post-head receipt failure (S-1,
  bounded and pinned by a test); OpenAI `incomplete_details` and ClickHouse error-text causes
  (S-3). The rustdoc artifact `for<'__concord> T: Clone` (B12-B) is still printed.

## In-flight work at close

Every local branch with commits not contained in this branch's head `c1b75b95`, classified
(observed with `git rev-list` / `git cherry` at close; of the 60 local `sdk50/*` branches, the 54
not listed here are fully contained):

| Branch (head) | Class | Reason | Goes to |
|---|---|---|---|
| `sdk50/x50-docs` (`8fcf4dcc`) | C | Generated-docs prototype D1–D7, three review rounds; macros only | Loop H04, with the six-run docs trial (below) |
| `sdk50/x50-pages-start` (`0a91b471`) | C | `Pages::starting_at`; 17 `_from` twins removed in 11 SDKs; found start-position holes in Mailgun, Strava, Xero (guarded on the branch) | Loop H09a |
| `sdk50/x50-adopt` (`377da4d2`) | C | SDK adoption of existing API (A1, A2, A4–A6), −191 SDK source lines; changes callers' error types | Loop H11 (and H13's `Error::meta()` build) |
| `sdk50/x50-transport` (`2643d3aa`) | C | `single_attempt_client_builder()` + 22 SDK builders; the independent review leans to a recipe | Loop H10 |
| `sdk50/b34-auth` (`5304d763`) | C | Rotation prototype, already decided not accepted (B34-A) | Evidence only; kept |
| `sdk50/b34-values` (`4fa26231`) | C | Declared-values experiment; both representations rejected by the cross-50 analysis | Evidence only; kept |
| `modular-docs` (`97c80113`) | A | An equivalent patch is already in this branch (`git cherry` reports it upstream) | Nothing |

The owner's **documentation trial** (six fresh docs-only consumer runs: Cloudflare, Kraken and
Sentry, each on the current docs at `15d436df` and on the x50-docs round-3 docs at `fc7bfaa0`)
ran on 2026-11-07. Its worktrees were deleted with the SDK50 workspace; the reports survive in the
session transcripts and Codex rollouts and are imported as H04 evidence by the loop. Observed in
the Kraken new-docs report: `balances(Default::default())` fails with E0283 and the diagnostic
names `::concord::__macro::concord_core::IntoValue` (H03 evidence); the old and new arms did not
face equal base-URL tasks, so their base/path results do not compare like for like.

No branch was classified B (finishing work for this PR) or D (unknown). Nothing was deleted.

## Where the work continues

The desirability loop ([summary](../desirability/SUMMARY.md); records on the
[archive branch](https://github.com/gqsnt/concord_api/tree/archive/desirability-loop-1/v4/desirability)) took the proposals,
prototypes and open items above as hypotheses, each pre-registered with target metrics and kill
conditions, measured across all 55 SDKs by automatic instruments, independently reviewed, and
decided by a fixed rule. Its seed queue started with the send result (H01), a shared result and
error shape (H02), exact input inference (H03) and generated documentation (H04).
