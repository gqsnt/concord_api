# Findings and placement ledger

One row per shared decision or finding. Dispositions: IMPLEMENT, SDK_ONLY, DOCS_ONLY,
DEFER_WITH_REASON, REJECT_WITH_REASON. Source review findings F1–F7 are in the handoff
(`reference/Developer_Interface_Review.md`).

## Phase A

### D-A1 — response boundary: owned `Received<T>` (IMPLEMENT, core)

- **Reproduction:** F1 — GitHub `send_edited`/`send_conditional`, Stripe `send` (fabricated
  `status: 0` in an impossible branch), Salesforce `send_with_usage`, Shopify `ShopContext::send`
  each re-implement `into_parts` → `execute` → `Decoder::finish(Err(e))`/`decode`. F2 — the
  Kubernetes watch and Salesforce result readers need identity passed as strings (`open_for`).
- **Compared** (`v4/sdk20/a1-boundary/`, compiled against the candidate's public API): the owned
  state, and the smallest head-guard alternative `send_observed(prepared, |head| -> Result<M, R>)`
  built over `into_parts`. The guard covers metadata pairing and a refusal of a 2xx head, but
  (a) every caller handles a `Refused` arm even for an infallible observer, (b) a provider document
  for a non-2xx goes through a second error path, and (c) it decodes with the finite decoder, so
  the incremental path needs a second helper, keeps the bodyless declaration (whose `send()`
  drains) and still names the reader with a duplicated string, because the guard never exposes the
  identity. It does not satisfy A1's identity-transfer clause; the owned state does with one public
  type and three steps (`receive`, `head`, consume once).
- **Selected:** `Prepared::receive() -> Received<T>`; `send()` is `receive()` + `decode()` (one
  code path). Natively edited requests: `Decoder::execute`; synchronous `Decoder::transport_error`.
  Identity: public `Operation` (declared name + method, or a static label); extras sees only
  `Operation::name()`, never `EndpointMeta`.
- **Ownership:** each consumer takes `self` (compile-fail doctest in `response.rs`); dropping
  releases the response; no implicit drain or retry; `Debug` prints operation, status, version and
  header names only.
- **Metadata:** SDK-owned values parsed from `head()` before the consumer; Concord copies no header
  (no header snapshot on the ordinary path).
- **Cost:** allocation counts unchanged on all 94 rows of `concord-bench --bin alloc-count`
  (base `a95fb2fb` vs `d83b4782`).
- **Compatibility:** additive; `send`, `send_raw`, `into_parts`, `Decoder::{new, drain, decode,
  finish}` unchanged in signature and behaviour. `Error`'s internal identity became `Operation`
  (Display unchanged for declared endpoints).

### D-A2 — runtime media: defaulted `Encode::encode_body` + `EncodedBody` (IMPLEMENT, core + extras)

- **Reproduction:** F3 — `checks/dx03-csv-runtime` declared the upload bodyless and applied
  `Upload` to the native request; `examples/multipart` likewise for a per-body boundary.
- **Competing option:** a separate `EncodeRuntime<T>` trait. Rejected: the macro would have to pick
  a trait per body (it knows no format), and a blanket bridge between the two traits conflicts with
  custom formats that implement both.
- **Selected:** a defaulted hook on `Encode` (existing implementations unchanged; static
  `CONTENT_TYPE` still compile-time validated), and a pass-through `Encoded` format in extras for
  `EncodedBody` / `Upload`. Precedence table in `EncodedBody` docs and `v4/SEMANTICS.md` §7:
  strict byte equality, contradictions are `Build` errors before I/O, `remove content-type` keeps
  the runtime value, absent optional bodies send none.
- **Second consumer:** a generated-boundary `multipart/form-data` codec
  (`checks/phase-a/tests/bodies.rs`, known length → `Content-Length`).
- **Found while implementing:** `IgnoreContentType<F>` forwarded only `encode`, which would have
  dropped `F`'s runtime media; it now forwards `encode_body`. Custom wrapper formats in SDKs must
  do the same (checked during migration).

### D-A3 — truthful incremental outputs: `-> stream T` / `-> stream` (IMPLEMENT, core + macros + extras)

- **Reproduction:** F2 — Kubernetes watch and Salesforce result endpoints were declared without
  output; their generated `send()` drained the body. F4 — readers had no standard `Stream`.
- **Runtime path first:** `Incoming` + readers opening `Openable` (`Incoming`, `Received<T>`,
  `reqwest::Response`) with automatic identity; `RecordStream` (one boxed future per record read,
  reader moved in and out, `Unpin`, cancellation-safe, no read-ahead); `completion()`.
- **Declaration opt-in:** `stream` keyword after `->` (not when it begins a path or a generic
  type). `-> stream T` needs `F: Incremental<T>` (new core trait, the streaming counterpart of
  `Decode`, not a `Decode<Stream<..>>`); `send()` returns `Records<F, T>`. `-> stream` returns the
  unread `Incoming` for runtime-configured readers. No generated method was added (no new
  collision surface); `send`/`prepare` keep their names.
- **Known tradeoff:** a finite format used for `-> stream T` is reported once per signature that
  names the reader type (six identical-message E0277 errors in `ui_json/stream_outputs.stderr`)
  where a finite mismatch is reported once.

### D-A4 — ordinary pagination facade (IMPLEMENT, concord-pagination)

- **Reproduction:** F5/F6 — Stripe returns a factory because `Sequential` cannot be named;
  `Sequential::next` returns `None` on exhaustion and on a budget stop alike; dx03 stage 2 clones
  `discovery.resume` and `mem::take`s errors before `into_rerun`.
- **Selected:** `Pages`/`PageItems`/`ItemStream` (engine erased behind a boxed trait object, no
  per-page allocation beyond the engine's own; `Send + 'static` factories), typed `PagesError`
  (`Provider`, `Bounded`, `Ended`), `Incomplete` for `collect_complete`; lossless `into_resume` on
  `JobsReport` and `IndependentReport`. No hidden cap; exhaustion is checked before a cap reached on
  the last page. Engine semantics unchanged; `into_rerun` documented as lossy.

## Gate A independent consumer (`checks/consumers/phase-a-dx`, Codex `gpt-6.1-terra` xhigh)

Unassisted first use from public docs and rustdoc; no implementation source read (its
`REPORT.md`). All four outcomes completed (9 tests). Findings and dispositions:

| ID | Observation | Attribution | Disposition |
|---|---|---|---|
| FA-1 | RECIPES' metadata recipe led with `into_parts`; rustdoc of `receive` showed the shorter path | docs | DOCS_ONLY, `8bcd1015`: recipe starts from `receive()`; native recipe uses `transport_error` |
| FA-2 | `csv`/`jsonl` do not enable `json`; the consumer wrote its own JSON decoder | docs (feature selection was the brief's) | DOCS_ONLY, `8bcd1015`: feature list says so |
| FA-3 | moving a `RecordStream` into an upload hid its completion and terminal error; consumer wrote a shared-state `unfold` adapter and stringified the error | library (A3 composition) | IMPLEMENT, `8bcd1015`: `RecordStream::watch() -> StreamWatch` (completion, position, failure kind; the `ReadError` itself is the upload error's source); test `streams.rs::a_watch_reports_the_download_outcome_after_the_upload` |
| FA-4 | `CsvWriter` docs still said to apply `Upload` to a native request | docs | DOCS_ONLY, `8bcd1015` |
| FA-5 | the page projection closure and resume wrapper are written per SDK; `Incomplete` easier to find in the pagination README than in the guides | provider adapter (expected) + docs | SDK examples (Stripe/GitHub facades) and CALLER_GUIDE §6a link; re-check after migration |
| FA-6 | reading a provider's JSON error document from a non-2xx needed the native response | library candidate | DEFER to the SDK migration evidence: bounded error-document reads already repeat in Stripe/Kubernetes/Salesforce; decide with those cases |

### D-A5 — repeated parameter placement (F7): DEFER_WITH_REASON (measured)

Kubernetes repeats its list selector block in 19 places (13 list + 6 watch endpoints across 4
groups × cluster/namespaced scopes); adding one list parameter today is ~13 identical edits.
Scope values do not help: they would also reach the single-object endpoints of the same scope.
Stripe's account override and Salesforce's per-type query declarations show the same pattern in
smaller form. A parameter-group construct needs collision, docs, override and cfg semantics; the
contract allows it only as a targeted experiment. Decision: collect the Phase B cases (GitLab,
Microsoft Graph, Notion, Airtable list endpoints) before designing anything; revisit between
waves with at least two new distinct providers showing the same repetition.

## Phase A migration findings (existing five SDKs)

| ID | Source | Observation | Disposition |
|---|---|---|---|
| MG-1 | GitHub migration (`meta.rs`, `download.rs`) | relative `Link`/`Location` resolve against the final response URL, which `Received` did not expose; the SDK took `into_parts()` to read `response.url()` and re-wrapped with `Decoder::receive` | IMPLEMENT: `Received::url()` (unredacted, documented, not in `Debug`); GitHub `capture` now uses `head()` + `url()` |
| MG-2 | Shopify bulk result download | a labelled native response could reach a reader only via `Incoming::new`, which turns a non-2xx into a `concord::Error` instead of the reader's `OpenError` | IMPLEMENT: `Openable for (reqwest::Response, Operation)` |
| MG-3 | Shopify bulk result download | no public conversion of a native `reqwest::Error` to a labelled Concord error (URL removed) | IMPLEMENT: `Operation::transport_error` |
| MG-4 | GitHub `download::locate` | no way to build a `Status` error from an unexpected status without a branch that cannot succeed | DEFER: one call site; revisit if Phase B shows it again |

Test: `checks/phase-a/tests/received.rs::the_final_url_and_labelled_native_inputs`.
| MG-5 | Salesforce result parts | `Received::operation()`/`Incoming::operation()` are `Option` even for generated endpoints; the SDK keeps a fallback name for provider errors built from decoded JSON | DEFER: no wrong result, one SDK; revisit if Phase B SDKs repeat it (a per-endpoint `Operation` constant would be the candidate) |
| MG-6 | Salesforce query facade | a `&self` helper returning `impl Future + 'static` captures the borrow under edition 2024 and needs `+ use<..>` for `Pages::new` | DOCS_ONLY: pagination README note |
| MG-7 | Kubernetes / multipart | generated body docs ignored runtime media; format links tripped `rustdoc::redundant_explicit_links` under `-D warnings` | IMPLEMENT in the macro (docs wording, targeted allow on generated owner types) |
| MG-8 | Phase A UI case | rustc lists `Incremental` implementors, which depend on enabled features; one stderr cannot hold for json-only and workspace builds | test layout: `tests/ui_stream` runs only with `json`+`csv`+`jsonl` |

## Independent reviews of Phase A (Codex `gpt-6.1-sol`, xhigh, read-only)

| ID | Severity | Finding | Fix |
|---|---|---|---|
| SOL-A-1 | medium | `PagesError::Provider` `Display` formatted the provider error (may quote a URL) | content-free `Display`; error kept in the field and `source()`; the secrecy test now uses an error whose `Display` quotes its secret |
| SOL-A-2 | medium | `-> stream!()` (a type macro named `stream`) was parsed as the keyword | keyword excludes a following `!`; UI regression case |
| SOL-A-3 | low | a `StreamWatch` created after a terminal error lost its failure kind | `RecordStream` keeps the kind; `failure()`; test |
| SOL-B-1 | medium | `examples/multipart` `UploadForm`/`FileUpload` derived `Debug` printed field values, file names and content | hand-written `Debug` (counts, media type, length); test |

Verdicts: ACCEPT_WITH_FIXES (both). Areas reported clean in the second pass: GitHub, Stripe,
Shopify, Kubernetes, Salesforce A5 (validated EOF, locator rules, `maxRecords`), dx03 resume and
publication gating, runtime CSV upload, multipart wire encoding, library additions, tests.

## Wave 1 (frozen library `94d13527`; Nager, PokéAPI, Twilio, S3)

Sources: each SDK's `FRICTION.md` (author), `FIRST_USE.md` (consumer), `MAINTENANCE.md`
(maintainer), committed unmodified. All four first-use consumers reached both outcomes from public
docs; none needed implementation source (PokéAPI's consumer read the testserver API to build its
fake). No role patched Concord or used private items.

| ID | Evidence | Attribution | Disposition |
|---|---|---|---|
| W1-1 | Nager `fetch()`, Twilio `create_message`/`get_message` + `prepare_*`, PokéAPI methods: a wrapper per operation only to return the SDK's error and metadata (as GitHub, Stripe, Salesforce, Shopify do); Twilio author: "maintenance must keep declaration, preparation and wrapper methods aligned" | shared mechanic (generated operations cannot be addressed generically) | IMPLEMENT `db02ca02`: `concord::Endpoint` on every generated endpoint; migration with paired before/after in progress |
| W1-2 | Twilio re-implements a bounded error-document read through `into_response()` (64 KiB loop); Stripe/Kubernetes/Salesforce already do; Nager/PokéAPI consumers needed the native escape for error bodies | shared mechanic | IMPLEMENT `db02ca02`: `Received::bytes_limited`, `ErrorKind::Limit`; Twilio + Stripe migration in progress |
| W1-3 | Nager author: CHEATSHEET/RECIPES summary rows still lead to `into_parts`; Twilio author: RECIPES says the URL is not exposed | docs | DOCS_ONLY `db02ca02` |
| W1-4 | S3: signing must happen after `prepare()`; the generated `send()` would go unsigned, so the declaration is private and 37 forwarding setters re-create the generated builder | shared mechanic, one cohort consumer | DEFER_WITH_REASON: one consumer (AWS-style signing); revisit if a second provider needs a per-request finalizer (candidate: Snowflake key-pair JWT, wave 4). With `Endpoint` the SDK could expose generated types and one signed adapter, at the cost of an unsigned `send()` that fails with 403 |
| W1-5 | S3: object keys span path segments; Concord writes `/` as `%2F`; the SDK rewrites it after `prepare()` | declaration/placement limit | DEFER_WITH_REASON: one wave 1–4 consumer; the held-out registry (wave 5) must not tune it; revisit after wave 5's first pass |
| W1-6 | PokéAPI maintenance added `follow_with_limit`, rewriting `limit` inside a server `next` URL; conflicts with the cohort rule to follow returned links as returned | harness (ticket wording too loose) + SDK | SDK_ONLY: reverted in integration, evidence kept in history; capsule wording to name the contract explicitly |
| W1-7 | S3: SigV4 requires `%20` where Concord form-encodes `+` in queries; the SDK re-encodes before signing | provider signing rule | SDK_ONLY |
| W1-8 | S3: the testserver could not answer HEAD with a `Content-Length` without sending bytes | harness | IMPLEMENT `3b7d081b`: `Body::HeadOnly` |
| W1-9 | Twilio consumer: two `Error` types in public signatures; diagnostics name the hidden `TwilioApi`; `Debug` hides the status field the workflow is about; empty SID sent | SDK | SDK fix in integration |
| W1-10 | S3 consumer: after a body failure later reads return `Ok(None)`, like a clean end | SDK completion semantics | SDK fix in integration (observable completion) |
| W1-11 | PokéAPI consumer: `InvalidUrl` gives no reason | SDK | SDK fix in integration |
| W1-12 | Consumers/maintainers: the capsule said "frozen commit" while HEAD carried the SDK's own role commits | harness | capsule wording fixed `cd1f659b` |
| W1-13 | Maintenance dispersion: Nager (fixture + docs only, model already tolerant), Twilio (one model file + fixtures/tests/docs), S3 (fixtures + fake store + docs; no declaration/adapter change) | positive | — |

### Review of the between-wave changes (Codex `gpt-6-astra`, xhigh, read-only, session `01a1ae9f…`)

| ID | Severity | Finding | Fix |
|---|---|---|---|
| AST-W1-1 | high | the generated `impl Endpoint for Op { type Output = .. }` made a `pub` client with a private output type (or a private stream format) fail with E0446 — a source break for previously valid declarations | `337da135`: `Endpoint<T>` with the output as a trait parameter (probed with rustc 1.99.1: an associated type is E0446, a parameter compiles without warning); compile-pass regression `checks/phase-a/src/lib.rs` `private_outputs` |

Areas reported clean: cfg inheritance, generics, items named `Endpoint`, renamed dependency,
inherent/trait `prepare`, all output forms, `Operation` identity and redaction, `bytes_limited`
boundaries and kinds, `Body::HeadOnly` for HEAD, docs. Verdict ACCEPT_WITH_FIXES.

### Wave 1 integration results (`8d434143`; Opus worker `a0b72bc7…`)

- **W1-1 paired evidence (`Endpoint<T>`):** PokéAPI adopted (`pub type Client = PokeApi`, one
  `Call<T>` adapter; adding an operation = declaration + README instead of declaration + a client
  method + README; `examples/pokeapi-sdk/INTEGRATION.md`). Twilio prototype built, measured and
  **not adopted**: construction keeps a wrapper (validated Basic credentials), its empty-SID rule
  had no place in a generic adapter, every call gained `.call()` and an import
  (`examples/twilio-sdk/INTEGRATION.md`). Nager not adopted (one operation; `fetch()` is already an
  inherent method). Conclusion recorded in AUTHOR_GUIDE §5.12: the trait pays where the generated
  client is the SDK's client; it is not a universal replacement for domain methods.
- **W1-2:** Twilio (+12 −19) and Stripe `send.rs` (+13 −22) read error documents with
  `bytes_limited`; classification, the exact-limit boundary and every test expectation unchanged.
- **W1-6/9/10/11:** PokéAPI `follow_with_limit` reverted (`ac766d6a`); typed `UrlRejection`;
  Twilio `TwilioError` in signatures, `Debug` shows sid/status/error code, empty SID refused (one
  first-use expectation changed from "observes 404" to "refused locally", required by the fix); S3
  `ObjectBody::completion()`; S3 test support uses `Body::HeadOnly`.

Remaining gaps reported, with dispositions:

| ID | Gap | Disposition |
|---|---|---|
| W1-14 | an empty value in a hole with a literal suffix (`"{sid}.json"`) produces `.json`; a whole-segment hole refuses empty | REJECT_WITH_REASON: a non-empty `.json` segment is a valid URL; which identifiers are valid is the provider's rule (Twilio now refuses an empty SID) |
| W1-15 | the generated `{Client}Config` takes raw declared types; validated construction keeps an SDK wrapper | DEFER: SDK construction policy; `ToHeaderValue` is sealed on purpose (credential text must not go through `Display`) |
| W1-16 | passing a generated operation where a future is expected gives E0277 with no hint toward the adapter | DOCS_ONLY (AUTHOR_GUIDE §5.12) |
| W1-17 | no decoder from an `Endpoint` type: following a server URL prepares a placeholder operation to borrow its decoder | DEFER, grouped with server-URL following (GitLab `Link`, Graph `nextLink` in later waves) |

### D-W2-HDR — validated credential types as header values (REJECT_WITH_REASON)

Twilio (W1-15) and GitLab (`PrivateToken`; the generated `new`/`set_token` accept any `HeaderValue`,
so an empty token was built and sent) asked for an SDK type in a header placement. A trial
unsealing of `ToHeaderValue` was reverted before commit: the seal is the mechanism behind H10b /
K12 (`ui/to_header_value_sealed.rs`, `native_headers.rs::dual_use_values_are_withheld_everywhere_or_are_text`)
— a value whose sensitivity is carried only by a native flag must never also be text, and a user
type could implement both `Display` and a flagged `ToHeaderValue`, leaving its path/query
placements unredacted unless the author also declared `#[sensitive]`. The replacement would not
preserve the check, so the invariant stays. Validated construction remains SDK policy: keep the
generated client behind the SDK's constructor, or refuse an invalid credential in the client's
send policy (`send Policy;`) before execution.

## Wave 2 (frozen library `e04236ca`; Frankfurter, GitLab, Slack, OpenAI)

All four first-use consumers reached both outcomes from public docs with no implementation-source
intervention. Two of four authors (Frankfurter, Slack) used `Endpoint<T>` unprompted. No role
patched Concord or used private items.

| ID | Evidence | Attribution | Disposition |
|---|---|---|---|
| W2-1 | Frankfurter consumer: generated `send()` and SDK `fetch()` compile side by side with different error contracts; Frankfurter/Twilio consumers: two types shown as `Error`; GitLab consumer: "choosing `.call()` for SDK errors"; Slack/S3 keep the declaration private for the same reason | shared mechanic (generated terminal method fixed to `concord::Error`) | IMPLEMENT `badb2f59`: client `send P;` + `concord::SendWith<T>`; migration in progress |
| W2-2 | GitLab (`Link`), PokéAPI (`next`), GitHub: follow a server URL by throwaway `prepare` + `url_mut` + `Decoder::execute`; GitLab noted the URL then leaves redaction | shared mechanic (3 consumers) | IMPLEMENT `e8118a44`: `Prepared::follow` (same origin, `Debug` withholds) |
| W2-3 | Slack, Frankfurter: finite success decoding has no byte bound | shared mechanic | IMPLEMENT `1f71432c`: `Received::decode_limited` |
| W2-4 | GitLab control test: reqwest forwards `PRIVATE-TOKEN` on a cross-origin redirect; GitHub and GitLab each build a redirect policy | application transport + docs | DOCS_ONLY `1f71432c` (author guide §3); Concord does not own the client |
| W2-5 | OpenAI: `with_base_url` replaced the `/v1` in `base` | docs | DOCS_ONLY `1f71432c` |
| W2-6 | Frankfurter: RECIPES error-document entry described a chunk loop | docs | DOCS_ONLY `1f71432c` |
| W2-7 | GitLab, Twilio: a validated credential type cannot be a header value; generated `new`/`set_token` take any `HeaderValue` | shared mechanic vs H10b invariant | REJECT_WITH_REASON (D-W2-HDR, `5c507798`) |
| W2-8 | OpenAI: SSE framed in the SDK over the native response | representation; one consumer so far | DEFER to wave 3 (ntfy is the second consumer) |
| W2-9 | OpenAI: multipart Files body built by hand | representation; one real consumer (plus `examples/multipart`) | DEFER to wave 3 (Google Drive multipart/related) |
| W2-10 | GitLab: second strict RFC 8288 `Link` parser (GitHub has one) | representation | DEFER: both strict parsers work and are tested; revisit with the registry (wave 5) |
| W2-11 | Slack consumer: `PageItems` drops page metadata; a `None` after an `Err` is not completion | pagination facade | DEFER: documented contract; revisit if another SDK needs per-item page metadata |
| W2-12 | GitLab: the generated client exposes no base URL; the SDK strips declared segments from a prepared URL | declaration | DEFER: one consumer; `follow`'s same-origin rule removes part of the need |
| W2-13 | Frankfurter: `.parse()` into a generic setter needs `::<Date>` (E0283) | Rust inference, documented on `IntoValue` | DOCS (already documented) |
| W2-14 | Maintenance dispersion: all four perturbations needed no production change (Frankfurter, Slack, OpenAI, GitLab: tests, fixtures, docs only) | positive | — |

### Review of the between-wave changes after wave 2 (Codex `gpt-6.1-sol`, xhigh, read-only, session `01a1aee8…`)

| ID | Severity | Finding | Fix |
|---|---|---|---|
| SOL-W2-1 | low | with a send policy, generated docs of a `-> stream T` endpoint still said `send()` returns the reader | generated docs add a policy line: `send()` returns `<P as SendWith<T>>::Output`, `prepare()` keeps the declared handling |

Areas reported clean: `send` parsing, cfg, generics, collision checks, renamed crate, privacy (a
private policy/output in a pub client fails visibly), the `+ Send` future (a `Send` future may
return a non-`Send` output), non-`Send` endpoints (E0277 naming the field), `follow` origins
(default ports, IDN, IP), redaction, `decode_limited`. The reviewer confirmed reqwest's
cross-origin redirect header removal (Authorization, Cookie, Cookie2, Proxy-Authorization,
WWW-Authenticate); the author guide now lists exactly those. Verdict ACCEPT_WITH_FIXES.

### Between waves 2 and 3: integration (`5ee3dfd6`..`a40aaa00`)

**D-W23-1 — send policy as a value (IMPLEMENT, macros + core, `5ee3dfd6`).** The S3 migration
attempt showed the type form could not express S3's need: SigV4 signs every request with the
client's credentials, region and clock, and a policy built by `<P as Default>::default()` with
a receiver-less `SendWith::send(endpoint)` has nowhere to get them. (An earlier phase-a signing
test signed without credentials and hid this.) Now `SendWith<T>::send(&self, endpoint)`; the
client statement `send value;` names a required client value (cloned per call, `Debug`-redacted
like a sensitive value; an optional value is refused, UI `send_policy_value`), and `send Type;`
keeps `Default`. Every policy in the cohort moved to `&self` + `#[derive(Default)]` (`634b135b`).
Test: `checks/phase-a/tests/send_policy.rs` signs two concurrent clients with different
credentials and checks each request carries its own client's signature.

**D-W23-2 — `Incoming::chunk` (IMPLEMENT, core, `08df6614`).** OpenAI's event-stream reader had
to leave the boundary with `into_response()` because `Incoming` could not read its own body; its
mid-stream failure then printed the URL. One primitive, no framing (SSE stays SDK-local until
ntfy, W2-8). OpenAI migrated (`a40aaa00`, `examples/openai-sdk/INTEGRATION.md`). Test:
`checks/phase-a/tests/streams.rs` `an_own_parser_reads_incoming_chunks_with_identity`.

Migration of the eight cohort SDKs to the between-wave refinements (`send` policy,
`Prepared::follow`, `Received::decode_limited`); workers: Opus subagents `a776f091…` (wave 1
SDKs) and `aadef365…` (wave 2 SDKs), each with a measured before/after in the SDK's
`INTEGRATION.md`:

| SDK | Send policy | `follow` | `decode_limited` | Reason recorded |
|---|---|---|---|---|
| Nager.Holidays | adopted (`send KeepHead;`) | n/a | no | rustdoc then shows a projection type (cost noted) |
| PokéAPI | adopted | adopted | no | replaces the throwaway prepare + `url_mut` + `Decoder::execute` |
| Twilio | not adopted (prototype compiled, measured, reverted) | n/a | no | empty-SID rule stays a domain method; generic misuse gives E0277 without a hint |
| Amazon S3 | adopted on the value form (`send signer;`, `38e6333d`, merged `bbbf6c97`) | n/a | no | 36 forwarding definitions (245 lines) gone; a new declared header reaches callers with no other edit (probe: `x-amz-checksum-mode` signed and sent); metadata puts stay a domain method |
| Frankfurter | adopted | n/a | no | no success-cap friction |
| GitLab | adopted | adopted | no | 8 mutation runs caught by tests; `follow` alone blocks the cross-origin token leak |
| Slack | not adopted (prototype measured, reverted) | n/a | adopted (16 MiB) | declaration stays private; made public, its `prepare()?.send()` would still return an `ok: false` envelope as success and expose an unvalidated `limit` |
| OpenAI | adopted (+ `-> stream` for the event stream) | n/a | no | buffered event stream with no bound before; endless-stream test fixed |

Gaps reported by the workers, with dispositions:

| ID | Gap | Disposition |
|---|---|---|
| W23-1 | rustdoc of a policy `send()` shows `<P as SendWith<T>>::Output` (Nager, PokéAPI) | ACCEPT: it is the real type; the generated doc line names the policy and output (SOL-W2-1) |
| W23-2 | a policy missing `SendWith<Out>` reports E0277 twice (at the `api!` call and at the `send` statement) | MEASURED, KEPT: spanning only the trait path at the endpoint name left the output unchanged and was reverted; both errors name the policy, the output and the fix |
| W23-3 | `Incoming` has no chunk reader (OpenAI) | IMPLEMENT D-W23-2 |
| W23-4 | following a server URL still prepares a placeholder operation (PokéAPI `pokemon("placeholder").prepare()?.follow(url)`) | DEFER (W1-17): revisit with Graph `@odata.nextLink` / registry `Link` in waves 4–5 evidence |
| W23-5 | the gate on `634b135b` failed its three `test` steps: `ui/send_policy_unimplemented` used `concord::Text` (absent without `extras`) and its expected `Default` spelling depended on the build | FIXED `08df6614`: the case checks only the `SendWith` diagnostic with a unit output |

### Review of the between-wave changes before wave 3 (Codex `gpt-6.1-sol`, xhigh, read-only, session `01a1af0c…`)

Scope `1f71432c..08df6614` (crates, checks/phase-a, docs).

| ID | Severity | Finding | Fix |
|---|---|---|---|
| SOL-W23-1 | high | the policy value was marked withheld after sensitivity propagation and composition, so a policy value also placed in a path, query or header template was printed by `Prepared`'s `Debug` | marked before propagation (`model.rs`); regression `checks/phase-a/tests/send_policy.rs` `a_placed_policy_value_is_withheld_at_every_placement`, observed to fail without the fix (URL and `x-tenant` printed) |
| SOL-W23-2 | medium | a defaulted value (`signer: Signer = ..`) was accepted as the policy, contradicting "required" | refused like an optional value; UI `send_policy_value` second case |

Areas reported clean: value-vs-type resolution (qualified paths, generics, lowercase types, scope
values not selected, renamed crate), one clone/default per `send()` outside the future, no borrow
of the caller's client, bounds (`Clone + Send + Sync` value, `Default + Send` type), `Debug` of
client/config/endpoint, cfg, concurrency (independent clients share nothing),
`Incoming::chunk` (URL-free `Transport` with identity; a failure is not sticky: a caller must
keep the error, a later read may return EOF), §5.13. The reviewer could not run cargo (read-only
temp dirs); it used source tracing and rustc type/MIR probes. Verdict ACCEPT_WITH_FIXES.

S3 on the value-form policy (Opus subagent `aa6a8748…`, `examples/amazon-s3-sdk/INTEGRATION.md`
"Between waves 2-3 (after the value-form policy)"): `src/client.rs` + `src/api.rs` + `src/lib.rs`
1227 → 910 lines (830 → 554 code); every first-use and existing test kept its assertions (only
setter renames, `Bytes` bodies, 12 calls moved to the signed `S3Client::prepare`; one protocol
test added). Not yet independently reviewed (scheduled with the wave 3 integration review).
Library gaps it reported:

| ID | Gap | Disposition |
|---|---|---|
| W23-6 | a per-request input meant only for the policy (S3 user metadata, a per-request credentials override) cannot be declared: an unplaced value is "never used" | DEFER: one consumer; reconsider between waves 3-4 if a wave 3 SDK needs the same |
| W23-7 | a decorated request cannot go back through the policy: `Endpoint::prepare` returns `Prepared<T>`, which cannot be edited or rebuilt after `into_parts()` | DEFER with W23-6 (same need: S3 adds metadata headers before signing) |
| W23-8 | generated scope/endpoint handles have no value getters (S3 lost `Bucket::name()` and reads the range and continuation token back from the built request) | CANDIDATE between waves 3-4: with W2-12 (GitLab base URL) this is a second consumer of "read what the declaration holds"; decide on wave 3 evidence |
| W23-9 | adopting a policy leaves `prepare()` the declared (unsigned) request: S3's 12 signed `prepare()` call sites compiled unchanged and failed only at run time | DOCS (between waves 3-4, docs frozen during the wave): AUTHOR_GUIDE §5.13 to say that an SDK whose callers relied on a decorated `prepare()` gives that a distinct name |

