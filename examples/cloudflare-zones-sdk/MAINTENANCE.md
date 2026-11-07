# SDK40 Cloudflare Zones maintenance

Date: **2026-11-07**. Live provider calls: **NOT_RUN**. Every executed workflow used
`concord-example-testserver` on loopback. Changes remain in the working tree; no commit was
attempted. Shared crates, checks, scripts and other examples were not edited.

## Change and caller contract

Added the declared endpoint
`cf.zone(zone_id).dns_records().create(NewDnsRecord)` →
`POST /client/v4/zones/{zone_id}/dns_records`. It returns
`Result<Response<DnsRecord>, Error>` and uses the existing two-phase envelope decoder.
A refusal inside HTTP success, including a duplicate record with a wrong-shaped `result`,
is `Error::Provider` retaining provider codes/messages. It never returns an empty record.

`NewDnsRecord` has private fields and validates the supported content-based types A, AAAA,
CNAME, NS, PTR and TXT. Other types are refused rather than accepting unvalidated structured
data or silently omitting required priority. `RecordTtl` separates the usual minimum from
the explicit Enterprise minimum. The create policy also checks the decoded zone ID length.
Existing read types, declarations, page walkers and caller assertions remain unchanged.

Create uses the existing non-read single-attempt branch. Neither the default policy nor the
opt-in 429 policy replays it. Transport failures before headers or during a successful body,
and successful bodies failing decode/media/size checks, are `Error::CreateUncertain` with the
Concord diagnostic retained. Callers list all record pages to reconcile. An absent/null result
still yields `Error::Envelope`, requiring reconciliation as documented. Native `prepare()`
and custom transport retry behavior remain explicit caller responsibilities.

## Evidence and documentation

Exact official pages read on **2026-11-07**:

