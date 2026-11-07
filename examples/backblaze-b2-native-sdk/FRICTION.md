# Author observations

Frozen Concord baseline `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`. Paths are relative to this
SDK unless stated. No shared crate, check, script, doc or other example was modified; the
changes below are proposals only. "Observed" means reproduced by a command or test named in the
item; "inferred" is marked as such.

## Costs

1. **B2 issues its hosts at run time; a client has one static base.**
   `b2_authorize_account` returns `apiUrl` and `downloadUrl`, `b2_get_upload_url` an upload URL
   per bucket. The declaration (`src/wire.rs:17-63`) has one client with one scope per
   credential role (`account`, `storage`, `download`, `upload`), so `path "b2api/v4"`,
   `format Json`, `limit` and `send` are stated once; a counter-design with one client per host
   would repeat those four statements four times and add four `Config`/navigation surfaces for
   no wire difference. The cost is the binding: the SDK rebinds the client to each issued URL
   (`with_base_url`) and then navigates to the role scope (`src/lib.rs:128`,
   `src/upload.rs:748`). Nothing in the declaration ties `storage` to `apiUrl` or `download` to
   `downloadUrl`; reached from the unbound client, those scopes send the account token to the
   declared base (the account host). The declared base must also exist for roles that never use
   it. Hand-written: the `bind` closure, a `Session` that keeps the unbound client for upload
   rebinding, and the issued-URL rules (`src/model.rs:398`, no credentials/fragment, no base
   query, no `https`→`http` downgrade; unit tests `src/model.rs:421-456`). **Cause:** provider
   semantics (which host per role, URL trust) on top of a generic mechanic (a scope whose base
   is a run-time value). **Smallest change:** a scope-level base value, e.g.
   `scope storage(base api_url: Url, #[sensitive] header(AUTHORIZATION) token: String)`, making
   the binding part of navigation (`b2.storage(api_url, token)`) and leaving the trust rules in
   the SDK. Short of that, documenting "`with_base_url`, then the role scope" as the pattern.

2. **An opaque issued URL with a query has no declared representation.**
   G's documented `uploadUrl` is `https://pod-…/b2api/v4/b2_upload_file?cvt=…&bucket=…`.
   `with_base_url` refuses a query (observed: `src/wire.rs:190`,
   `the_documented_query_form_upload_url_is_not_a_base`; the path form is accepted). The SDK
   binds the upload scope to the URL's **origin**, prepares the endpoint with a placeholder
   `path "b2_upload_file"`, and moves the request with `Prepared::follow(upload_url)`
   (`src/upload.rs:742-760`). That keeps the declared headers, body, decoder, operation name
   and `Debug` withholding of the followed URL, and is checked on the wire for both forms
   (`tests/protocol.rs`: `upload_file_posts_to_the_issued_url_verbatim_with_length_and_sha1`,
   `a_path_form_upload_url_is_followed_verbatim`). But `follow`'s same-origin rule guards
   nothing here: the base was set to that origin a line earlier, so the trust decision is the
   SDK's alone, and the placeholder path is never sent. **Cause:** generic (a request target
   issued whole by the server, common to upload URLs and pre-signed URLs), not B2-specific.
   **Smallest change:** an endpoint placement for an issued absolute URL, e.g.
   `POST upload_file(target issued: Url, ...)`, replacing base and path, withheld from `Debug`
   like a followed URL, with scheme/credential checks; the SDK keeps deciding which issuer it
   trusts.

3. **The send policy is client-wide; two endpoints need their own handling.**
   `send B2Policy` (`src/wire.rs:25`) serves `authorize_account` and `get_upload_url` (one
   execution, a bounded B2 error document, decoded value with head). The upload needs `follow`
   and per-attempt evidence; the download needs a caller's byte limit and checksum
   verification; both go through `prepare()`/`receive()`. Their generated `send()` still exists
   and goes through the policy: the upload's would POST to the placeholder path at the account
   host (inferred from item 2; the declaration is private, so no caller can reach it). A probe
   adding `send B2Policy;` inside `scope upload` fails with "`send` is only allowed on the
   client: one send policy serves every endpoint" (observed, temporary edit, reverted).
   **Cause:** generic. **Smallest change:** none required for a private declaration; for a
   public one, a per-endpoint `send none;` (or scope-level `send`) so that an endpoint whose
   handling is the SDK's has no misleading `send()`.

