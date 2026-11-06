# Integration record

## Between waves 2-3

Coordinator integration of the between-wave library refinements (`send Policy;`,
`Prepared::follow`, `Received::decode_limited`) at Concord `1f71432c`. Role: integrator,
Claude Opus 5.5 agent session (requested model; this record does not re-derive it from
harness metadata). Not an independent review: nobody has reviewed this change yet.

**Verdict: adopted.** The client declares `send SendPolicy;`, so the generated `send()` is
the typed adapter and the two inherent `fetch()` methods are gone. Decisive reason: the
consumer's friction (FIRST_USE.md, friction 3: "two terminal methods with different error
contracts") goes away because only one terminal method remains, and adding an operation no
longer needs a forwarding method. `Prepared::follow` does not apply (no pagination).
`decode_limited` was not adopted: no friction asked for a success-body cap, and the README's
"no SDK byte cap" statement still holds.

### Declaration

Before (`src/lib.rs`, `1f71432c`):

```rust,ignore
    /// the operations themselves declare `/v2`. Use `fetch()` for typed provider errors
    /// and response metadata; generated `send()` returns just `Rate` and Concord errors.
    pub client Frankfurter() {
        base "https://api.frankfurter.dev";
        format Json;
```

After:

```rust,ignore
    /// the operations themselves declare `/v2`.
    ///
    /// Every request's `send()` returns `Result<`[`RateResponse`]`, `[`Error`]`>` (this
    /// crate's error, through [`SendPolicy`]): typed provider errors and response metadata.
    /// Two calls return [`concord::Error`] instead: `with_base_url` and `prepare()` (no
    /// request is sent). Signatures show both as a bare `Error`; this crate's [`Error`]
    /// implements `From<concord::Error>`, so `?` converts.
    pub client Frankfurter() {
        base "https://api.frankfurter.dev";
        send SendPolicy;
        format Json;
```

The two operations are unchanged.

### Adapter / policy code

Before: one forwarding method per operation, plus the shared function:

```rust,ignore
impl FrankfurterLatestPair {
    /// Executes once, returning a complete rate or a typed error with available metadata.
    /// ...
    pub async fn fetch(self) -> Result<RateResponse, Error> {
        fetch(self).await
    }
}

impl FrankfurterHistoricalPair {
    /// ...
    pub async fn fetch(self) -> Result<RateResponse, Error> {
        fetch(self).await
    }
}

async fn fetch(endpoint: impl Endpoint<Rate>) -> Result<RateResponse, Error> {
    let received = endpoint.prepare()?.receive().await?;
    // ... status branch, bounded typed error document, decode (unchanged below)
}
```

After: the same body, moved into the policy:

```rust,ignore
pub struct SendPolicy;

impl SendWith<Rate> for SendPolicy {
    type Output = Result<RateResponse, Error>;

    async fn send<E: Endpoint<Rate> + Send>(endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        let meta = ResponseMeta::capture(&received.head(), received.operation());
        if !received.status().is_success() {
            // Status is authoritative even when the document is absent, invalid or inconsistent.
            let document = match <Json as Decode<Problem>>::check_content_type(&received.head()) {
                Err(error) => Err(ErrorBodyError::Metadata(error)),
                Ok(()) => match received.bytes_limited(ERROR_BODY_LIMIT).await {
                    Err(error) => Err(ErrorBodyError::Read(error)),
                    Ok(bytes) => <Json as Decode<Problem>>::decode_body((), bytes)
                        .map_err(ErrorBodyError::Decode),
                },
            };
            return Err(Error::Provider(Box::new(ProviderError { meta, document })));
        }
        match received.decode().await {
            Ok(rate) => Ok(RateResponse { rate, meta }),
            Err(source) => Err(Error::Concord {
                source,
                meta: Some(Box::new(meta)),
            }),
        }
    }
}
```

The policy implements `SendWith<Rate>` only, not a generic `SendWith<T>`: both operations
return `Rate` and `RateResponse { rate, meta }` is specific to it.

Lines of SDK adapter code (`src/lib.rs`; counted by script: non-blank lines that are not
`//` comments, total in brackets):

| | before (lines 59-100) | after (lines 65-101) |
| --- | --- | --- |
| code | 32 | 26 |
| doc comments | 8 | 9 |
| total | 42 | 37 |

Per-operation adapter code: before, 2 forwarding `impl` blocks (8 lines each including
docs); after, none.

### One caller call

Before (`README.md`):

```rust,ignore
let latest = api.latest_pair("EUR", "USD").fetch().await?;
```

