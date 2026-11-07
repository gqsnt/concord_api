# Concord friction and decisions — SDK37

Frozen core: `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`. All changes stay in
`examples/strava-sdk`, plus workspace membership and the lockfile package entry.
The proposed shared changes below are recorded only; none was applied.

| Item | Observable evidence | Handwritten code and boundary | Smallest useful Concord change |
|---|---|---|---|
| F1 — Streaming multipart | `concord-codecs` multipart rustdoc explicitly holds all parts in memory. `multipart_is_lazy_nonreplayable_and_correct_on_wire` proves zero source polls at prepare, a non-cloneable native request, and one source read on generated send. | `UploadFile` and `StreamMultipart::encode_body` wrap reqwest's form stream and bind its boundary through `EncodedBody`. Multipart framing/laziness are generic; `data_type`, filename validation and external label are provider/SDK rules. | Add a streamed-part variant to `concord::multipart`, preserving one-shot ownership and runtime media. No declaration/macro change is needed. |
| F2 — Rotating refresh-token storage | `concord-auth` caches a sensitive header/lifetime, not a provider token document. The concurrent workflow proves one refresh for two calls and verifies the next grant uses the rotated refresh token. | `Session` keeps an async mutex over `Tokens`; acquisition stores rotations before applying access-token validation. `latest_tokens` exposes deliberate caller persistence. Refresh document fields, scopes and storage durability are provider/application concerns. Cache coordination and generation invalidation are shared. | An optional validated-token seed/insert API would remove the initial-token branch in the acquisition closure. Rotating provider documents and durable writes should remain outside the generic cache. |
| F3 — Multiple aligned quota windows | `rate-windows.json` distinguishes daily exhaustion with short capacity available and four exhausted windows at once. The rate-limit crate exposes a single observation per key and local windows start on admission. | `rate.rs` parses both comma pairs, maps them to four keys, translates quarter-hour/day wall time to monotonic resets, and submits observations. It never hardcodes provider defaults. Pair grammar/header names/route exemptions are Strava rules; atomic charging and conservative merging are shared mechanics. | A generic calendar-aligned reset helper or grouped observations API could shorten conversion/observation plumbing. A Strava-specific limiter would not belong in core. |
| F4 — Retry/auth/admission composition | The retry README deliberately separates replay permission from byte recreation and classification. GET retry tests show two attempts; upload timeout shows one. A 401 upload is not replayed, and a later GET refreshes. | One `Retry::run_tokio` factory acquires credentials, checks quota, dispatches once and observes the head. `Replay::Never` governs uploads, with no template clone. Strava 429 is always final because retrying can burn daily quota; the retry crate cannot infer which window caused it. Native retry/redirect settings must be explicit. | No missing retry scheduling capability was found. A facade transport-construction recipe and a documented composition helper would reduce repeated generic setup; provider replay/429 decisions should stay in the SDK. |
| F5 — Upload processing polls | The upload schema returns a job ID, not a pagination continuation. Poll tests verify a minimum interval, fixed ID, completion, processing failure, bound/resume and no second POST. | `poll` is a finite delay/status loop; `error` and `activity_id` decide its state. `concord-pagination` schedules traversal/job descriptors, not time-spaced rechecks of one asynchronous receipt, so its job engine was not bent into this loop. Poll state is Strava; counter/delay mechanics are generic. | A runtime-independent bounded polling helper with injected sleep, terminal projection and retained last value could replace the loop, without encoding provider states. |
| F6 — Typed HTTP errors and metadata | Malformed, oversized, interrupted and wrongly labelled error documents keep their original HTTP status in protocol tests. The framework provides `Received::document`, but no SDK error model. | `finish`, `Response`, `HttpFailure` and value-free `Debug` types preserve head, typed body error and quota projection. Shared bounded read/codec handling already does the difficult mechanics. Provider error envelopes and ambiguous-upload classification stay here. | An opt-in response/document result wrapper could remove the small generic success-vs-document branch. Provider models should not be added to Concord. |
| F7 — Pagination type ergonomics | Initial workflow compilation reported E0609 (`Incomplete` has `collected`, not `items`); corrected the caller. Clippy then reported `type_complexity` on `Pages<Activity,u32,Response<Vec<Activity>>,Error>`. | Added the public `ActivityPages` alias and `PageMeta`, moving items once and retaining metadata separately. Number advancement, checked overflow, empty-page termination and page-size rules remain SDK code; cap/prefix/resume mechanics are shared. | Document the nameable-alias plus metadata projection pattern beside the ordinary facade example; no runtime change is necessary. |
| F8 — Prepare bypasses send policy | Concord rustdoc explicitly leaves `prepare` unchanged when a send policy is adopted. In this SDK only generated `send` authenticates, admits and classifies. | README and rustdoc document the unauthenticated preparation boundary. `Session::client` uses the same strict transport and trusted origin; changing the generated client's origin is refused before credentials dispatch. | Generated `prepare` rustdoc could explicitly name the selected policy and say it is bypassed. An asynchronous decorated preparation API would be a larger optional addition. |

