# Post-wave-1 integration (coordinator record)

Two library refinements arrived after wave 1: `concord::Endpoint<T>` (implemented by every
generated endpoint, so one SDK trait can send all of them) and `Received::bytes_limited`. This
records what each changed here. "Before" is the author's code at `6a9dcb19`.

## `concord::Endpoint`: evaluated, measured, not adopted

The per-operation methods stay: `Twilio::create_message`/`get_message` and their `prepare_*`
companions (now returning `TwilioError`, see the consumer findings below).

### Why

A prototype of the adopted form was compiled and run against this crate's three test suites,
then discarded. What it took and what it showed (observed):

1. **The client cannot be the generated type.** The generated `TwilioApi::new(http,
   TwilioApiConfig { authorization: HeaderValue, account_sid })` takes a raw header, while the
   documented `Twilio::new(http, account_sid, BasicAuth)` takes validated credentials. So `Twilio`
   stays a wrapper, and the generated operations must be reached through it: the prototype used
   `impl Deref for Twilio { type Target = TwilioApi; .. }` (an accessor such as
   `client.api().get_message(sid).call()` was the alternative). Neither is needed today.
2. **A per-operation input rule loses its home.** The empty-SID refusal (consumer friction 4) is
   one `is_empty()` check in `prepare_get_message`. A generic adapter knows no SID, so the
   prototype checked the prepared URL instead (a last path segment `.json` names no resource),
   and that check runs only in `.call()`: the prototype's own test
   `an_empty_sid_is_refused_before_sending` failed on `client.get_message("").prepare()`, which
   built `GET .../Messages/.json`. The generated `send()` would bypass it as well.
3. **The caller path got longer and gained a trap.** Every send became
   `client.get_message(sid).call().await`; without `use concord_sdk_twilio::Call` rustc reports
   E0599 with a correct import hint. But code that passed the operation itself as a future
   (`tokio::time::timeout(d, client.get_message("SM_a"))` in `tests/first_use.rs`) failed with
   E0277 "`TwilioApiGetMessage` is not a future", with no hint towards `.call()`.
4. **What it saved.** The four methods (39 lines with their docs) were replaced by a `Deref`
   impl, a re-export and the trait (24 lines with one-line docs; documented like the methods it
   would be about the same size). The tests needed 69 call sites rewritten, 3 `prepare_*` sites,
   2 future-passing sites and 3 imports (`first_use.rs`: 21 lines removed, 82 added after
   rustfmt split the longer chains).

For a two-operation slice, the saving does not pay for a `Deref` wrapper, a weaker input rule and
a longer call. The trait would start to pay when operations multiply (see the maintenance table),
or if the generated client could take the SDK's own construction (see the report's library gap).

### Code

