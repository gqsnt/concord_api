# Integration record

## Between waves 2-3

Coordinator integration of the between-wave library refinements (`send Policy;`,
`Prepared::follow`, `Received::decode_limited`) at Concord `1f71432c`. Role: integrator,
Claude Opus 5.5 agent session (requested model; this record does not re-derive it from
harness metadata). Not an independent review: nobody has reviewed this change yet.

**Verdict: partially adopted.** `Received::decode_limited` was adopted: 2xx bodies are now
bounded by `SUCCESS_BODY_LIMIT` (16 MiB), which closes the author's friction 5 ("finite success
decoding has no bounded typed consumer"). The send policy was **not** adopted, and the
declaration stays private. Decisive reason: a public declaration with a Slack-aware policy was
built and measured. Its `send()` does check `ok`, but the declaration's own
`prepare()?.send()` still returns an `ok: false` envelope as success. It also exposes an
unvalidated `limit: u16`, and it adds code instead of removing it (below). With the
declaration private, callers never see a generated `send()`, so a policy would change nothing
for them. `Prepared::follow` does not apply: Slack continues with a body cursor in a query
value, not a server URL.

### Declaration

Unchanged (`src/wire.rs`), still private (`mod wire;`, not re-exported):

```rust,ignore
concord::api! {
    /// Declared transport for this SDK. Use [`crate::Client`] for Slack-aware results.
    pub client SlackApi(
        /// Raw bearer token, without the `Bearer ` prefix; redacted by Concord.
        #[sensitive] token: String,
    ) {
        base "https://slack.com/api";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;

        GET users_list(
            query limit: u16,
            #[sensitive] query cursor?: String,
            query include_locale?: bool,
            query team_id?: String,
        ) -> Envelope<UsersPayload> { path "users.list"; }
        // conversations_info ...
    }
}
```

### Adapter code

The one change, in `execute` (`src/lib.rs`):

```rust,ignore
// before
    let envelope = received.decode().await.map_err(|error| Error::Concord {
        error,
        meta: Some(Box::new(meta.clone())),
    })?;
// after
    let envelope = received
        .decode_limited(SUCCESS_BODY_LIMIT)
        .await
        .map_err(|error| Error::Concord {
            error,
            meta: Some(Box::new(meta.clone())),
        })?;
```

plus the documented constant:

```rust,ignore
/// Maximum bytes of a 2xx body decoded per request (16 MiB). An SDK safety bound against an
/// endpoint that never stops sending, not a Slack-documented size: ...
pub const SUCCESS_BODY_LIMIT: usize = 16 * 1024 * 1024;
```

16 MiB is an SDK choice, not a provider fact; Slack documents no response size. It bounds
memory per page. The README says to lower `UsersOptions::limit` if a page reaches it. No test
measured real `users.list` page sizes.

Lines of SDK adapter code (counted by script: non-blank lines that are not `//` comments,
total in brackets):

| | before | after |
| --- | --- | --- |
| `execute` (`src/lib.rs`) | 51 (52) | 54 (55); +3 from rustfmt wrapping the longer call |
| `SUCCESS_BODY_LIMIT` | none | 1 code + 5 doc |
| `src/lib.rs` total code | 197 | 201 |

### One caller call

Unchanged (`README.md`):

```rust,ignore
    match slack.conversation_info("C_SYNTHETIC_GENERAL", ConversationOptions {
        include_num_members: Some(true),
        ..ConversationOptions::default()
    }).await {
```

The only new caller-visible behaviour: a 2xx body over 16 MiB is now
`Error::Concord { error, meta: Some(_) }` with `error.kind() == ErrorKind::Limit`. It used to be
buffered whole.

### The evaluated alternative: public declaration with a bounded send policy

Built in the working tree, compiled, tested and measured, then reverted; it is not committed.
The policy needs a bound on the payload. Excerpt:

