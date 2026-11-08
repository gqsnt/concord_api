# Hypothesis queue (re-ranked under plan amendments A1 and A2)

Mirror of the loop workspace's queue, re-ranked on 2026-11-08 by the owner-adopted review (A1:
task outcomes primary, structural counts diagnostic; H04 first) and the owner's charter (A2: the
fixed core is the DSL usage — client → scope → endpoint — on reqwest; everything else, including
the charter's non-responsibilities, is challengeable). Each entry is a hypothesis, not a decision;
a card (`cards/HNN.md`) is committed before any implementation.

| Rank | ID | Family | Hypothesis (short) | Primary outcome (A1 §2.2) | Notes |
|---|---|---|---|---|---|
| — | H04 | DOCS | Generated docs tell the caller the actual contract — **ACCEPTED (WIN, pilot-3; merged 070ae0ae)** | discover the contract; complete ordinary use | gain concentrated in the base vs appended-path fact; other tasks at ceiling (the other doc changes are unmeasured by this pilot); see `measurements/H04.md` |
| — | H08 | POLICY | An omitted operation classification fails the SDK build — **ACCEPTED (WIN; merged 197d8389)** | change one provider fact (omission detection stage) | 8 SDKs adopt the opt-in `operations` enum; see `measurements/H08.md`, `impact/H08.md` |
| — | H03 | INPUT | Exact inputs infer — **REJECTED (LOSS by its kill condition "any newly rejected caller spelling"; 1 INPUT-family rejection)** | complete ordinary use; diagnose and recover | see DECISIONS; branch `dz/h03` kept |
| 1 | H03b | INPUT | Exact inputs infer under an enumerated, documented compatibility break (K1–K4) | complete ordinary use; diagnose and recover | card `cards/H03b.md` (pre-registered 6ea66f43 before any H03 measurement result was read); a rejection closes the INPUT family |
| 4 | H06 | SURFACE | Generated construction and controls match intended use; scopes cheap to add/remove (no flattening) | trustworthy construction; scope representation | Classify wrappers first (mechanical / provider semantics / safety boundary / residue); counterexamples QuickBooks, S3/Kubernetes rotation |
| 5 | H01 | RESULT | The public send result is visible and accurate (docs/aliases vs a representation change) | discover the contract | after H04 |
| 6 | H02a | RESULT | A reusable value+metadata carrier | complete ordinary use; maintainer cost | split from old H02 |
| 7 | H02b | RESULT | Provider error-document ownership | diagnose and recover | higher semantic risk |
| 8 | H12 | DOMAIN | One-place versioning / shared path prefix without a navigation step; scope cost | change one provider fact (I4 e1) | Mastodon mixed versions must stay expressible; baseline (B=734991e9): I4 e1 version/prefix — missed-propagation flags in 6/8 tasks, all in SDK test expectations or package docs (not production paths); Kubernetes e1 oracle fail 1/4 |
| 9 | H09a | CAPABILITY | `Pages::starting_at` removes `_from` twins | complete ordinary use | Mailgun eager validation |
| 10 | H09b | CAPABILITY | A page whose continuation failed after its records were accepted | diagnose and recover | Kubernetes/Elasticsearch/Discord must still reject |
| 11 | H10 | CAPABILITY | Opt-in single-attempt transport constructor | diagnose and recover (write uncertainty) | same-origin redirect SDKs; opaque supplied clients |
| 12 | H11 | ADOPTION | SDKs adopt what already exists (Error::kind, inherent operation(), receive_with, document) | maintainer cost | provider-specific errors stay |
| 13 | H05 | SENSITIVE | Declarative redaction for SDK-owned types, scoped, paired with diagnosis | safe place (I2) | the I2 fixes show where hand redaction failed |
| 14 | H15 | SENSITIVE/CAPABILITY | Concord-produced observability inside the safe place (redacted events) | safe place; diagnose and recover | from owner item O1 (A2) |
| 15 | H07 | SURFACE | Facade residue after H01–H06 | — | classify by purpose |
| 16 | H13 | RESTRAINT | Remove unused surface with compatibility evidence | concept cost | — |
| 17 | H14 | CAPABILITY | Capability-crate status (records reconciliation) | records truth | largely done in v4/sdk50/CLOSING.md |
| 18 | H16 | REQUEST | Request composition order, encoding and format-implied headers are predictable from the declaration (fixed query pairs vs caller values; path-value percent-encoding; `Accept` from the format) | discover the contract (R1 query/destination) | baseline I3: Claude misses query order on Kubernetes w1/w2/w6 and MediaWiki w1–w3 in both runs, Nager path value encoding; docs or ordering change, to be carded after H04 |
| — | T2 | CORRECTNESS | Four intermittent SDK test failures seen in CI (not caused by H04/H08): PayPal `an_uncertain_authorize_…` (Ubuntu, run 42473297864 on c7c7a983), Snowflake `malformed_or_uncorrelated_partition_links_…` (Windows, run 42474510287 on 60c20ea0), stream-stress `jsonl_upload_late_source_error_…` (Windows, run 42454082871 on 0cd1dc0a; earlier O5), concord `representations::transport_error_message` (Ubuntu multipart-only, run 42479398910 on f533523c; port reuse, seen locally in F3) | correctness repair (A1 §3 path 1) | in progress on `dz/t2-flakes`; tests are not weakened |
| — | T1 | DOCS-TRUTH | `v4/PRODUCT.md` "What Concord is not" states the fixed core and current scope (A2) | records truth | docs-truth path (A1 §3) |