Declaration (unchanged; the fetch path's `sid` is no longer `#[sensitive]`, see below):

```rust
            /// Creates a single text message.
            POST create_message(body message: CreateMessage) -> Message {
                path "Messages.json";
                request Form;
            }
```

SDK adapter, kept (`src/lib.rs`, two of the four methods; `error::send` is the shared
receive/classify/decode step and was already generic):

```rust
    pub async fn get_message(
        &self,
        sid: impl Into<String>,
    ) -> Result<Response<Message>, TwilioError> {
        error::send(self.prepare_get_message(sid)?).await
    }
```

```rust
        let sid = sid.into();
        if sid.is_empty() {
            return Err(TwilioError::EmptySid {
                operation: <wire::TwilioApiGetMessage as Endpoint<Message>>::operation(),
            });
        }
        Ok(self.api.get_message(sid).prepare()?)
```

(`Endpoint` is used here only for the operation's identity without building a request.)

Prototype adapter, measured and discarded:

```rust
impl std::ops::Deref for Twilio {
    type Target = wire::TwilioApi;
    fn deref(&self) -> &wire::TwilioApi {
        &self.api
    }
}

pub use wire::{TwilioApi, TwilioApiCreateMessage, TwilioApiGetMessage};
```

```rust
impl<T: Send, E: Endpoint<T> + Send> Call<T> for E {
    async fn call(self) -> Result<Response<T>, TwilioError> {
        let prepared = self.prepare()?;
        // Twilio names a resource `<Sid>.json`: a last segment `.json` means an empty SID.
        if prepared
            .request()
            .url()
            .path_segments()
            .and_then(|mut s| s.next_back())
            == Some(".json")
        {
            return Err(TwilioError::EmptySid {
                operation: E::operation(),
            });
        }
        error::send(prepared).await
    }
}
```

One caller call (`tests/workflows.rs`), kept:

```rust
    let observation = client.get_message(&submission.value.sid).await.unwrap();
```

prototype:

```rust
    let observation = client
        .get_message(&submission.value.sid)
        .call()
        .await
        .unwrap();
```

### Adding an operation

| Kept design | With the trait |
|---|---|
| 1. the `api!` entry | 1. the `api!` entry |
| 2. `Twilio::op` (one line: `error::send(self.prepare_op(..)?)`) with its docs | — |
| 3. `Twilio::prepare_op` (one line, plus any input rule such as the empty SID) with its docs | an input rule has no per-operation place: a URL-shape rule in `call()`, or a validating newtype in the declaration (every caller then constructs it, since generated arguments take only `impl IntoValue<T>`) |
| 4. README | 2. README |

## `Received::bytes_limited`: adopted

Before (`src/error.rs`), a chunk loop over the native response:

```rust
    let mut bytes = BytesMut::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ErrorDocumentProblem::Read)?
    {
        if chunk.len() > MAX_ERROR_BODY - bytes.len() {
            return Err(ErrorDocumentProblem::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
```

After:

```rust
    let bytes = received
        .bytes_limited(MAX_ERROR_BODY)
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::Limit => ErrorDocumentProblem::TooLarge,
            _ => ErrorDocumentProblem::Read,
        })?;
```

`read_error` now takes the `Received` instead of `received.into_response()`, so the error body
is read inside Concord's boundary. The content-type and context checks before reading, the
64 KiB limit (exactly 65,536 bytes accepted, 65,537 `TooLarge`) and the typed reasons are
unchanged; `error.rs` is 7 lines shorter (+12 −19) and no longer imports `BytesMut` or
`ResponseHead`.

## Consumer findings (`FIRST_USE.md`)

1. Two `Error` types: the SDK error is `TwilioError` in every signature; both `prepare_*` return
   it too; only `with_base_url` returns `concord::Error`. `Error` stays as a `#[doc(hidden)]`
   alias.
2. `TwilioApi` in diagnostics: documented on `Twilio` and in the README.
3. `Debug` of `Message` shows `sid`, `status`, `error_code` (`Response<Message>` shows its value).
   The message SID is an identifier, not a credential: the fetch path no longer marks it
   `#[sensitive]`; the account SID, addresses, body and `error_message` stay withheld.
4. An empty SID is `TwilioError::EmptySid` before sending (one row of
   `first_use.rs::b_a_sid_is_always_one_path_segment` changed accordingly).

# Between waves 2-3

Coordinator evaluation of the client **send policy** (`send P;`: every generated `send()` returns
`<P as concord::SendWith<T>>::Output`) for this SDK. "Before" is `1f71432c`. The source is
unchanged; this section is the record.

## Verdict: not adopted

A send policy fixes the one thing the wave-1 `Call` prototype added for callers (an import), but
not the two that decided against it: the validated construction keeps `Twilio` a wrapper, and the
empty-SID rule becomes weaker as a policy check. The kept design has the shortest correct call
(`client.get_message(sid).await`) and refuses an empty SID on both of its paths.

## Prototype (compiled and run, then reverted)

`src/lib.rs` was changed to the adopted form: `send crate::Classified;` on the private
declaration, the policy below, `impl Deref for Twilio { type Target = wire::TwilioApi; .. }`, a
re-export of the generated types, and the four per-operation methods removed. A temporary test
(`tests/probe_tmp.rs`, removed) ran against the fixtures:

```rust
pub struct Classified;

impl<T: Send> SendWith<T> for Classified {
    type Output = Result<Response<T>, TwilioError>;

    async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare()?;
        // Twilio names a resource `<Sid>.json`: a last segment `.json` means an empty SID.
        if prepared
            .request()
            .url()
            .path_segments()
            .and_then(|mut s| s.next_back())
            == Some(".json")
        {
            return Err(TwilioError::EmptySid {
                operation: E::operation(),
            });
        }
        error::send(prepared).await
    }
}
```

Observed:

1. `client.create_message(message()).send().await` and `client.get_message(SID).send().await`
   returned `Response<Message>` with no import (the policy works as documented).
2. `client.get_message("").send().await` was `TwilioError::EmptySid`, displayed
   `GET TwilioApi.get_message: empty message SID (nothing sent)`, with no request.
3. `client.get_message("").prepare()` returned `Ok`, a request for
   `/2010-04-01/Accounts/AC_fixture_account/Messages/.json`. A policy sees only the built
   request, so the rule runs in `send()` and nowhere else; the generated `prepare()` (and
   `Prepared::send` after it) bypass it. Today both `get_message` and `prepare_get_message`
   refuse it (`tests/protocol.rs` `an_empty_sid_is_refused_before_sending`).
4. The existing suites stopped compiling with 78 errors: 71 `error[E0277]: 'TwilioApiGetMessage'
   is not a future` / `'TwilioApiCreateMessage' is not a future` (every
   `client.op(..).await`, each with the help "remove the `.await`", which is wrong: the fix is
   `.send()`), 3 `E0599` for the removed `prepare_*` methods, and 4 follow-on `E0277` (`str` is
   not `Sized`) where the error type could no longer be inferred.

## The empty-SID rule: stays a domain method

Decided for the domain method. As a policy check it would have to recognise the empty SID from
the URL shape (a last segment `.json`), which is an inference about the built request rather
than the input rule itself, and it would not cover `prepare()`, the path the SDK documents for
inspection. In `prepare_get_message` it is one `is_empty()` on the argument, before anything is
built, and `get_message` goes through it.

## Lines of SDK adapter code

| | lines | code (no blank/comment lines) |
|---|---|---|
| kept: `create_message`, `get_message`, `prepare_create_message`, `prepare_get_message` (with docs) | 44 | 30 |
| prototype: policy + `Deref` + re-export (one-line docs), plus `send crate::Classified;` | 29 + 1 | 26 + 1 |

Documented like the methods, the policy and the `Deref` would be about the same size as what
they replace.

## One caller call

Kept (`tests/workflows.rs`):

```rust
    let observation = client.get_message(&submission.value.sid).await.unwrap();
```

prototype:

```rust
    let observation = client
        .get_message(&submission.value.sid)
        .send()
        .await
        .unwrap();
```

## Adding an operation

| Kept design | With a send policy |
|---|---|
| 1. the `api!` entry | 1. the `api!` entry |
| 2. `Twilio::op` (one line: `error::send(self.prepare_op(..)?)`) with its docs | — (reached through `Deref`, sent by the policy) |
| 3. `Twilio::prepare_op` (one line, plus any input rule) with its docs | an input rule goes into the policy as a check on the built request (seen by `send()` only), or into a validating newtype every caller constructs |
| 4. README | 2. README |

The policy would start to pay as operations multiply, if their input rules can be expressed in
the declaration's types; for this two-operation slice it does not.

## Unchanged

`TwilioError` stays the SDK error in every operation signature (`Error` the hidden alias),
`with_base_url` alone returns `concord::Error`, and the `Debug` decisions of the post-wave-1
integration hold (`Message` shows `sid`, `status`, `error_code`; the message SID is not
`#[sensitive]`, the account SID, credentials, addresses, body and `error_message` are withheld).
`Received::bytes_limited` was already adopted; `Received::decode_limited` was not (Twilio
documents no size for a Message resource), and `Prepared::follow` has no use in this slice.
Tests at the recorded state: `first_use` 22, `protocol` 21, `workflows` 4, doctests 2 — all pass
after the prototype was reverted (`git checkout -- src/lib.rs`, probe test removed).

# Between waves 3-4 (ResponseMeta)

Concord gained `concord::ResponseMeta` (an owned head: status, version, header map, operation;
`Received::meta()`; `Debug` prints header names only; AUTHOR_GUIDE §5.11). Base `92775757`.
Role: implementer, a Claude agent session spawned by the coordinator (model and effort not
re-derived from harness metadata here). Not an independent review.

## Verdict: adopted (re-export)

`src/error.rs` now has `pub use concord::ResponseMeta;` in place of its own struct, `capture`,
`headers()` and `Debug`; `send` captures with `received.meta()`. Decisive: the SDK type had
exactly Concord's four values and already kept `headers` behind a method with a names-only
`Debug`; the only SDK-specific text was documentation, now on the re-export. Callers already
wrote `meta.headers()`; the other three fields become methods too, so access is uniform.

## Lines (non-blank, non-comment, from `git diff`; this file excluded)

| | removed | added |
|---|---|---|
| `src/` | 36 | 6 |
| `tests/` | 30 | 31 |
| `README.md` (doctest line) | 1 | 1 |

## Caller-visible changes

- Fields become methods (README example):

  ```rust,ignore
  assert_eq!(submitted.meta.status.as_u16(), 201);   // before
  assert_eq!(submitted.meta.status().as_u16(), 201); // after
  ```

  Likewise `meta.version()` and `meta.operation()`; `meta.headers()` is unchanged.
- `Response::meta`, `TwilioError::meta()` and `TwilioError::Provider { meta }` are now
  `concord::ResponseMeta` (the same type as in the other adopting SDKs); `ResponseMeta::new(head,
  operation)` builds one (before, the private `headers` field prevented building one at all).
- `Debug` (from the code): fields are `status, version, header_names, operation` (was `status,
  version, operation, header_names`) and the operation prints by name
  (`Some("TwilioApi.create_message")`, was `Some(Operation(POST TwilioApi.create_message))`).
  No header value, as before. `Display` of errors is unchanged.

## Test edits, by kind

1. Field → method, mechanical (30 lines in `tests/first_use.rs`, `tests/protocol.rs`,
   `tests/workflows.rs`): `.status` → `.status()`, `.operation` → `.operation()`. Expected
   values unchanged.
2. Added, one assertion
   (`tests/first_use.rs::a_submit_sends_the_documented_form_and_returns_the_queued_resource`):
   `Debug` of `submitted.meta` does not contain the `Twilio-Request-Id` value.

## Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-twilio`, run twice: `first_use` 22,
`protocol` 21, `workflows` 4, doctests 2 passed, 0 failed (the same counts as at `92775757`).
`cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check` and `cargo doc --no-deps`
(no warnings): clean. Offline only.