4. **Run-time header names (`X-Bz-Info-<name>`) cannot be declared.**
   U documents custom file info as headers whose names are chosen per file. Query has
   `query(..)` with `QueryPairs`; headers have no equivalent. Probe: adding
   `header(..) info?: Vec<(String, String)>` to `upload_file` fails with "`header(..)` is not
   supported: a header placement names its field" (observed, reverted). The slice declares the
   one documented fixed name (`X-Bz-Info-src_last_modified_millis`, `src/wire.rs:55`) and
   excludes custom names (their grammar is on a page not read). Reading them back on download
   is a plain header iteration (`src/download.rs:131`). **Cause:** generic mechanic plus a
   provider grammar. **Smallest change:** `headers(..) x?: T` with a `HeaderPairs` trait
   mirroring `QueryPairs` (endpoint-only, never `#[sensitive]`, names validated by `http` at
   build time); the native alternative is `into_parts()` and `headers_mut()`.

5. **An exact-length one-shot request body is hand-written.**
   B2 requires `Content-Length` and refuses chunked transfer (U). The documented streaming
   recipe (`examples/native/src/upload.rs`) uses `reqwest::Body::wrap_stream`, which is
   chunked, and `concord::stream::Upload` serializes records lazily (chunked too). The SDK
   wrote `SizedStream` (`src/upload.rs:25-146`: an `http_body::Body` with
   `SizeHint::with_exact`, a `Mutex` reached through `get_mut` for `Sync`, a stream-error
   adapter, and a `Tally` of bytes handed out and failure). It is sent as
   `EncodedBody::with_content_type(reqwest::Body::wrap(stream), content_type)` with
   `request Encoded;` (`src/wire.rs:49-61`), so the caller's `Content-Type` stays bound to the
   body as `EncodedBody` documents, and Concord's documented "exact size hint → Content-Length"
   holds on the wire (observed: `a_one_shot_stream_is_sent_with_its_declared_content_length`,
   `content-length: 63`, not chunked). Observed along the way: hyper writes at most
   `Content-Length` bytes of a longer source, so truncating a crossing chunk in the wrapper was
   dead code (a mutation removing it survived every test) and was removed. **Cause:** generic.
   **Smallest change:** a `concord::stream` helper for "a one-shot byte stream of exactly N
   bytes" that fails the transfer when the source ends early or errs and reports whether it
   completed (which item 6 needs).

6. **`Delivery::Unknown` does not tell an incomplete request body from a lost answer.**
   Both a one-shot source that ended early and a closed connection after a complete request are
   `Delivery::Unknown` transport errors. For B2 they differ: a body that never reached its
   `Content-Length` cannot have been stored (`Evidence::SourceFailed`), a lost answer may have
   been (`Evidence::NoAnswer`). The SDK reads its own `Tally` (`src/upload.rs:779-784`).
   Tests: `a_source_shorter_than_declared_fails_before_the_body_completes` (source failure) and
   `a_consumed_stream_whose_answer_was_lost_may_have_been_stored` (complete body, lost answer);
   each fails under the matching mutation ("Test strength check"). **Cause:** a generic
   delivery fact that only the body knows. **Smallest change:** with item 5's helper, nothing in
   core; without it, no core change is justified (Concord cannot see inside an arbitrary
   `reqwest::Body`).

