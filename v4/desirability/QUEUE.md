# Hypothesis queue (re-ranked under plan amendments A1 and A2)

Mirror of the loop workspace's queue, re-ranked on 2026-11-08 by the owner-adopted review (A1:
task outcomes primary, structural counts diagnostic; H04 first) and the owner's charter (A2: the
fixed core is the DSL usage — client → scope → endpoint — on reqwest; everything else, including
the charter's non-responsibilities, is challengeable). Each entry is a hypothesis, not a decision;
a card (`cards/HNN.md`) is committed before any implementation.

| Rank | ID | Family | Hypothesis (short) | Primary outcome (A1 §2.2) | Notes |
|---|---|---|---|---|---|
| 1 | H04 | DOCS | Generated docs tell the caller the actual contract (base/path, actual send result, error owner, decisive exceptions) — docs-only pilot on current signatures | discover the contract; complete ordinary use | Witnesses Cloudflare, Sentry, Kraken; controls Nager, Xero, Strava; 72 sessions, 2 providers; candidate from `sdk50/x50-docs` round 3 |
| 2 | H08 | POLICY | An omitted operation classification is caught early (SDK-local exhaustive mapping vs one narrow opt-in macro design) | change one provider fact (omission detection stage) | I4 e4 omission controls: only 2 of 7 SDKs catch an omission at compile time today |
| 3 | H03 | INPUT | Exact inputs infer (`Default::default()`, `.parse()`, `.into()`) without the sealed `IntoValue<T>` barrier | complete ordinary use; diagnose and recover | I5 caller bank: all 5 `Default::default()` body cases name `concord_core::IntoValue` |
| 4 | H06 | SURFACE | Generated construction and controls match intended use; scopes cheap to add/remove (no flattening) | trustworthy construction; scope representation | Classify wrappers first (mechanical / provider semantics / safety boundary / residue); counterexamples QuickBooks, S3/Kubernetes rotation |
| 5 | H01 | RESULT | The public send result is visible and accurate (docs/aliases vs a representation change) | discover the contract | after H04 |
| 6 | H02a | RESULT | A reusable value+metadata carrier | complete ordinary use; maintainer cost | split from old H02 |
| 7 | H02b | RESULT | Provider error-document ownership | diagnose and recover | higher semantic risk |
| 8 | H12 | DOMAIN | One-place versioning / shared path prefix without a navigation step; scope cost | change one provider fact (I4 e1) | Mastodon mixed versions must stay expressible |
| 9 | H09a | CAPABILITY | `Pages::starting_at` removes `_from` twins | complete ordinary use | Mailgun eager validation |
| 10 | H09b | CAPABILITY | A page whose continuation failed after its records were accepted | diagnose and recover | Kubernetes/Elasticsearch/Discord must still reject |
| 11 | H10 | CAPABILITY | Opt-in single-attempt transport constructor | diagnose and recover (write uncertainty) | same-origin redirect SDKs; opaque supplied clients |
| 12 | H11 | ADOPTION | SDKs adopt what already exists (Error::kind, inherent operation(), receive_with, document) | maintainer cost | provider-specific errors stay |
| 13 | H05 | SENSITIVE | Declarative redaction for SDK-owned types, scoped, paired with diagnosis | safe place (I2) | the I2 fixes show where hand redaction failed |
| 14 | H15 | SENSITIVE/CAPABILITY | Concord-produced observability inside the safe place (redacted events) | safe place; diagnose and recover | from owner item O1 (A2) |
| 15 | H07 | SURFACE | Facade residue after H01–H06 | — | classify by purpose |
| 16 | H13 | RESTRAINT | Remove unused surface with compatibility evidence | concept cost | — |
| 17 | H14 | CAPABILITY | Capability-crate status (records reconciliation) | records truth | largely done in v4/sdk50/CLOSING.md |
| — | T1 | DOCS-TRUTH | `v4/PRODUCT.md` "What Concord is not" states the fixed core and current scope (A2) | records truth | docs-truth path (A1 §3) |
