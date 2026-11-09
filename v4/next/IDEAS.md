# Idea families (seed)

Each family states a problem with evidence, witness SDKs with a lot of glue, counterexamples that
any design must survive, and seed ideas. **The seeds are a starting point, not the answer.** P3.2
lanes generate their own, and P3.1 may add families.

The numbers come from the scoreboard at `734991e9` → `f178e63d`. They are static heuristics, and
P3.1 replaces them with the glue inventory.

Simple controls for every family: **Nager, PokeAPI, Frankfurter.** Their declarations must not
grow (charter rule C3).

---

## A. Errors: the provider's error document as a fact

**Problem.**
- 45 SDKs have an `error.rs`, ≈11.1k non-blank lines in total.
- 54 `impl From<concord::Error>`.
- Each SDK re-models "status + provider error body + concord error", with its own enum, `Display`,
  accessors and redaction.

**Witnesses.** distribution-registry (750 lines), amazon-s3 (594), salesforce (582),
backblaze-b2 (501), anthropic (440), quickbooks (428), elasticsearch, discord, notion.

**Counterexamples.**
- Slack: HTTP 200 with `ok:false`.
- Cloudflare: diagnostics on success.
- S3 and B2: XML error bodies.
- Snowflake: 202 means "still running".
- Kraken: errors inside a success envelope.
- GraphQL-style APIs (Linear) with `errors` next to `data`.
- Callers that `match` on SDK error variants.

**Seeds.**
1. `error ProviderError;` on a client or scope: Concord decodes non-2xx bodies into it, using the
   same format rules. The send result becomes `concord::Error<ProviderError>`, with `.provider()`,
   `.status()` and `.kind()`.
2. Per-status error documents (`error 4xx => ApiError; error 429 => RateLimited;`).
3. Errors as an associated type of the format or of the scope, so XML providers get XML errors
   without new syntax.
4. A derive that generates the SDK error enum from a declaration (`#[derive(concord::Error)]`), so
   the SDK keeps its own type but loses the boilerplate.
5. "Envelope errors" (Slack, Kraken): the error fact sits inside a 200 body. A declared rule says
   when a success body is an error (see family B).

## B. Results: envelopes, value plus metadata, the `_` placeholder

**Problem.**
- 20 SDKs define the same `pub struct Response<T> { value, meta }`: adyen, airtable, anthropic,
  digitalocean, dropbox, gitlab, google-drive, intercom, kraken, linear, mailgun, notion, openai,
  pokeapi, sentry, slack, strava, stripe, twilio, usgs.
- Envelopes are unwrapped by hand.
- `send P -> Result<Response<_>, Error>` repeats in many SDKs with small variations.
- In Kraken, `_` stands for the whole envelope rather than the public value.

**Witnesses.** Slack, Kraken, Cloudflare, MediaWiki, Stripe, Notion.

**Counterexamples.**
- APIs whose envelope carries pagination or diagnostics the caller needs.
- Endpoints that return no body.
- Streaming endpoints (SSE, JSONL).
- Generic code over `Sends<O>`.

**Seeds.**
1. `concord::Response<T>` in core (value plus `ResponseMeta`), selectable per client with one word.
2. `envelope Data<_>` / `unwrap "result"`, a declared success envelope where `_` is the inner value
   and the envelope stays reachable.
3. A result shape declared once on the client that applies to all endpoints (it already partly
   exists through the send template). Remove the repetition.
4. Make the generated `send()` return type readable in rustdoc without `<P as SendWith<…>>::Output`.

## C. Construction: what is fixed and what may change (replaces `controls`)

**Problem.**
- Every client parameter gets public generated setters (`set_token`, `set_policy`), and every
  client gets `with_base_url`.
- Some of these break SDK invariants. FM1–FM4 in `v4/next/inputs/CONSTRUCTION_PROBLEM.md`: for
  example, Mastodon's token can be sent to another instance, and a Shopify credential can be
  separated from its ledger.
- About 22 SDKs wrap the generated client in their own struct, partly to control construction.
- The `controls { … }` patch was reverted because it lists things to turn off (rule C1).

**Witnesses.** Mastodon, Shopify, Zendesk, Salesforce, QuickBooks, Xero, DigitalOcean, Datadog,
Discord, PayPal (whose generated `new` cannot be constructed).

**Counterexamples.**
- QuickBooks: coordinated realm and credential replacement.
- S3 and Kubernetes: credential rotation.
- ClickHouse and Kubernetes: useful defaults.
- Tests and fixtures need a different base URL.
- Proxies.

