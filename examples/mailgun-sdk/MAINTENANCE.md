# SDK27 Mailgun independent maintenance

Date: **2026-11-07**. Frozen maintenance baseline / observed HEAD:
`497bf896cc63855095a1d2310ffdcb6ef697d85e`. Initial working tree was clean.
Live provider calls: **NOT_RUN**. Every executed send/search uses
`TestServer::start` on HTTP loopback. Work remains uncommitted; no shared crate,
check, other example or script was edited. Client → domain scope → endpoints
is preserved.

## Prior exposure and reading order

Before starting, this role had the user capsule and session instructions; it did
not author the SDK or its first consumer. Before implementation it read the
applicable parent `AGENTS.md`, `START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`,
`AUTHORITY.md`, `contracts/PHASES.md` and `contracts/RESOURCES.md`; inspected the
root manifest/lockfile and package manifest, README, PROVENANCE and VALIDATION;
read the five SDK source files and inspected the existing tests/support and
fixtures. Some broad initial tool outputs were truncated, followed by targeted
source/test excerpts. This is source-assisted maintenance, not a docs-only
consumer or an independently delegated review. Historical author/consumer
assistance claims in the older records describe those earlier roles only.

`FRICTION.md` and `FIRST_USE.md` were **not read until after** implementing the
ticket, observing the initial regression failure, correcting it, and passing
the whole package suite (39 integration tests and two doctests). They were then
read for comparison; neither record nor `tests/first_use.rs` was edited. Later
read-only inspection covered relevant buffered multipart codec code, cached
reqwest 0.13.5 multipart header encoding, and testserver API signatures. No
other provider SDK source was consulted in this role.

## Changed behavior and provider basis

