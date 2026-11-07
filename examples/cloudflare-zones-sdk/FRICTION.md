# Friction

What building this slice on Concord cost, with the evidence, what was written by hand, whether
the cost is Cloudflare's semantics or generic mechanics, and the smallest Concord change that
would remove it. Proposed changes are proposals: none was implemented or tested (shared crates are
frozen for this work). Examples consulted are listed at the end.

## F1. A send-policy result cannot be named when the declared output is a wire envelope

- **Evidence.** The endpoints declare the wire envelope (`GET get() -> Single<Zone>`), and the
  policy maps it to the SDK's result (`impl<R> SendWith<Single<R>> for SendPolicy { type Output =
  Result<Response<R>, Error>; }`). `send policy -> Result<_, Error>;` would fill `_` with
  `Single<Zone>`, which is not the output, so the client keeps plain `send policy;`. The generated
  rustdoc of every `send()` then reads
  `pub fn send(self) -> impl Future<Output = <SendPolicy as SendWith<Single<Zone>>>::Output> + Send`
  (`target/doc/.../struct.CloudflareZoneGet.html`); the concrete `Result<Response<Zone>, Error>`
  appears only on the `SendWith` impl. `examples/world-bank-sdk` has the same shape
  (`impl SendWith<WireResponse>`, plain `send SendPolicy;`).
- **By hand.** The crate docs and README state what each `send()` returns.
- **Kind.** Generic: any provider whose every body is an envelope (Cloudflare, World Bank,
  Slack) declares the envelope and returns its content.
- **Smallest change.** Let a `send ... -> template;` match the declared output's outer type and
  bind its parameter, e.g. `send policy -> Result<Response<R>, Error> for Single<R>, Result<PageResponse<R>, Error> for Paged<R>;`.
  The macro already rewrites `_` in the template per endpoint (`with_output` in
  `concord-macros/src/emit.rs`); a per-constructor template is the same substitution, chosen by
  the output's path.

## F2. "`success` decides which part is decodable" needs a hand-written two-phase decode

- **Evidence.** `Json` decodes the declared output in one serde pass. A derived envelope with
  `result: Option<Vec<Zone>>` makes a refusal whose `result` has another shape a `Decode` error,
  losing Cloudflare's `errors`: `tests/protocol.rs::success_false_wins_over_an_unexpected_result_shape`
  is the counterexample (fixture `failure_unexpected_result.json`). `src/wire.rs` therefore
  implements `Deserialize` for `Single<R>`/`Paged<R>` by hand: `success`, `errors`, `messages`
  first, `result` and `result_info` kept as `RawValue` and parsed only when `success` is true
  (serde_json feature `raw_value`).
- **By hand.** About 55 lines in `src/wire.rs` (`Raw`, `parse`, two `Deserialize` impls); the
  unit test `wire::tests::a_derived_envelope_cannot_read_this_refusal` is the control showing that
  a derived envelope fails on that fixture.
- **Kind.** The rule is Cloudflare's; the mechanism (a discriminant field decides how the rest is
  read) is generic serde work.
- **Smallest change.** No API change. The recipe table row "An error inside a 2xx
  (`{"ok": false}`)" in `docs/RECIPES.md` recommends "a typed output that models the envelope"
  without saying that a derived envelope fails on a refusal whose payload has another shape; one
  sentence and a pointer to this two-phase pattern would prevent the trap.

## F3. Sealed `IntoValue`: an SDK identifier newtype cannot take `&str` or `&Self`

- **Evidence.** With `scope zone(zone_id: ZoneId)` the first test build failed with
  `error[E0277]: &str is not accepted for a value of type ZoneId` (and the same for `AccountId`).
  `IntoValue` is sealed (`concord-core/src/value.rs`): only `T`, `&str`/`&String` for `String` and
  `&HeaderValue` for `HeaderValue` are accepted, and `"..".into()` has no target type (E0283).
  Callers write `cf.zone(ZoneId::from("…"))`, and `cf.zone(zone.id.clone())` rather than
  `cf.zone(&zone.id)`.
- **By hand.** Nothing in the SDK; the cost is at every call site with a literal or a borrowed id.
  Declaring the ids as `String` would remove it and lose the type distinction between a zone and a
  record identifier (`cf.zone(record.id)` would compile).
- **Kind.** Generic.
- **Smallest change.** An opt-in marker trait in `concord-core` (say `TextValue`), implemented by
  an SDK for its identifier newtypes, with `impl<T: TextValue> IntoValue<T> for &str` and
  `impl<T: TextValue + Clone> IntoValue<T> for &T`. The trait being local to `concord-core` should
  keep these impls coherent with the existing ones; not verified.

## F4. `concord-retry`'s outcome is an `Option` the SDK cannot reach as `None`

- **Evidence.** `Run::outcome` is `Option<Result<Received<T>, Error>>`; `None` means no attempt
  completed (a deadline, or a refused replay preflight). This SDK sets no deadline, uses
  `ReplayMode::BestEffort` and sends bodiless `GET`s, so `None` is not expected, but the mapping
  must be total: `Error::NoAttempt` exists only for it (`src/policy.rs`, `src/error.rs`).
- **By hand.** One error variant and its documentation.
- **Kind.** Generic.
- **Smallest change.** Document on `Retry::receive` when `outcome` is `Some` (no `timeout` and
  either `BestEffort` or a cloneable request), or add a `receive` variant without a deadline whose
  outcome is not optional.

## F5. `concord-ratelimit` was not adopted: Cloudflare names its limits only in the response

- **Evidence.** `Limiter::acquire(&keys, ..)` needs the scope keys before dispatch. Cloudflare's
  `Ratelimit` header names the limits that applied (`"default"`, a service limit) only on the
  response; the pages document no route-to-limit mapping. Gating every request on every observed
  name would recreate the single global bucket the brief excludes; learning a route-to-names table
  (as Discord learns bucket aliases) is undocumented behaviour.
- **By hand.** Observation only: `ResponseMeta::rate_limits()` returns every item by name, and
  `SendPolicy::retry_rate_limited` honours the server's `Retry-After` on a 429 read.
- **Kind.** Provider semantics. Not a Concord defect: the crate fits providers that publish their
  bucket keys.
- **Smallest change.** None proposed.

## F6. The RFC 8941 list parser for `Ratelimit` / `Ratelimit-Policy` is hand-written

- **Evidence.** `src/ratelimit.rs`: a parser for structured-field lists of items with
  parameters (strings, tokens, integers, decimals and booleans recognised; byte sequences and inner
  lists refused): about 210 lines (`Member`, `members`, `parse_list`, `Parser`) plus 120 lines of
  unit tests. `concord_retry::http::retry_after` already
  offers the same kind of strict, value-free parser for `Retry-After`.
- **Kind.** The syntax is generic (the IETF `RateLimit`/`RateLimit-Policy` fields); which names
  and parameters matter (`r`, `t`, `q`, `w`) is Cloudflare's.
- **Smallest change.** A strict `field::sf_list(values) -> Result<Vec<Item>, FieldError>` beside
  `concord::field::{single, unsigned}` (or in `concord-ratelimit`), returning names and raw
  parameters without echoing values; the SDK keeps the parameter meanings.

## F7. Wire envelope types are public API because the declared output must be nameable

- **Evidence.** `Single<R>` and `Paged<R>` appear in the generated public signatures
  (`prepare() -> Prepared<Single<Zone>>`, `Sends<<SendPolicy as SendWith<Single<Zone>>>::Output>`).
  Made `pub(crate)` as an experiment, the build fails at `send policy;` with
  `error[E0446]: crate-private type Single<Zone> in public interface` (plus a
  `private_interfaces` warning naming the generated `Sends::send`), although
  `docs/AUTHOR_GUIDE.md` §5.12 says private output types stay allowed for `Endpoint<T>`: the
  policy's `Output` projection in `send()` names the output. A `pub` type in a private module
  (unnameable) compiles, but then a native caller of `prepare()?.send()` cannot match the
  envelope. The SDK keeps a documented `pub mod wire`, although ordinary callers only see
  `Response<R>`/`PageResponse<R>`; the envelope is useful to native callers
  (`tests/protocol.rs::the_raw_envelope_keeps_the_failure_for_native_callers`), so the cost is
  small.
- **Kind.** Generic.
- **Smallest change.** State in AUTHOR_GUIDE §5.12/§5.13 that under a send policy the declared
  output must be nameable at the client's visibility; the F1 template change would also keep it
  out of `send()`'s signature.

## What fitted without friction

- `concord-pagination::Pages` with the parameter-group trait `Paging` (`params Paging(query
  page?: NonZeroU32);`, included by both listings) and `concord::Sends`: one walker
  (`src/pagination.rs::walk`) for both listings, exposed as inherent `pages()`/`pages_from()`
  methods on the generated list types. Exhaustion, caps, failure with the collected prefix, and
  resumption all come from the crate.
- `Received::document::<Json, ErrorEnvelope>(ERROR_BODY_LIMIT)` for non-2xx bodies: bounded, the
  status kept whether the document is typed, HTML, empty, malformed or oversized.
- `concord-auth::bearer` for a validated, sensitive `Authorization` value.
- `concord-retry::Retry::receive` for the opt-in 429 policy, with
  `concord_retry::http::retry_after` for strict `Retry-After` parsing.
- `limit SUCCESS_BODY_LIMIT;`, `with_base_url` with `path "client/v4"`, `validate_header_names()`.

## Examples consulted

`examples/zendesk-support-sdk/{Cargo.toml,src/lib.rs,src/policy.rs}`,
`examples/slack-sdk/{Cargo.toml,src/lib.rs}`, `examples/world-bank-sdk/src/lib.rs` (lines
150–260), `examples/fred-sdk/{src/pagination.rs,src/lib.rs}` (lines 70–110). What was taken
from each is recorded in `PROVENANCE.md`.