**Seeds.**
1. **Fixed by default.** Client parameters are fixed at construction, with no setters unless the
   declaration says a value may change (`token: Token` vs `mut token: Token`, or `#[rotates]`).
   That is a fact about the provider's credential model.
2. Base selection as a declared fact: `base "https://{instance}"` with `instance` fixed;
   `with_base_url` only where the declaration allows a replaceable base. Test fixtures use a
   separate, explicit, test-only constructor (`#[cfg(test)]` or a `fixture` feature).
3. Grouped values that change together: `credential (realm, token)` replaced as one unit.
4. Generated constructors that take the SDK's validated types, so wrapper structs are not needed.

## D. Secrets inside SDK types

**Problem.**
- 361 hand-written `Debug` impls (2,954 lines) and 314 `Display` impls.
- About 164 of them visibly withhold fields.
- `#[sensitive]` covers declared values only.
- The canary tests found 7 leaks, all in SDK-owned types.

**Witnesses.** Salesforce (34 impls), S3, Elasticsearch, Shopify, Xero, Registry, ClickHouse,
GitHub, Twilio, Mailgun.

**Counterexamples.**
- Types that must show some fields, such as Twilio `Message` showing `sid` and `status`.
- Identifiers that are public by design (Europe PMC IDs, the ClickHouse username).
- Types holding native values.

**Seeds.**
1. `#[derive(concord::Debug)]` with `#[sensitive]` on fields, using the same withholding style as
   Concord's own output.
2. Sensitivity flowing from the declaration: a model type used as a `#[sensitive]` body or header
   value gets redacted `Debug` automatically, or through a marker trait.
3. A `Secret<T>` value type in core, so it is withheld everywhere; this is the S3 signing-key
   case.
4. Canary helpers as a public testing utility for SDK authors (rule C9).

## E. Operation facts: read, write, idempotent, uncertain, paged

**Problem.**
- H08 made the operation enum opt-in (`operations Name;`, 8 SDKs). Behavior then classifies it in
  Rust.
- But each SDK still hand-writes tables of the same facts: write vs read, idempotent, uncertain
  after a timeout, paged.
- The `operations` statement is itself ceremony.

**Witnesses.** Strava, Sentry, Kraken, PayPal, Zendesk, Datadog, Square, GitHub.

**Counterexamples.**
- Kraken: POST reads.
- Search endpoints that are POSTs (Square, Datadog).
- Adyen's identity of the final request on the wire.
- Policies that ignore operations.
- `#[cfg]`-gated endpoints.

**Seeds.**
1. Always generate the operation enum. Remove the statement, and make it available to every
   policy.
2. Declarative operation attributes: `#[idempotent]`, `#[uncertain]`, `#[read]` on a POST. The
   policy reads them as typed constants.
3. Operation groups declared on scopes (everything in `scope search` is a read).
4. Let capability crates (`concord-retry`) read the declared facts directly, so most SDK
   classification disappears.

## F. Paging facts

**Problem.**
- 11 SDKs carry `_from` twin methods (17 pairs).
- H09c added docs to explain `Resume` instead of simplifying it.
- The cursor location (query, body, `Link` header, response field) is restated by hand.

**Witnesses.** GitHub, Notion, Discord, Stripe, Mailgun, DigitalOcean, S3, Intercom, Graph.

**Counterexamples.**
- Kubernetes: snapshot consistency.
- Elasticsearch: partial results.
- Discord: unordered pages.
- Mailgun: eager cursor validation.
- A continuation that fails after records were accepted.

**Seeds.**
1. `page cursor(query "starting_after" <- response.data.last.id)`: the cursor location as a
   declared fact. `concord-pagination` drives it.
2. One paged method that takes an optional starting position, with no twins.
3. `Link`-header paging as a format-level fact.

## G. Facades and naming

**Problem.**
- 98 hand-written forwarders in 32 SDKs (Registry 9, Drive 9, Salesforce 9, OpenAI 7,
  Elasticsearch 6).
- 22 wrapper structs.
- The generated surface is not the final SDK.

**Method.** First classify every forwarder by cause: construction, result shape, paging,
validation, naming, provider semantics. Most should disappear through A–F. Address the remainder
here.

**Seeds.**
1. Endpoint aliases or renames in the declaration.
2. Typed validated inputs as declared types, instead of wrapper methods.
3. Re-export layout conventions, so `crate::Error`, `crate::Resume` and so on are where callers
   expect them. The H09c baseline misses were all import-path errors.

