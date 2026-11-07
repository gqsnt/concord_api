# SDK27 provenance

Author worktree baseline: `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`.
Package: `concord-sdk-mailgun` 0.0.0. Research/read date: **2026-11-07** (UTC).
Live provider calls: **NOT_RUN**. Only public documentation was fetched remotely.
All executions use `concord-example-testserver` on HTTP loopback. No real API key,
email address, message ID, account, or provider response was used.

Assistance label: **example-assisted**, through the required Concord documentation
and its embedded example excerpts. No other provider SDK source file was opened
directly. The fixture harness was inspected directly. There was no independent
consumer/reviewer agent, and no claim of holdout or docs-only implementation.

## Official sources

Every page below was read on 2026-11-07. These are contract sources, not evidence
of live service behavior. The Markdown representations were fetched read-only
with curl after the web reader could not expand nested response properties.

| ID | Exact page | Facts used |
|---|---|---|
| M1 | [Send via HTTP](https://documentation.mailgun.com/docs/mailgun/user-manual/sending-messages/send-http) | Basic `api:key`, text fields, repeated `to`, regional substitution, provider message size limit. |
| M2 | [Send an email](https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/messages/post-v3--domain-name--messages) | POST `/v3/{domain}/messages`, multipart fields/attachment, `id` and `message`, 200/400/401/429/500 statuses. |
| M2m | [Send an email, Markdown schema](https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/messages/post-v3--domain-name--messages.md) | Expanded error schemas: `message` required for 400/429/500; 401 does not promise a typed body. |
| E1 | [Introduction to Events](https://documentation.mailgun.com/docs/mailgun/user-manual/events/events) | Queued acceptance versus recipient-server delivery; follow next links until empty items, despite always-present links. |
| E2 | [Retrieve domain events](https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/events/get-v3-domain_name-events) | Search range, ascending yes/no, max 300, message-id/recipient/event filters, paging next/previous, temporary/permanent failures. |
| E2m | [Retrieve domain events, Markdown schema](https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/events/get-v3-domain_name-events.md) | Expanded `message.headers.message-id`, failure severity and `flags.is-test-mode`; test-mode delivery does not establish actual sending. |
| A1 | [Authentication](https://documentation.mailgun.com/docs/mailgun/api-reference/mg-auth) | HTTP Basic username `api`; domain sending keys authorize sending only, so both workflows require broader access. |
| A2 | [API Overview](https://documentation.mailgun.com/docs/mailgun/api-reference/api-overview) | US/EU origins; general 400/401/403/404/429/500 classes; rate-limit limit/remaining/reset headers. |
| E3 | [Event Structure](https://documentation.mailgun.com/docs/mailgun/user-manual/events/event-structure) | Common event/type/timestamp/id fields, day-scoped event IDs, additive schema evolution. |
| E4 | [Event Samples](https://documentation.mailgun.com/docs/mailgun/user-manual/events/event-samples) | This page points to E2 for examples; it provided no sample body itself. |
| M3 | [Send with Attachments](https://documentation.mailgun.com/docs/mailgun/user-manual/sending-messages/send-attachments) | File upload field named `attachment`, distinct from inline images. |

Documentation discrepancies remain explicit: E1 describes 30-day retention,
while E2 says at least 3 days. E3 describes Unix seconds, while E2m's timestamp
description says nanoseconds. The SDK promises neither retention period, stores
the numeric event timestamp without conversion, orders it within a response
dataset, and uses the documented epoch-seconds request range. Synthetic fixtures
use seconds as in E3. No live verification resolved these discrepancies.

The SDK deliberately chooses no automatic replay, even where general HTTP error
guidance suggests retrying. No cited contract grants idempotency for this email
submission slice; an inbox-placement-test key is not a general send idempotency
key. Rate reset is exposed as an integer rather than used to guess a sleep.

## Fixture derivation

All fixture values are **authored synthetic**. Expected field names and behavior
were taken from the sources above, not generated from the SDK encoder/decoder.
No recorded production traffic is present. `__ORIGIN__` is replaced with the
local request Host; tests never replace it with a live service address.

| File / generated fixture family | Derivation and independent expectation |
|---|---|
| `fixtures/accepted.json` | M2 response shape and acknowledgement text; authored synthetic ID. Tests retain the exact ID, including angle brackets. |
| `fixtures/events-first.json` | E1/E2/E2m/E3: two recipient records, accepted and temporary failure; synthetic timestamps and opaque next URL. |
| `fixtures/events-second.json` | E1/E2m: delivered and permanent failure for separate recipients, plus an unknown additive field. |
| `fixtures/events-empty.json` | E1 explicitly requires the empty terminal page even though a next URL still exists. |
| `fixtures/problem.json` | M2m/A2 `message` error envelope; synthetic secret marker to test safe formatting. Used with each documented status and synthetic fallback statuses. |
| `fixtures/malformed.json` | Authored invalid JSON, including a secret marker; expects Decode without printing input. |
| `fixtures/truncated.json` | Authored incomplete JSON on an otherwise complete HTTP body; expects Decode, distinct from transport truncation. |
| `fixtures/report.txt` | Authored harmless file bytes; M1/M3 provide request field naming, tests compare exact bytes independently. |
| Protocol-test mutations | Synthetic empty/wrong-shape/oversized JSON; absent/repeated/wrong charset/media headers; HTTP chunk failure, redirects, bad numeric headers and hostile page links. These are robustness cases, not claims of documented Mailgun responses. |
| Workflow-test mutations | Synthetic delayed/stalled success, one-shot source errors and size overflow, 429/500 with Retry-After, failed second page, link cycles, changed resume contexts, unknown event/severity and conflicting timestamps. |
| Test-mode mutation | E2m `flags.is-test-mode=true`, applied to a synthetic delivered event, must produce `TestMode`. |

The multipart assertion reader in `tests/support/mod.rs` is independently
authored for these fixtures. It reads the boundary from the received header and
checks required names, repeats and bytes. It does not copy an expected boundary
or request body from `prepare()` or the SDK's implementation. Synthetic Basic
expectation is standard Base64 of `api:synthetic-key`.

## Local reading and example assistance

Required guidance read: the parent handoff's `AGENTS.md`, `CLAUDE.md`,
`START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`, `AUTHORITY.md`,
`contracts/PHASES.md`, `contracts/RESOURCES.md`; workspace `README.md`,
`docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`, and
`docs/RECIPES.md`. No worktree-local AGENTS/CLAUDE was found. The specific user
capsule restricts this role to authoring SDK27, superseding the parent's broader
campaign tasks and commit authority.

Concord crate READMEs read before adoption decisions: `concord-core`,
`concord-macros`, `concord-auth`, `concord-codecs`, `concord-pagination`,
`concord-retry`, `concord-ratelimit`. The facade uses the root README.
Rustdoc read in source: `crates/concord/src/lib.rs`,
`concord-core/src/{lib,prepared,response,codec,error}.rs`,
`concord-codecs/src/{multipart,json}.rs`,
`concord-auth/src/{lib,credentials}.rs`,
`concord-pagination/src/{lib,pages,types}.rs`, and `concord-retry/src/http.rs`.
Public signatures/source inspected only where needed; no shared source changed.
The cached reqwest 0.13.5 `src/async_impl/multipart.rs` rustdoc/source established
that `Form::boundary` and `Form::into_stream` are public and lazy.

Every consulted example file is distinguished below from a hyperlink merely
mentioned in documentation. No claim is made to have read the complete source
of a file whose excerpt or prose recipe alone was used.

| File | How consulted; what was taken |
|---|---|
| `examples/testserver/Cargo.toml` | Direct read: package name/path and available Tokio fixture dependency. |
| `examples/testserver/src/lib.rs` | Direct read of rustdoc, Recorded/Reply/Body definitions and API signatures: closure signature, request capture, delay/stall/failure modes and explicit shutdown. |
| `examples/quickstart/src/lib.rs` | Embedded in README/QUICKSTART: client→scope→endpoint, generated `send`, base overrides and bounded documents. |
| `examples/quickstart/src/main.rs` | QUICKSTART excerpt: local-server execution; its custom server was not copied. |
| `examples/shapes/tests/wire.rs` | QUICKSTART excerpt: regional subdomain testing; not adopted because Mailgun chooses full origins. |
| `examples/openai-sdk/src/lib.rs` | AUTHOR_GUIDE excerpt: ordinary declared multipart body with a text field and file; adopted through the shared codec. |
| `examples/google-drive-sdk/src/upload.rs` | AUTHOR_GUIDE excerpt: multipart/related; evaluated and not adopted for Mailgun's form-data contract. |
| `examples/frankfurter-sdk/src/lib.rs` | AUTHOR_GUIDE excerpt: `SendWith`, bounded provider document and success metadata; adapted in `SendPolicy`. |
| `examples/native/src/native_edit.rs` | RECIPES excerpts: native query/timeout edits; no request mutation was needed here. |
| `examples/compat/src/lib.rs` | RECIPES excerpt: multi-segment resource keys; not adopted for a validated DNS domain. |
| `examples/native/src/{errors,metadata,policies,upload}.rs` | RECIPES prose descriptions only: bounded error handling, value-free Debug, explicit replay permission and lazy source ownership. No direct source read. |
| `examples/multipart/src/lib.rs` | RECIPES prose only: streamed reqwest multipart as an escape. Used the narrower `Form::into_stream` encoder route instead. |
| `examples/app/src/traversal.rs` | RECIPES prose only: lossless bounded traversal; selected the shared pagination crate instead of a handwritten loop. |

The cloud-environment runtime skill and networking reference were also read.
Its status tool was unavailable and `/etc/codex/network-policy.json` absent;
no credentials or environment secret files were inspected. Public documentation
retrieval succeeded through the available web tool and ordinary curl. All Cargo
commands used the local registry offline.

## Dependencies

No new third-party crate was added to Cargo.lock. Only the new package entry was
added; every existing package entry, version and dependency list is unchanged.
Direct dependencies resolved from the existing lock:

| Crate | Version | License | Reason |
|---|---|---|---|
| concord, concord-auth, concord-pagination, concord-retry | 4.2.0 | MIT OR Apache-2.0 | Declaration/runtime, Basic, bounded pages, Retry-After parsing. |
| reqwest | 0.13.5 | MIT OR Apache-2.0 | Enable existing transport's multipart/stream features. |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | Lazy, byte-bounded attachment stream. |
| mime | 0.3.17 | MIT OR Apache-2.0 | Validate attachment media values consistently across both encoders. |
| serde | 1.0.229 | MIT OR Apache-2.0 | Provider response models. |
| serde_json (tests) | 1.0.154 | MIT OR Apache-2.0 | Synthetic fixture mutations, independent of SDK encoders. |
| tokio (tests) | 1.54.1 | MIT | Fixture async runtime. |
| concord-example-testserver (tests) | 0.0.0 | MIT OR Apache-2.0 | Required local fixture harness. |

Workspace feature unification was not used as a substitute for declaring this
package's features: `concord/multipart` and `reqwest/{multipart,stream}` are explicit.
