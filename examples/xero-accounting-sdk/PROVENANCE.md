# Provenance — SDK35

Authoring/retrieval date: **2026-11-07**. Frozen baseline: `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`. Classification: **example-assisted** (required fixture infrastructure and documentation excerpts). Live provider calls: **NOT_RUN**. No Xero credentials, connections, invoices or token endpoints were contacted; only read-only documentation research and loopback fixture traffic occurred.

## Official provider sources

All rows were read on 2026-11-07. Initial direct opens of the three capsule URLs returned the JavaScript-only shell; targeted searches supplied the official pages' indexed text. This is documented research from Xero's pages, not a claim that a live API was tested. Fixture values are synthetic and no full documentation example is copied.

| Key | Exact page | Applied facts |
|---|---|---|
| XR | [HTTP requests and responses](https://developer.xero.com/documentation/api/accounting/requests-and-responses/) | XML default, JSON opt-in, paged resource shapes, optional pagination envelope, page size limits, summarizeErrors behavior. |
| XT | [Token types](https://developer.xero.com/documentation/guides/oauth2/token-types/) | Tenant header, Bearer access, offline refresh, Basic/form refresh request, returned pair rotation and saving both; documented grace period deliberately unused. |
| XS | [Scopes](https://developer.xero.com/documentation/guides/oauth2/scopes/) | Offline scope and current granular invoice read/write permissions; legacy broad invoice permissions accepted for existing grants. |
| XI | [Invoices](https://developer.xero.com/documentation/api/accounting/invoices?emailbeta=true%3Femailbeta%3Dtrue) | GET collection/detail, POST draft, ACCREC, ContactID-only request, lines, DRAFT, page/pageSize, validation envelope. Indexed URL returned by research, retained exactly. |
| XE | [HTTP response codes and errors](https://developer.xero.com/documentation/api/accounting/responsecodes) | All listed HTTP error classes, ApiException-shaped JSON, text maintenance/offline responses. |
| XL | [OAuth API limits](https://developer.xero.com/documentation/guides/oauth2/limits/) | Five concurrent calls, 429, Retry-After minimum and tenant quota distinction. Quota counts are not hardcoded. |
| XO | [OAuth troubleshooting](https://developer.xero.com/documentation/guides/oauth2/troubleshooting/) | Invalid grant and missing/incorrect/unauthorized tenant behavior; reauthorization response. |

Additional official indexed search excerpts encountered but not used as endpoint contracts: [changelog](https://developer.xero.com/changelog), [payments](https://developer.xero.com/documentation/api/accounting/payments), [contacts](https://developer.xero.com/documentation/api/accounting/contacts?source=post_page---------------------------%3Fsource%3Dpost_page---------------------------), [credit notes](https://developer.xero.com/documentation/api/accounting/creditnotes?source=post_page---------------------------), and [PKCE flow](https://developer.xero.com/documentation/guides/oauth2/pkce-flow/). These did not expand the slice.

## Fixture derivation

Every fixture is **authored synthetic**, shaped using the sources above. UUIDs, invoice numbers, descriptions, tokens, messages and accounting amounts are invented. Expected wire paths, headers, methods and required fields come from the cited contracts, independently of SDK serialization. `create-request.json` is the independently authored expected request document. Tests compare structure and also assert exact decimal bytes.

| Fixture | Derivation and purpose |
|---|---|
| `page-modern-1.json` | XR/XI envelope, two synthetic records across size-one pages, lower-case `pagination`. |
| `page-modern-2.json` | Same independent scenario, final page; synthetic capitalized `Pagination` compatibility variation, not claimed as canonical provider casing. |
| `page-legacy.json` | XI older invoice envelope without metadata; one record on a size-two request tests short-page continuation. |
| `empty.json` | XR/XI empty legacy collection establishes exhaustion. |
| `invoice.json` | XI individual invoice fields and line shape, chosen first list record. |
| `create-request.json` | XI minimal sales draft: fixed Type/Status, existing ContactID, one line; exact 12.34 numeric token. |
| `draft.json` | XI single created invoice in DRAFT, reused for GET state in the fixture workflow. |
| `rotated-token.json` | XT JSON refresh response and XS scopes, wholly synthetic access/refresh strings. |
| `validation.json` | XE error number/type/message/elements shape with synthetic validation text; reused as a synthetic body under all tested error statuses (not a claim that each status emits this body). |
| `validation-200.json` | XR/XI per-invoice validation fields; synthetic 2xx defensive error case even though requests opt into summarized errors. |
| `oauth-error.json` | XO invalid_grant shape with invented description. |
| `wrong-media.xml` | XR default XML shape, synthetic minimal refusal body. |
| `malformed.json` | Synthetic broken JSON containing a leak sentinel; intentionally invalid. |

Generated cases in tests are also synthetic: repeated/missing Content-Type, oversized whitespace bodies, chunked prefixes disconnected before EOF, redirects/hangups, wrong paging counts/status/ID, changed token scope/type/lifetime/syntax, all HTTP statuses, and malformed/duplicate Retry-After. None are captured provider responses. The filesystem failure uses an authored directory obstruction so the real `rename` fails after `pending.json` is fsynced. The crash tests retain `Refreshing` or a stale exclusive lock and assert no old-token replay. Test expectations derive from contracts and the explicit fail-closed design, not from recording the SDK's output as golden data.

## Files read and example assistance

Read repository `README.md`, `docs/QUICKSTART.md`, all of `docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`, `docs/RECIPES.md`; inherited campaign `AGENTS.md`, `CLAUDE.md`, `START_HERE.md`, `DESIGN_CHARTER.md`, `AUTHORITY.md`, `PROMPT.md` and `contracts/RESOURCES.md`. The capsule's bounded ownership and frozen baseline supersede broader campaign instructions. No worktree-local AGENTS/CLAUDE file was present.

Read READMEs of `concord-core`, `concord-auth`, `concord-pagination`, `concord-retry`, `concord-ratelimit`, `concord-codecs`. Source rustdoc read for the APIs used: `crates/concord/src/lib.rs`; core `prepared.rs` Endpoint/SendWith/Decoder, `response.rs` Received/document/decode/meta, `error.rs` delivery/error kinds; auth `lib.rs`, `cache.rs`, `credentials.rs`; pagination `lib.rs`, `types.rs`, `pages.rs`; retry `lib.rs`, `concord.rs`, `http.rs`; ratelimit `src/lib.rs`; codecs `lib.rs`, `json.rs`, `form.rs`. Also inspected root/crate manifests, lockfile, resource capacity and baseline/status. Reading was done in bounded chunks after initial combined output truncated.

Every directly opened example file:

| File | What was taken |
|---|---|
| `examples/testserver/src/lib.rs` | Required TestServer/Recorded/Reply API, request inspection, gates, failing chunked bodies, owned shutdown; dependency used unchanged. |
| `examples/testserver/Cargo.toml` | Package name/path and already available Tokio fixture features. |

No existing `examples/*-sdk` source file was opened directly. The required guides contain embedded example excerpts; these were read, so assistance is also recorded explicitly:

| Embedded source (through guides only) | Use |
|---|---|
| `examples/quickstart/src/lib.rs`, `examples/quickstart/src/main.rs` | Declaration/config/scope/send and local-base construction style; no business models reused. |
| `examples/shapes/tests/wire.rs` | Loopback/domain resolution example read; not used. |
| `examples/fred-sdk/src/lib.rs` | Explicit no-redirect/no-retry/referer transport pattern adopted through AUTHOR_GUIDE. |
| `examples/frankfurter-sdk/src/lib.rs` | Generic send policy with bounded `document` and boxed provider failure, through AUTHOR_GUIDE. |
| `examples/native/src/native_edit.rs` | Prepared native boundary and decoder preservation, through RECIPES; no path rewriting adopted. |
| `examples/compat/src/lib.rs` | Multi-segment key escape example read; not used. |
| `examples/openai-sdk/src/lib.rs`, `examples/google-drive-sdk/src/upload.rs` | Multipart excerpts read in AUTHOR_GUIDE; not used. |
| `examples/native/src/repr.rs` | Custom-format behavior described by RECIPES; no custom codec copied or needed. |

Guide references to other files without code inspection are references, not additional source consultation. No generated expectations were copied from another SDK.

## Dependencies and ownership

All dependencies already existed in Cargo.lock: Concord family 4.2.0, serde 1.0.229, serde_json 1.0.154, Tokio 1.54.1 and fixture package 0.0.0. No new third-party crate was added. `serde_json/raw_value` is an additional feature on the existing MIT OR Apache-2.0 crate, needed to emit exact two-decimal JSON numbers without float arithmetic; Concord `form` uses the already-locked serializer. No existing package version changed.

Only this package, its root workspace member string and its new lock package stanza are changed. No shared crate, checks, script or other example was edited. No commit was attempted. Runtime skill read: `cloud-environment:cloud-environment-runtime`; its environment-status tool was absent, so no managed-environment ID or readiness claim was invented. No agents were delegated.