`TextEmail::tag` adds up to three repeated `o:tag` fields. Tags preserve spelling
and must be nonempty, control-free ASCII of at most 128 bytes. The provider's
three-tag/ASCII/128-character rules and case-insensitive comparison come from
[Tagging](https://documentation.mailgun.com/docs/mailgun/user-manual/tracking-messages/track-tagging),
read **2026-11-07**; nonempty/control-free checks are SDK restrictions.

`TextEmail::variable(name, value)` accepts typed JSON values, serialized once
per `v:<name>` field. Duplicate names replace their old value. Bare names must
match `[A-Za-z0-9_-]+`; this conservative grammar is a local contract because
the cited provider pages do not specify one. Invalid names and a fourth tag
return value-free `InputError` variants before network I/O or attachment polling.
The existing explicit `Attachment::from_file` read still occurs when called.

The field names and metadata purpose come from the
[send-message reference](https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/messages/post-v3--domain-name--messages)
and [custom-variables section](https://documentation.mailgun.com/docs/mailgun/user-manual/sending-messages/send-attachments#attaching-metadata-to-messages),
both read **2026-11-07**. Variables are also recipient-visible, with documented
provider send-options/truncation limits described in README/PROVENANCE. The
ticket's JSON-valued caller contract is explicit, including quoted JSON strings.

The generated event builder gains `.set_tags(...)`, using the `tags` query
filter from the [events reference](https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/events/get-v3-domain_name-events),
read **2026-11-07**. `Event.tags` and `Event.user_variables` expose returned
metadata, with empty defaults when absent and existing redacted Debug behavior.
The original URL context automatically binds the new filter during resumption;
no paging code needed editing. Continuation URLs remain verbatim. Submission
acceptance, recipient-specific delivery interpretation and no automatic replay
retain their existing behavior.

## Every edited file and location

| File | Location and edit |
|---|---|
| `Cargo.toml` (inside this package) | `[dependencies]` / `[dev-dependencies]`: promote already-locked `serde_json` to runtime use. |
| `src/message.rs` | `TextEmail` fields and `new`: initialize tags/variables; new `tag` and `variable` methods; `MailgunMultipart::encode_body`: one shared ordered text-field mapping and two existing attachment sinks; streamed form now preserves literal field names. |
| `src/error.rs` | `InputError`: add `TagLimit`, `Tag`, `VariableName`; existing value-free Error formatting suffices. |
| `src/lib.rs` | `Mailgun → domain → events` declaration: sensitive optional `tags` query and generated setter/doc. |
| `src/events.rs` | `Event`: defaulted tags and renamed `user-variables` JSON map; collection import. Delivery/paging functions untouched. |
| `tests/metadata.rs` | New regression suite: both wire encoders, JSON types/escaping, repeated tags, replacement, validation bounds, zero source polls/connections, redaction and ambiguous timeout without replay. |
| `tests/metadata_consumer.rs` | New public-API consumer and authored synthetic pages: file/one-shot send, retained ID, tag filter, bounded accepted prefix, changed-filter refusal, explicit resume through delivered and empty pages, returned custom data and verbatim opaque links. |
| `README.md` | Compiled workflow now tags/adds custom data/filters; added naming, JSON, metadata visibility/limits, diagnostics and source/date contract. |
| `PROVENANCE.md` | Runtime dependency row plus dated maintenance sources, fixture provenance, local naming choices and baseline. |
| `MAINTENANCE.md` | This maintenance evidence and candidate improvements. |

The root `Cargo.toml` already included `examples/mailgun-sdk`, and Cargo.lock
already contained its entry. Neither changed. No new crate or package version
was needed: `serde_json` 1.0.154, MIT OR Apache-2.0, was already a direct test
dependency. Its promotion supports actual runtime JSON encoding/decoding.

## Repeated facts and diagnostics

The tag limit/shape appears in validation, method docs, README and independent
test boundaries. The variable grammar appears in validation, `InputError` docs,
method docs, README and invalid-name tests. Wire spellings occur in the shared
field mapping, event serde rename or endpoint declaration, and independently
authored fixtures/assertions plus documentation. JSON's runtime role required
the manifest, send/event types and provenance to agree. These are intentional
contract/test/doc repetitions; they still cost maintenance.

Previously required message field mapping was repeated inside both encoder
branches. This change maps from/to/subject/text/tags/variables once before
selecting a sink, so future text metadata needs one encoding edit. The two
multipart framing paths and field insertion loops still remain.

The initial library build passed; no compiler omission diagnostic occurred.
Rust requires the new struct fields to be initialized and handled in exhaustive
destructuring, but no deliberate omitted-field compiler experiment was run.
The first regression run had **2 passed / 2 failed**: equality and timeout wire
assertions found no literal `o:tag` on the streamed branch. The inherited reqwest
`percent_encode_attr_chars()` setting emitted `name*=utf-8''o%3Atag` (and encoded
`v:` names), unlike the buffered codec. The compiler cannot check that wire
contract. Switching to `percent_encode_noop()` fixed it; all field names are
fixed or validated ASCII names, and filename escaping remains reqwest's job.
No assertion was weakened. The new regression proved why copying a field loop
alone was insufficient. No generated setter or paging edit was missed.

## Verification and resource record

All Cargo commands used `CARGO_BUILD_JOBS=4`, one command at a time, default
worktree `target/`, offline; no CARGO_TARGET_DIR override. Artifacts were reused,
so these are incremental results, not clean-build claims. Toolchain:
`rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1 (5f94df478 2026-08-27)`.

| Command / check | Observed result |
|---|---|
| `cargo build -p concord-sdk-mailgun --offline` | PASS, first build without `--locked`, 0.62 s; no lockfile change. |
| `cargo test -p concord-sdk-mailgun --test metadata --locked --offline` (first attempt) | FAIL: 2/4, streamed colon encoding, corrected as above. |
| `cargo test -p concord-sdk-mailgun --locked --offline` | PASS: first_use 15, protocol 10, workflows 9, new metadata 4, new consumer 1; two doctests; zero failures/ignored. |
| `cargo clippy -p concord-sdk-mailgun --all-targets --locked --offline -- -D warnings` | PASS. |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p concord-sdk-mailgun --no-deps --locked --offline` | PASS. |
| `cargo test -p concord-sdk-mailgun --doc --locked --offline` | PASS, both doctests after README update. |
| `rustfmt --edition 2024` on changed Rust files; `git diff --check` | PASS. |
| Byte comparison against frozen baseline | Existing first_use/protocol/workflows/support, root manifest and lockfile unchanged. Parsed lockfile package entries/versions identical. All changed/new files owned by this package. |

Pre-build commands `df -h . /tmp`, `du -sh target ~/.cargo/registry`, `free -h`,
`nproc` observed `/dev/nvme0n1p2`: 915G total / 199G available; default target
1.5G, registry 1.5G; RAM 30Gi total / 21Gi available; 20 CPUs. `/tmp` is a
separate 16G tmpfs / 15G available. Final samples: target 2.1G, filesystem 198G
available, RAM 21Gi available. No pressure-triggered cleanup was needed; samples
are not peak measurements. Only this worktree's target is an owned disposable
cache. Source, fixtures and evidence were preserved.

## Remaining responsibilities and Concord candidates

Provider/SDK responsibilities remain tag rules, local variable-name policy,
wire prefixes and JSON values, provider HTTP envelopes, Basic `api:key` and
regional origins, domain/path trust, empty-page termination, accepted versus
delivered semantics, and the lack of a general send idempotency guarantee.
Application responsibilities remain durable IDs, search windows, completeness
of evidence, explicit resume and reconciliation after ambiguous sends. Metadata
does not authorize replay; new timeout tests observe one connection/complete
POST and one source evaluation, with a subsequent no-replay observation window.

Remaining generic duplication includes two multipart sinks/boundary-binding
paths, the unused `Encode::encode` refusal, prepared-follow policy dispatch,
resume-context/cycle tracking and response-policy packaging. The earlier
FRICTION findings independently describe these same areas. No shared fix was
made under this capsule's frozen boundary. Candidate changes to record:

- **External codec — `crates/concord-codecs/src/multipart.rs`:** support lazy,
  non-Clone parts with runtime media binding and literal quoted field names.
  This would remove the reqwest sink and its independent name-encoding choice.
  Preserve zero preparation polls; buffering is not a substitute.
- **Core codec contract — `crates/concord-core/src/codec.rs`:** consider a
  default body-only refusal or one canonical body-plus-media operation, removing
  the dummy method from runtime-media SDK adapters. Compare compatibility first.
- **Execution boundary:** retain an SDK policy when following a prepared request,
  avoiding the split between generated first-page `.send()` and private
  `SendPolicy::execute`. Mailgun's domain collection validation remains SDK code.
- **Pagination/macros:** optional context/cycle helpers could remove repeated
  bookkeeping; generated docs should identify expanded `QueryPairs` inputs.
  The new `tags` query needed only an ordinary declaration, so this ticket does
  not justify a provider-specific macro keyword or hierarchy change.

The fixture suite establishes local wire/ownership behavior only. It does not
establish real Mailgun delivery or resolve the previously recorded timestamp,
retention and message-ID-filter documentation questions.