## Capability decisions

- `concord`/`concord-macros` declarations and generated `send()` are the primary API. `SendWith<T>` supplies the SDK result through an explicit result template; `Endpoint` keeps generated operation identity.
- `concord-codecs`, through facade features `json` and `form`, provides strict JSON and OAuth form encoding. `limit` bounds finite bodies; `document` bounds HTTP error documents. The built-in `multipart` feature was evaluated and not enabled because it cannot carry a one-shot streamed part. The custom encoder uses the public open format contract instead of editing a prepared endpoint to seed a different operation.
- `concord-auth` supplies Bearer validation/sensitivity, single-flight caching and generation-aware invalidation. It cannot persist or interpret Strava refresh-token rotation, which remains SDK state.
- `concord-pagination` supplies the named page facade, caps, prefix retention and resume reports. Page-number progression and conservative empty-page termination remain the adapter's responsibility.
- `concord-retry` with feature `tokio` supplies one attempt owner, `Replay::Never` and capped delays. Its strict `Retry-After` parser supplies optional 429 cooldowns. It expresses the required scheduling, but deliberately cannot decide provider write safety, identify calendar windows, refresh credentials or manage admission itself.
- `concord-ratelimit` with feature `tokio` supplies atomic admission across the four interpreted keys and monotonic observations. It cannot parse Strava comma fields, infer UTC/quarter-hour alignment, learn app-wide counts across processes, or decide how an unexplained 429 should gate requests. Those mappings and conservative fallback are recorded in `rate.rs` and README.

The native boundary is narrow: the send policy uses `Prepared::into_parts` to
insert the current sensitive Authorization header before `Decoder::execute`.
Streaming multipart uses reqwest byte/form types inside `Encode`; it does not
rewrite an unrelated endpoint URL or execute a second transport path. Decoding,
limits, metadata and operation identity still come from the declaration.

## Example assistance

**Example-assisted via required guide excerpts.** See the complete per-file ledger
in `PROVENANCE.md`. The SDK-source excerpts were FRED's transport settings, OpenAI's
buffered multipart, Drive's runtime multipart and Frankfurter's bounded error send
policy. Quickstart, shapes, native-edit and compat snippets were also read in the
guides; unused patterns are labelled there. No existing `*-sdk` source file was
opened directly. `examples/testserver/src/lib.rs` and its manifest were read
directly for the required fixture API, then used without modification.

## Observable diagnostics and validation

The initial offline no-run build succeeded and added only this local package to
`Cargo.lock`. An initial workflow compile failed on `Incomplete.items`; using
`collected` fixed it. First strict Clippy failed on the pagination return type;
introducing `ActivityPages` / `PageMeta` addressed it without suppressing the lint. An intermediate E0308 metadata projection mismatch was also corrected locally.
Final verification results are recorded in `VALIDATION.md`. This file contains
observable implementation costs and decisions, not private reasoning transcripts.