After:

```rust,ignore
let latest = api.latest_pair("EUR", "USD").send().await?;
```

Same type (`Result<RateResponse, Error>`), no import. The Concord-only result that `send()`
used to return is still `api.latest_pair("EUR", "USD").prepare()?.send().await`
(`Result<Rate, concord::Error>`).

### The two types shown as `Error`

Observed in `cargo doc -p concord-sdk-frankfurter --no-deps`: `with_base_url` and `prepare`
still render `Result<Self, Error>` / `Result<Prepared<Rate>, Error>` with an unlinked `Error`
(`concord::Error`). Those signatures are generated; the SDK cannot rename the type there. What
changed:

- `send()` no longer returns `concord::Error`, so the only calls that do are the two that send
  nothing (`with_base_url`, `prepare`).
- The client rustdoc, the `Error` rustdoc, `Error::Concord::source`, `ErrorBodyError::Read`
  and the README now say which `Error` each one is, with links, and that `?` converts
  (`From<concord::Error>`).

Not done: re-exporting `concord::Error` under another name (it would add API surface and
rustdoc would still print `Error` in the generated signatures).

Observed rustdoc for the generated `send()`:

```text
pub fn send(
    self,
) -> impl Future<Output = <SendPolicy as SendWith<Rate>>::Output> + Send
```

The concrete `Result<RateResponse, Error>` appears only on `SendPolicy`'s impl and in the SDK's
own client documentation, which is why the client doc states it. Library note: the generated
signature does not name the policy's output; a caller reading one request type's page has to
follow `SendPolicy`.

### What a maintainer edits to add an operation

- Before: the `GET ... -> Rate { path ...; }` declaration, **and** an inherent
  `impl NewRequest { pub async fn fetch(self) ... }` forwarding block; without it the new
  operation has only the Concord-error `send()`, the friction above.
- After: the declaration only, if it returns `Rate`. Another output type needs another
  `SendWith<NewOutput>` impl (or a generic one). The compiler reports the omission at the
  `send` statement. Observed with a temporary `GET currencies() -> Vec<String>` (reverted, not
  committed):

```text
error[E0277]: the send policy `SendPolicy` does not send a `Vec<std::string::String>` output
  --> examples/frankfurter-sdk/src/lib.rs:34:14
   |
34 |         send SendPolicy;
   |              ^^^^^^^^^^ named by the client's `send` statement; it needs `SendPolicy: SendWith<Vec<std::string::String>>`
```

It is emitted twice (once spanned at `send SendPolicy;`, once on the whole `api!` invocation).
Neither span points at the `GET currencies` line; the output type in the message is the clue.

### Test changes, each explained

No expected value was changed.

1. `.fetch()` became `.send()` at every call site: 33 in `tests/first_use.rs`, 17 in
   `tests/protocol.rs`, 9 in `tests/workflows.rs`, 5 in `README.md`. Mechanical rename;
   result type and expected values unchanged.
2. `tests/first_use.rs`: `a_generated_send_skips_the_typed_error_adapter` was renamed to
   `a_prepared_send_skips_the_typed_error_adapter`. It checked that generated `send()`
   returns `concord::Error` kind `Status` with the unread 422 body. That is the behaviour this
   change removes on purpose. The same assertions (kind `Status`, status 422, endpoint
   `Frankfurter.latest_pair`, exact body bytes) now run on `prepare()?.send()`, the
   Concord-only path. One assertion was added: generated `send()` on the same 422 gives
   `Error::Provider` with status 422.
3. `tests/workflows.rs` (maintenance test
   `unknown_provider_attribution_preserves_existing_rate_callers`): the two
   `latest.send()` / `historical.send()` calls that checked the Concord-only path against the
   same `Rate` now call `.prepare().unwrap().send()`. Expected `Rate` and the five recorded
   targets are unchanged.

### Verification (observed)

At `1f71432c` before the change: `cargo test --locked -p concord-sdk-frankfurter`: 24
first_use + 18 protocol + 5 workflows + 2 doctests passed.

After the change, with `CARGO_BUILD_JOBS=6`:

- `cargo test --locked -p concord-sdk-frankfurter`: 24 first_use + 18 protocol + 5 workflows
  + 2 doctests passed, 0 failed.
- `cargo clippy --locked -p concord-sdk-frankfurter --all-targets -- -D warnings`: clean.
- `cargo doc --locked -p concord-sdk-frankfurter --no-deps`: no warnings.
- `cargo fmt -p concord-sdk-frankfurter -- --check`: clean.

Offline only; no live provider calls.