| Page | Maintenance use |
|---|---|
| [Create DNS Record](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/create/) | POST/body/result, validation bounds and type-dependent content, permissions |
| [List Zones](https://developers.cloudflare.com/api/resources/zones/methods/list/) | Existing account/list envelope contract retained |
| [List DNS Records](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/list/) | Existing record listing and reconciliation paging retained |
| [Rate limits](https://developers.cloudflare.com/fundamentals/api/reference/limits/) | Global/service observations and server delay contract retained |

`PROVENANCE.md` distinguishes documented constraints from conservative SDK choices and lists
the synthetic fixtures and dependency license. The reference does not promise POST
idempotency or a stable duplicate code, and these tests establish local behavior only.

## Prior exposure and files read

Before starting: the maintenance ticket and environment/tool instructions only. I did not
write this SDK. Neither `FRICTION.md` nor `FIRST_USE.md` was read before attempting the ticket.

Before opening those two reports, inspected the worktree status, root/package manifests,
the SDK source modules (`lib`, `models`, `wire`, `error`, `policy`, `pagination`, `ratelimit`),
README, provenance, existing test files and support helpers. Read the four official pages
above. Inspected relevant sections of `examples/testserver/src/lib.rs`,
`crates/concord-core/src/{error,prepared}.rs`, `crates/concord/src/lib.rs`,
`crates/concord-macros/src/{emit,model,syntax}.rs` and
`docs/{AUTHOR_GUIDE,CHEATSHEET}.md` for the endpoint, body, transport and policy contracts.
Searches covered the same relevant source directories; shared files were read only.

Ran the baseline suite, implemented the endpoint/input/error/policy changes, ran the original
suite successfully, and added/executed the new fixture tests before opening `FRICTION.md`
and `FIRST_USE.md`. The first create-specific run had one new assertion failure, described
below. Read both reports only after that attempt. Subsequent inspection included the local
cargo-registry manifest of `percent-encoding` 2.3.2 for version/license verification.

No campaign material, parent-directory files or other worktrees were opened. Historical
example consultations recorded by the original author in `PROVENANCE.md` are not new
consultations by this maintainer. No agent delegation or live provider calls were used.

## Every edited location

Paths in this table are relative to `examples/cloudflare-zones-sdk/`, except `Cargo.lock`.

| File/location | Edit |
|---|---|
| `src/lib.rs`: module/export list, crate navigation docs, `zone → dns_records` scope | Export validated input types; declare `POST create(body record: NewDnsRecord) -> Single<DnsRecord>`; describe expanded slice |
| `src/create.rs` (new): input types, target check, name/TXT helpers | Immutable validated body, TTL constructors, value-free typed input errors; create-only decoded zone-ID validation |
| `src/policy.rs`: `Response`/policy docs, `SendWith<Single<R>>::send`, `create_failure` | Preserve read handling; validate create target before dispatch; classify create transport/decode failures as uncertain; retain existing read-only retry gate |
| `src/error.rs`: `Error`, conversion/accessor/format/source implementations | Add `Input` and `CreateUncertain`; preserve diagnostic kind/status/source and safe formatting |
| `Cargo.toml`: package description, dependencies | Remove read-only claim; use already-locked `percent-encoding` |
| root `Cargo.lock`: `concord-sdk-cloudflare-zones.dependencies` | Cargo adds only the `percent-encoding` dependency edge |
| `fixtures/dns_record_created.json` (new) | Minimal synthetic created record in success envelope |
| `fixtures/dns_record_exists.json` (new) | Synthetic duplicate refusal, intentionally non-record result |
| `tests/create_record.rs` (new) | Seven focused regression tests: wire shape, refusal, 429 non-replay, timeouts, failed bodies, preflight and validation boundaries |
| `tests/create_consumer.rs` (new) | Three public-contract caller tests: select/create/fetch, duplicate handling, committed-but-timed-out write reconciled across record pages |
| `README.md`: overview/call table, create section/example, failures, exclusions, tests | Explain new types, supported subset, uncertain outcomes and transport requirements; add fixture base-URL example |
| `PROVENANCE.md`: maintenance section prepended | Exact read dates/pages, provider facts versus SDK choices, fixture origins and dependency justification |
| `MAINTENANCE.md` (new) | This observable maintenance record |

Root `Cargo.toml` already contained workspace membership, so it needed no edit.
`tests/first_use.rs`, every existing test/assertion, `FIRST_USE.md` and `FRICTION.md` are
unchanged. No package version changed in the lock.

## Repeated facts and diagnostics

The new operation's path/name appears in the declaration, navigation documentation and wire
tests. Its output stays declared as `Single<DnsRecord>` while the actual public send result
is explained in the README and policy docs. Supported record types, TTL/name/proxy constraints
are implemented in `create.rs`, asserted independently in regression inputs, and restated in
README/provenance. Create uncertainty appears in the error enum and accessors, the policy's
two transport/decode boundaries, docs and caller tests. Adding an error variant required
updating status, kind, formatting, conversion and source handling; exhaustive Rust matches
would catch an omitted match arm. Body serialization itself is derived once.

There was no compiler failure and no observed omitted implementation edit. The first new test
run failed at `invalid_create_target_and_body_are_refused_before_network_io`: it expected
`Error::Input(ZoneId)` for an empty ID, but Concord's existing `prepare()` already returned
`Error::Concord(Build)` before SDK target validation. Split the new expectations: empty/dot
segments are Build; an overlong ID is Input. All still assert zero network requests. No old
assertion was changed. Subsequent tests and Clippy passed. Documentation facts still require
manual review; the compiler cannot catch a stale supported-type list or excluded-feature claim.

## Remaining duplication and provider responsibilities

Generic mechanics still in the SDK: `Single`/`Paged` policy extraction has parallel envelope
branches; two-phase JSON decoding; response/error metadata wrappers; read-only retry
classification; structured rate-header parsing; input-error conversion and create failure
mapping. The new zone check recovers the identifier from the prepared URL because the generic
policy has no typed scope-value view; `percent-encoding` avoids duplicating percent decoding.
The check depends on this endpoint's final `/{zone_id}/dns_records` layout. POST currently
identifies create because it is the only POST in this slice; additional writes need explicit
operation classification rather than inheriting that assumption.

Cloudflare-specific responsibilities remain: success-envelope meaning, DNS content and
type rules, plan-dependent TTL, proxy capability, authorization, zone membership,
cross-record conflicts and concurrent state. The SDK cannot prove the plan or conflict state
from this input. Callers assert Enterprise entitlement, use the provided no-retry/no-redirect
transport or configure an equivalent one, and decide what matching records mean during
reconciliation. Missing/ambiguous matches or an incomplete listing do not justify an automatic
resend. No global token bucket was introduced.

## Candidate Concord improvements (recorded only)

- Add a typed preflight hook or documented endpoint-validation adapter that sees scope values
  before serialization. This could remove prepared-URL extraction while keeping permissive
  read IDs and validated create IDs. Candidate locations: macro declarations/emission and
  `Endpoint` policy integration; no shared edits made.
- Provide generated documentation that names a policy's concrete result for envelope output
  types. Create inherited FRICTION F1's projected `SendWith<Single<DnsRecord>>::Output` burden.
- Document a complete write-policy recipe: pre-send failures, transport loss before and after
  headers, provider refusals, malformed successful envelopes, no POST replay, and reconciliation.
  Existing `Error::delivery`, `transport()` and response heads are useful primitives; this
  ticket did not require a shared transport change.
- Reuse a discriminated-envelope recipe/helper and strict structured-field parsing so each
  SDK does not repeat the same generic JSON/header mechanics. The provider keeps the meaning
  of its discriminant and named limits.
- Make read-only replay authorization and endpoint identity easier to express explicitly in
  a policy. A cloneable JSON POST body must not itself authorize retry. Tests here protect the
  current method gate; generic retry changes would need equivalent write regression coverage.

## Validation

All Cargo invocations used `CARGO_BUILD_JOBS=4`, one at a time, default worktree `target/`.
Baseline `cargo test --offline -p concord-sdk-cloudflare-zones` passed 51 tests and one doctest.
The workspace member/lock entry already existed. After adding the direct dependency,
`cargo test --offline -p concord-sdk-cloudflare-zones` let Cargo update only its SDK dependency
list; all later Cargo commands used `--locked`.

Final executed checks:

- `cargo test --offline --locked -p concord-sdk-cloudflare-zones`: **61 tests passed**
  (9 unit, 3 create consumer, 7 create regression, 2 original first-use, 29 original protocol,
  11 original workflow), plus **2 doctests passed**.
- `cargo clippy --offline --locked -p concord-sdk-cloudflare-zones --all-targets --no-deps -- -D warnings`:
  passed without warnings.
- `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-cloudflare-zones --no-deps`:
  passed, including the public create API and links.
- `rustfmt --check --edition 2024` on all changed/new Rust files: passed.
- `git diff --check`: passed.
- Compared parsed before/after lock package identities and contents: no crate additions or
  version changes; only the SDK dependency list changed. Compared all original tests/support,
  read model/wire/pagination/rate modules and both earlier reports byte-for-byte with HEAD:
  unchanged. Checked every status path against the ownership boundary: passed.

The suite includes all original read workflows plus local create fixtures; live-provider
compatibility remains NOT_RUN.