## H. Documentation: short, from the declaration

**Problem.**
- Generated docs grew from 544 to 889 words per endpoint during the loop, written to pass model
  readers.
- Sentences such as "Query keys keep their first-appearance slots …" break rule C6.

**Seeds.**
1. A hard budget, for example ≤ 150 words per endpoint on the caller path.
   - First line: what the endpoint does (from the author's doc).
   - Then one line with method and URL template.
   - Then inputs, then `send()` result and error.
   - Composition details go on one shared page, linked.
2. Show the declaration excerpt itself in rustdoc (the declaration is the doc).
3. Remove the 76 SDK-local notes that H16s added, unless they state a provider fact.
4. Where docs had to explain a rule (query order, encoding), ask whether the rule should change
   instead (family K).

## I. Values and inputs

**Problem.**
- `impl IntoValue<T>` blocks inference for `Default::default()`, `.into()` and `.parse()?`, and
  its compile error names Concord internals.
- H03 was rejected on its kill condition. H03b gained +18.6 points against a threshold of 20 (kept
  current).
- The owner now decides trade-offs; this family may accept enumerated breakages if the gallery
  shows they are worth it.

**Counterexamples.** `&str` passed to aliases of `String`; generic `V: IntoValue<u16>` forwarders;
borrowed `HeaderValue`.

**Seeds.**
1. Hybrid exact inputs (H03 A′), presented honestly with the breakage list.
2. Exact types plus `impl Into<String>` only for string-like values.
3. Diagnostics-only fix: `#[diagnostic::on_unimplemented]` on `IntoValue`, so the error names the
   expected type.

## J. Observability: Concord logs safely

**Problem.**
- The owner's criterion is "let Concord log itself and be sure secrets are not logged". There is
  no mechanism today.
- Concord knows every sensitive value, so it is the right layer to log.

**Seeds.**
1. An optional `tracing` feature: one span per send (operation name, method, redacted URL, status,
   duration, attempt), with the same withholding as `Debug`, tested by the canary harness.
2. A redacted request/response rendering API that SDKs and apps can log.
3. Policy hooks that log through the same redaction.

**Counterexamples.** Zero cost when off; no secrets when on, including path holes and host labels.

## K. Domain facts and request composition

**Problem.**
- A version literal is repeated in 18 endpoint paths across 8 SDKs (Mastodon 7).
- Query order and percent-encoding of path values surprised readers (H16), and were fixed only
  with documentation.

**Counterexamples.**
- Mastodon mixes `api/v1` and `api/v2`.
- A scope must keep meaning navigation.
- The order of fixed query values (Kubernetes `watch=1`, MediaWiki `list=`).

**Seeds.**
1. A path prefix group without a navigation step (`prefix "api/v1" { … }`).
2. Make query order follow declaration order everywhere (fixed values where they are written),
   instead of documenting slot rules.

## L. Declaration syntax and conventions

**Problem.**
- 154 of 279 sampled endpoint bodies contain only a `path` statement.
- 42 paths are exactly the endpoint's name.
- `send policy -> Result<_, Error>` and similar lines repeat across SDKs.

**Seeds.**
1. Inline path: `GET tasks "tasks" -> Vec<Task>;` or `GET tasks -> Vec<Task> at "tasks";`, with no
   block when the endpoint only has a path.
2. Path defaults to the endpoint name, as a visible, deterministic convention (rule C4). Decide
   whether it is visible enough.
3. A one-line form for endpoints and scopes, keeping blocks for the complex cases.

**Counterexample.** Rule C4: a reader must still predict the URL from the declaration alone.

## M. Testing story for SDK authors

**Problem.** Every SDK hand-builds fixtures, test servers and wire assertions. Concord already has
`prepare()` and `examples/testserver`.

**Seeds.**
1. A public `concord-test` crate: a fixture server, wire assertions on `prepare()`, and canary
   helpers (with D4).
2. Snapshot of the request composition per endpoint (method, URL, headers) as a generated test.

## N. Removal and simplification

**Problem.** The surface accumulated over the campaigns.

**Candidates** (check usage first):
- `concord-extras`, which is now a compatibility wrapper;
- capability crates' status (`concord-auth`, `-retry`, `-ratelimit` are pilots);
- `first_page_then`;
- deprecated `Error::meta()`;
- statements that ideas from families A–L make unnecessary.

**Rule.** Removing something that has no users needs no experiment. It needs a usage check and a
review.