```rust,ignore
/// What a Slack method's payload yields once `ok` is checked.
pub trait Payload: Send + 'static {
    type Value;
    fn into_value(self) -> Result<Self::Value, EnvelopeProblem>;
}

impl Payload for UsersPayload {
    type Value = UsersPage;
    fn into_value(self) -> Result<UsersPage, EnvelopeProblem> {
        Ok(UsersPage {
            members: self.members.ok_or(EnvelopeProblem::MissingMembers)?,
            cache_ts: self.cache_ts,
        })
    }
}
// impl Payload for ConversationPayload { ... MissingChannel ... }

pub struct SlackPolicy;

impl<T: Payload> SendWith<Envelope<T>> for SlackPolicy {
    type Output = Result<Response<T::Value>, Error>;

    async fn send<E: Endpoint<Envelope<T>> + Send>(endpoint: E) -> Self::Output {
        let response = crate::execute(endpoint).await?;
        match response.value.into_value() {
            Ok(value) => Ok(Response { value, meta: response.meta,
                warning: response.warning, response_metadata: response.response_metadata }),
            Err(problem) => Err(Error::Envelope { problem, meta: Box::new(response.meta) }),
        }
    }
}
```

The declaration gained `send SlackPolicy;` and became `pub mod wire`. `Client::users_page` and
`Client::conversation_info` shrank to a builder chain ending in `.send().await`.

Observed:

- It compiles. `clippy -D warnings` was clean (with `#![allow(missing_docs)]` on the prototype
  module). All 39 existing tests and 4 doctests passed unchanged. A bound on `T` works with the
  generated `send()`.
- A temporary probe test (deleted) used the public declaration against a 200
  `{"ok":false,"error":"channel_not_found"}`: generated `send()` returned an error, but
  `api.conversations_info("C1").prepare().unwrap().send().await` returned
  `Ok(envelope)` with `envelope.ok == false`. A public declaration does not remove the risk the
  author recorded; it moves it from `send()` to `prepare()?.send()`.
- The same probe built `users_list(0)` and got the query `limit=0`. `PageSize` (1–1000) is a
  `Client` rule, and the generated argument is a raw `u16`.
- Code size: `src/lib.rs` + `src/wire.rs` non-comment code went from 240 to 254 lines (+14),
  before the rustdoc that `Envelope`, both payload types, `Payload` and `SlackPolicy` would
  need as public items. `execute` stays; the policy calls it.
- Maintainer diagnostic, observed with a temporary `GET auth_test() -> Envelope<AuthPayload>`
  that lacked an impl:

```text
error[E0277]: the trait bound `AuthPayload: Payload` is not satisfied
   --> examples/slack-sdk/src/wire.rs:77:1
    |
 77 | / concord::api! {
...
note: required for `SlackPolicy` to implement `SendWith<Envelope<AuthPayload>>`
```

  The diagnostic is clear about the missing `Payload` impl; its span is the whole `api!`
  invocation, not the new `GET` line.

### What a maintainer edits to add an operation

Unchanged by this integration. They add the `GET` with an `Envelope<NewPayload>` output and a
`NewPayload` struct in `src/wire.rs`, then a `Client` method that calls
`execute(self.api.new_method(..))` and checks the payload's required field (about 20 lines, like
`conversation_info`). The success-body bound applies to it automatically, because it goes
through `execute`. With the rejected alternative, the `Client` method would shrink to a chain,
but a `Payload` impl (about 8 lines) would be added and the new endpoint's raw arguments would
be public.

### Test changes, each explained

No existing test changed. One test was added: `tests/protocol.rs`
`success_body_is_decoded_up_to_the_limit_and_refused_beyond`. A synthetic `ok: true` page of
exactly `SUCCESS_BODY_LIMIT` bytes decodes. One byte more is `Error::Concord` with kind
`Limit`, operation `GET SlackApi.users_list`, status 200 and the request id kept. Each case
sends one request. Mutation check: with `decode_limited(SUCCESS_BODY_LIMIT)` changed back to
`decode()`, this test failed (protocol.rs:529, the `unwrap_err` on the over-limit case), and
the existing `interrupted_success_body_is_not_a_completed_page` still passed.

### Verification (observed)

At `1f71432c` before the change: 4 cursors + 6 first_use + 16 protocol + 9 workflows + 4
doctests passed.

After the change (`CARGO_BUILD_JOBS=6`):

- `cargo test --locked -p concord-sdk-slack`: 4 cursors + 6 first_use + 17 protocol (16
  unchanged + 1 new) + 9 workflows + 4 doctests passed, 0 failed.
- `cargo clippy --locked -p concord-sdk-slack --all-targets -- -D warnings`: clean.
- `cargo doc --locked -p concord-sdk-slack --no-deps`: no warnings.
- `cargo fmt -p concord-sdk-slack -- --check`: clean.

Offline only; no Slack workspace was contacted.