7. **`concord-retry`'s prepared-request adapter does not fit B2's upload; `run` does, with
   SDK bookkeeping.** `Retry::receive`/`receive_with` classify an **unread head**; B2's retry
   rule needs the error document's `code` (`401 expired_auth_token` vs `401 unauthorized`,
   `400 auth_token_limit` vs `400 bad_request`), and each retry must first obtain a new upload
   URL at another origin, which a re-executed template (even with an async hook) can only reach
   by a native URL rewrite outside `follow`. The SDK uses `Retry::run_tokio` with its own
   attempt future (`src/upload.rs:546-740`: shared state, an attempt record, `decide`), forcing
   `Replay::Never` for one-shot sources (the adapter's `BestEffort` semantics, re-stated).
   `Run` keeps only the last outcome, so per-attempt history is the SDK's, and an attempt
   abandoned at the deadline is detected as `report.attempts > records.len()`
   (`src/upload.rs:678`; test `an_attempt_abandoned_at_the_deadline_may_have_stored`).
   What the crate did give: the count/deadline/backoff bounds, replay permission separate from
   classification, and `StopReason::NotReplayable` exactly where B2's "a new URL cannot replay
   a consumed stream" lands (`a_new_upload_url_cannot_replay_a_consumed_stream`). **Cause:**
   provider semantics (body-borne codes, issued URLs) meeting a head-only classifier.
   **Smallest change:** a `completed` count (or `abandoned: bool`) in `Report` for `run`, as
   `ReceiveReport` already has for the adapter.

## Test strength check

Twelve single-line mutations of the SDK (one at a time, restored after each; script and output
kept outside the package under the worktree's `target/tmp/mutations/`) each made at least one
test fail: streams following the caller's replay permission, a 503 counted as "not stored",
skipping the download digest comparison, not recording a source failure, retrying a
stored-but-unverified answer, classifying a lost answer as a source failure, keeping a rejected
upload URL, not checking the 2xx file version, not percent-encoding the name, not retrying a
503, allowing an `https`→`http` downgrade, accepting an error document whose status contradicts
the HTTP status. One earlier mutation survived (truncating a chunk that crosses the declared
length: hyper already does it, item 5) and the dead code was removed.

## Fits (no change proposed)

8. **`concord-auth`**: `basic(key_id, key)` builds the sensitive Basic header with the colon and
   control refusals B2's `applicationKeyId:applicationKey` needs (`src/model.rs:30`); the wire
   value equals one computed with coreutils `base64`. `TokenCache` was not used: the account
   token arrives with its URLs and restrictions (a session, not a bare header and lifetime),
   and an upload token is bound to its URL and retired on rejection, not on time.
9. **Error documents**: `received.document::<Json, WireErrorDocument>(64 KiB)` gave every
   documented code, with the status kept for HTML, empty, oversized, interrupted and
   contradictory bodies (`tests/protocol.rs`: `every_documented_error_code_is_typed_with_its_status`,
   `an_error_body_that_is_not_b2s_keeps_the_status`). The "status always matches" rule is the
   SDK's (`src/error.rs`, `ProviderError::new`).
10. **Download**: `-> Bytes` with `response Binary` and `decode_limited_with_meta(limit)` gave a
    caller-bounded body with its head; verification is provider logic (`src/download.rs`).
11. **Empty upload body**: suspected that hyper sends no `Content-Length` for an empty body and
    added a native header; the wire showed `content-length: 0` without it (observed with the
    native edit removed: `an_empty_file_is_sent_with_content_length_zero` still passed), so the
    native edit was removed. reqwest 0.13.5 / hyper 1.12.0 as locked.
12. **Not used**: `concord-pagination` (no listing in this slice), `concord-ratelimit` (the
    four pages document caps as `403`s, no rate headers).

## Examples consulted

The result is example-assisted:

- `examples/dropbox-sdk/src/lib.rs`, `src/context.rs`, `src/wire.rs`, `src/error.rs`,
  `Cargo.toml`, and the first lines of `FRICTION.md` and `PROVENANCE.md`: the layout (private
  `wire.rs` declaration behind public SDK types), a send policy returning the value with its
  `ResponseMeta`, a boxed provider error with a bounded body, hand-written redacting `Debug`,
  and the shape of these two documents.
- `examples/native/src/upload.rs` and `examples/native/Cargo.toml` (grep): the one-shot,
  non-`Clone` source pattern and the fact that `Body::wrap_stream` is chunked (item 5).
- `examples/amazon-s3-sdk/Cargo.toml` and a grep of `src/sigv4.rs`: dependency and
  dev-dependency style only.
- `examples/testserver/src/lib.rs`: the loopback server API used by `tests/common`.
- Crate sources read (not examples): `crates/concord-core/src/{prepared,error,response,field}.rs`
  and the `EncodedBody` docs in `codec.rs`; `crates/concord-codecs/src/{encoded.rs,stream/upload.rs}`
  (`Body::wrap` with a `Mutex` for `Sync`); `crates/concord-retry/src/lib.rs`;
  `crates/concord-auth/src/credentials.rs`; registry sources of reqwest 0.13.5 (body, retry,
  client) and hyper 1.12.0 (`proto/h1/{role,dispatch,encode}.rs`) for `Content-Length`
  behaviour; `sha1_smol` 1.0.1 and `sha1` 0.10.7 manifests for the dependency choice.
- Docs read: `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`,
  `docs/RECIPES.md`, and the READMEs of `concord-retry`, `concord-auth`, `concord-ratelimit`,
  `concord-codecs` and (first part) `concord-pagination`.
