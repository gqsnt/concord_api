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
