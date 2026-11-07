# Independent maintenance — SDK42 Datadog Events

Date: **2026-11-07**. Live provider calls: **NOT_RUN**. All SDK requests in this work ran
against `concord-example-testserver` on loopback, including requests whose hostname was
resolved locally to simulate another Datadog site. Production URLs were prepared and
inspected without sending. No commit was attempted.

## Prior exposure and order of work

The maintainer did not write this SDK and had no earlier exposure to its implementation or
campaign records. Before starting, the available information was the user's capsule and the
session's tool/instruction descriptions. The worktree began clean. No parent campaign files,
other worktrees or files outside the permitted worktree/toolchain/registry sources were read.

Before reading `FRICTION.md` or `FIRST_USE.md`, the maintainer inspected the workspace/package
manifests, git status, SDK `README.md`, `PROVENANCE.md`, all seven existing `src/*.rs` files,
and the existing tests (`first_use.rs`, `protocol.rs`, `workflows.rs`). The existing consumer
was read as a compatibility constraint; none of its code or assertions was edited. A worktree
search found no `AGENTS.md`. Additional reads/searches covered the root `README.md`,
`examples/testserver/src/lib.rs`, Concord's facade/manifest, core manifest and
`codec.rs`, `error.rs`, `prepared.rs`, `response.rs`, and macro sources (searches across
`crates/concord-macros/src/*.rs`, with focused output from `syntax.rs`, `model.rs`, `emit.rs`).
The frozen baseline's core error source was also inspected with `git show`. These were
read-only; no shared source was edited. Installed reqwest and http-body registry sources
were consulted for body replay behavior. No other SDK example implementation was read.

The maintainer researched the official pages below, built the existing package offline,
implemented posting and both new test files, then attempted the full test build. Only after
that first implementation attempt failed with the client-only send-policy diagnostic were
`FRICTION.md` and `FIRST_USE.md` read. Those records informed this report; the initial change
was already concrete. No sub-agents or external messages were used.

## Provider evidence and resulting contract

Exact official pages read on **2026-11-07**:

- [Authentication](https://docs.datadoghq.com/api/latest/authentication/)
- [Get a list of events](https://docs.datadoghq.com/api/latest/events/get-a-list-of-events/)
- [Search events](https://docs.datadoghq.com/api/latest/events/search-events/)
- [Events reference](https://docs.datadoghq.com/api/latest/events/)
- [Post an event](https://docs.datadoghq.com/api/latest/events/post-an-event/), linked by the
  Events reference, including its same-page `.md` rendering.

The web reader rejected the supplied URLs as unsupported content. `curl` fetched their
documentation successfully; no live API requests were made. The reference now splits posting
onto its own page. The requested title/text/tags/alert-type/aggregation-key/source shape and
same `api.<site>` host identify **v1** posting. That page marks v1 deprecated and restricted
to organizations that previously used it. The v2 post uses a different intake host and
category-specific attributes, so substituting v2 would violate this ticket's contract.

`NewEvent` validates the documented 4,000-character text and 100-character aggregation-key
limits before a request exists; its fields are private and it has no unchecked deserializer.
Unicode scalar counting is an explicit SDK interpretation of “characters,” not a documented
provider algorithm. The seven documented alert values are an enum. The v1 page specifies no
title, tag or source length limit and includes a successful 100+ character title; none was
invented. Source vocabulary remains provider-defined. The request exposes only the six
ticket fields, not dates or additional event schemas.

`DatadogEvents::post(NewEvent)` and `events().post(NewEvent)` use the existing site and both
sensitive key headers. Requiring the application key on a post is this ticket's SDK contract;
the authentication page itself requires an API key for writes and both keys for reads.
The existing credential checks and read diagnostics stay unchanged.

Success requires HTTP **202** and a decodable `EventAcceptance` envelope. Provider status and
the raw event object are returned, including string IDs without precision loss. Acceptance
does not promise search visibility, and no follow-up search or URL fetch occurs. A v1 posting
ID is not promised to be a v2 search ID. Non-2xx status classification, response limits,
host/site reporting and secret-safe error formatting are shared with reads.

`Error::PostUncertain` means the event may exist: no response, an unreadable/unusable 202
body or an unexpected 2xx cannot become a successful, fully decoded acceptance. Any head
and safe Concord source are preserved. Preparation errors remain definitely local
`Error::Concord` errors. An aggregation key groups events; it supplies no documented
idempotency guarantee. No post is automatically resent, even if the supplied reqwest client
has a retry policy: `PostJson` wraps the encoded JSON as a non-replayable body. Native
prepared-request sends retain that body restriction but bypass the SDK's acceptance/error
policy, as documented. Explicit caller resubmission remains possible and requires the
caller's decision. A returned 5xx is also not proof that no write happened.

## Every edited location

All paths below are relative to `examples/datadog-events-sdk/`.

| File | Location and change |
|---|---|
| `Cargo.toml` | Package description now includes v2 reads and v1 posting; dependencies and versions unchanged. |
| `src/post.rs` (new) | `AlertType`, validated `NewEvent` builders, raw `EventAcceptance`, and private `PostJson` encoding adapter. |
| `src/api.rs` | `api!` declaration: common `api` prefix; explicit endpoint versions; generated `events.post`. `SendPolicy::send` uses the generated post operation identity to apply 202/uncertainty rules. `rejection` retains one status classifier for all operations. |
| `src/client.rs` | `DatadogEvents::post` convenience method and generated request import; scope/base-path rustdoc includes posting. Read methods and pagination logic untouched. |
| `src/error.rs` | `Error::PostUncertain`, construction/helper predicate, metadata/operation accessors, safe Display and source handling; application-key documentation updated. Existing variants and existing diagnostic text retained. |
| `src/query.rs` | Two `InputError` variants and Display messages for posting limits; existing query/search behavior untouched. |
| `src/credentials.rs` | Documentation distinguishes provider read requirements from the SDK's two-key posting contract. Validation unchanged. |
| `src/lib.rs` | New module and exports: `AlertType`, `NewEvent`, `EventAcceptance`, generated `DatadogApiEventsPost`. |
| `tests/post.rs` (new) | Eight focused regressions covering documented requests, all sites, limits, alert values, redaction/authentication, local build failures, timeouts, response failures, 202-only success and no replay under a retrying transport. |
| `tests/post_consumer.rs` (new) | Consumer posts all six fields, receives provider acceptance and a precise string ID, and verifies no follow-up request. |
| `fixtures/post-request.doc.json` (new) | First v1 request example, extracted verbatim from the official posting page. |
| `fixtures/post-long-title.doc.json` (new) | Second v1 request example, including the 100+ character title. |
| `fixtures/post-202.doc.json` (new) | Official v1 202 response example, including its numeric placeholders. |
| `README.md` | Operation table, compiled posting example, validation/acceptance/uncertainty semantics, v1 availability limitation, updated exclusions and test coverage. |
| `PROVENANCE.md` | Dated exact sources, provider facts versus SDK decisions, fixture derivations and replay implementation evidence; original source history retained. |
| `MAINTENANCE.md` (new) | This report. |

Root `Cargo.toml` already included the package, so no workspace edit was necessary.
`Cargo.lock` is unchanged. No new crate/feature was added and no package version changed.
All existing tests and fixtures, `FIRST_USE.md` and `FRICTION.md` remain byte-for-byte
unchanged. No file in `crates/*`, `checks/*`, other examples or `scripts/*` was edited.

## Facts changed in several places

- The new operation needed a declaration, wrapper/import, public export, caller example,
  wire fixture, regression and consumer coverage. The macro generated the endpoint type,
  setters, prepare/send and rustdoc from the declaration; those were not handwritten copies.
- Versioned paths moved from a shared `api/v2/events` prefix to `api` plus per-endpoint paths
  so the fixed client → scope → endpoint tree can contain both v1 posting and v2 reads.
  The v2 `events` segment now appears in both read endpoint paths. Their final URLs and
  generated public type names remain the same, including runtime base overrides.
- Limits and allowed alerts appear in the typed payload, input-error text, README/provenance
  and independently expected tests. Numeric limits have one validation site each, but their
  descriptions and boundary expectations are necessarily repeated. No title limit was added.
- Acceptance versus visibility, uncertain delivery, non-idempotent grouping and no automatic
  replay needed aligned implementation, public docs, tests and provenance. Two-key posting
  and the v1 availability restriction also needed updates in multiple public descriptions.

## Diagnostics and verification

The first implementation attempted an endpoint-local send-policy override. The compiler
rejected `src/api.rs` with: **“`send` is only allowed on the client: one send policy serves
every endpoint.”** The final implementation uses the existing single policy and compares
`E::operation()` with the generated post type's `Endpoint::operation()`. It does not repeat
the endpoint name as a hand-maintained dispatch string. A renamed/removed generated type
will fail compilation.

No omitted export, match arm or other omitted edit was observed. No existing assertion was
weakened to get a pass. After the compiler correction, all runtime tests passed on the first
run. Tests explicitly protect against future omitted edits: missing sensitive markers,
wrong version/site, ignored bounds, accidental reuse of read error semantics for timeout,
or a replayable posting body would fail the relevant assertions. Documentation was also
built with warnings treated as errors to verify public links and generated API integration.

Every cargo command used `CARGO_BUILD_JOBS=4`, one at a time, offline, with the worktree's
default `target/`; `CARGO_TARGET_DIR` was never set. The initial
`cargo test --offline -p concord-sdk-datadog-events --no-run` omitted `--locked` and passed.
All following cargo commands used `--locked`.

Final checks:

- `cargo test --offline --locked -p concord-sdk-datadog-events`: **38 integration tests
  passed** (2 unchanged first-use, 19 unchanged protocol, 8 unchanged workflows, 8 posting
  regressions, 1 posting consumer) and **7 doctests passed**; no failures or warnings.
- `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-datadog-events --no-deps`:
  passed.
- `rustfmt --edition 2024` on edited Rust files and `git diff --check`: passed.
- Scoped diff/status checks confirm ownership and the unchanged consumer, workspace manifest
  and lockfile. Live provider calls: **NOT_RUN**. Fixtures do not establish actual ingestion,
  regional key enforcement, source vocabulary, search consistency, TLS or HTTP/2 behavior.

## Remaining duplication and Concord candidates

Provider-specific work remains the site/host table; key requirements; v1 versus v2 contract
and availability; the payload fields, character-count interpretation and limits; status 202;
authentication status classification; error-envelope shape; grouping versus idempotency;
and the distinction between acceptance and search visibility. These belong in this SDK.

Generic mechanics still include safe provider-error/document wrappers, capturing a response
head alongside a safe source, input-error plumbing, cursor repetition/emptiness guards,
the no-redirect transport helper, and synchronizing wrapper imports/exports/docs with a new
operation. The post reuses the existing status classifier and Json encoder, avoiding copies
of those implementations. The small non-replayable-body adapter is generic transport work.

Candidate improvements, recorded only; no shared implementation changed:

1. Allow an optional endpoint-specific send-policy override within the existing hierarchy,
   inheriting the client policy by default. This would avoid policy-side operation dispatch
   for reads and writes with different delivery semantics. The compiler diagnostic above
   provides direct evidence for this maintenance cost.
2. Offer a reusable non-replayable encoded-body/JSON adapter, documented for client-level
   retry policies. The current solution works with public `Encode` and reqwest APIs, so no
   native request escape or dependency addition was required.
3. Offer a safe owned response/document failure envelope carrying metadata and source once,
   with provider text withheld by default. This would reduce the existing `ProviderError`,
   `ErrorBodyError` and head-capture boilerplate. Keep provider status/uncertainty decisions
   in SDK code; Concord's existing `Delivery` facts are sufficient for this ticket.
4. Retain the earlier records' candidates for opt-in repeated-key pagination guards,
   construction-time placement checks and clearer `prepare()` versus SDK `send()` error
   names. They were not needed to complete posting and were not implemented here.
