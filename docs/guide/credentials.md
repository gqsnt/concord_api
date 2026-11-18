# Credentials

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Send a body](bodies.md) · next: [Middleware](middleware.md)*

This page shows how a client sends credentials. A key or a token is a value, placed like any other value; a signature, or a token that must be renewed, is middleware.

The work is split as it is for [retries](retries.md) and [pages](pages.md): the description states the fact about the API (where the credential goes, what signs each request), a Rust value does the work (a `Secret`, a middleware layer), and the application supplies the credential itself, as an input.

## A credential is a value

**A credential that is a value is placed like any other value.** A key, a bearer token, a username and a password: each is an input, and the description specifies where it goes, with the statements it already has.

| The API wants | In the description |
|---|---|
| A key in a header | `headers { "x-riot-token" = key; }` |
| A bearer token | `headers { "authorization" = "Bearer {token}"; }` |
| HTTP Basic | `headers { "authorization" = basic(user, password); }` |
| A key in the query | `query { "api_key" = key; }` |
| A token in the URL itself (Telegram: `https://api.telegram.org/bot<token>/`) | `base "https://api.telegram.org/bot{token}";`: a `Secret` in a placeholder of the `base` ([Telegram](../examples/telegram.md)). |
| A secret in the body (a token endpoint) | `body form { grant_type; client_id; client_secret; }`: a `Secret` may be an entry of a body, and is sent as its string ([a body that is sent](bodies.md#a-body-that-is-sent)). |
| A client certificate | Nothing. It belongs to the connection: set it on the HTTP client, with `concord::Http::with(…)` ([the client](calling.md#the-client)). |
| A signature | `middleware Signed::new(key.clone());` ([a signature](#a-signature)). |
| A token that expires and must be renewed | Nothing in the description: `concord::auth::Renewed`, a middleware layer that Concord ships, added with `with_middleware(…)` ([a token that is renewed](#a-token-that-is-renewed)). |

**Its type is what makes it a credential.** The input is a `concord::Secret`, and a value that holds a `Secret` stays secret wherever it is placed:

| | |
|---|---|
| In a header | The header is marked sensitive, so `reqwest` and `http` do not print its value. |
| In a query, a path or a `base` | It becomes part of the URL. Concord never prints a URL. A `reqwest::Response` that you get from `send_raw()` shows its URL in `{:?}`. |
| In a next-page URL | A query pair whose value is a `Secret` is also sent when the call moves to a next-page URL (`by_link()`, `by_next_url(…)`), because a server does not put your key in its links. |
| In an error, a log, `{:?}` | Never. A `Secret` prints as `Secret(…)` and has no `Display`. |
| In a template | `"Bearer {token}"` with a secret in a placeholder is secret as a whole. |
| Under another `base` | It does not follow: [a secret stays with its base](#a-secret-stays-with-its-base). |

**The credential is an input.** When every call needs it, it is an input of the client, and `new` takes it. When one endpoint needs a different one (the player's own token in `me`, in [a whole description](describing-an-api.md#a-whole-description)), it is an input of that endpoint, which writes its own entry and removes the inherited one: `headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }`.

| | |
|---|---|
| A key that changes | A `Secret` can be replaced: `key.replace("the new key")`. Every client and handle that holds a clone of it sends the new key in every request built from then on. A request is built once per call, so a retry after a wait sends the key that the call started with. |
| Basic | `basic(user, password)` returns a `Secret`: `Basic` followed by the pair in base64 (RFC 7617), encoded as UTF-8. |
| A value that cannot be a header | A line break in a token is a Build error, and nothing is sent. |
| `request()` | It shows the request with these values in it, because they are part of what the description builds. |

There is no `auth` statement and no list of authentication schemes: a scheme would do nothing more than place a value, and `headers` and `query` already do that.

## A secret stays with its base

An entry of `headers` or `query`, or a placeholder in a `path`, whose value holds a `Secret` is bound to the `base` that applies where it is written: the `base` of its own node, or the nearest one above. When no `base` applies there (a key on the client, above every `base`), it is bound to the first `base` below it, on each branch, so such a key follows to all of them. A secret in a placeholder of a `base` is part of that `base`: a `base` written further down replaces it whole, so the secret goes there only if that `base` writes it again (Telegram's second `base`, `"https://api.telegram.org/file/bot{client.token}"`). This follows from how a `base` is inherited: the nearest one applies ([C1.13](../reference/contracts.md#c1-the-description)).

Under any other `base` further down (an upload host, a second API), the description does not compile until that node writes the entry again or removes it:

- `override "authorization" = "Bearer {client.token}";` sends the key to the new host too, because you wrote it there;
- `remove "authorization";` sends nothing there.

GitHub's `upload_asset`, in [the block](describing-an-api.md#the-block), is written this way. A `path` has no `override` and no `remove`: a `path` that holds a secret is written under each `base` that needs it, not above them.

The error is a `rustc` message in Concord's words: `a secret does not follow a change of base`, with the note `` declare this entry again with `override`, or remove it, under the new `base` `` (for a `path`: `a secret in a path does not follow a change of base`). See [troubleshooting](troubleshooting.md#it-does-not-compile).

A caller's `with_base(…)` moves everything, secrets included: that is the caller's explicit choice. It also replaces the whole `base` ([C2.10](../reference/contracts.md#c2-building-a-request)), including any secret written in it. See [rule 16](../reference/rules.md) for the rule, and C8.9 of [the contracts](../reference/contracts.md) for the contract.

## A signature

A signature is computed from each request as it is sent, so it is a middleware layer, written last in the description ([the order of layers](middleware.md#the-order-of-layers)). Concord ships one signer, `concord::sign::SigV4` (feature `sign`), and two helpers for a provider's own proof.

For AWS Signature Version 4, supply the credentials, origin, region, service and path convention explicitly:

```rust
use concord::sign::{CanonicalPath, Credentials, SigV4};

let origin = "https://iam.amazonaws.com".parse()?;
let credentials = Credentials::new(access_key_id, secret_access_key);
let signer = SigV4::new(&origin, credentials, "us-east-1", "iam",
    CanonicalPath::NormalizeAndEncode);
```

Pass `signer` as a required input of the client and write `middleware signer.clone();` last; the rustdoc of `concord::sign` has the whole example, with its description. It checks the origin before using a key and signs every attempt anew; replacing the complete set with `Credentials::replace` reaches every clone ([what a clone shares](middleware.md#what-a-clone-shares)). A stream is refused without being polled unless `allow_unsigned_payload()` was explicitly enabled for S3 over HTTPS; other services still refuse streams. For S3, use `CanonicalPath::Preserve` with an AWS-encoded path. `SigV4::sign(&mut request, region)` signs one request in a region chosen per call, and checks the configured origin as the middleware does; the [S3 example](../examples/s3.md) serves its configured bucket origins and the fixed global endpoint with one signer, so its adapter checks each request against those origins itself, then signs with `SigV4::sign_any_origin` in the region of the endpoint. `SigV4` does no chunk signing, makes no presigned URL and discovers no credentials: it signs each request with the credentials that it was given.

Use Concord's own client, `Http::new()` or `Http::with(…)`, which follows no redirects. A supplied transport that follows redirects may forward `x-amz-security-token` to the next host: the HTTP library removes `Authorization`, but keeps the session-token header even though it is sensitive.

A provider's own HMAC proof belongs beside its description. This complete recipe uses `hmac`, `sha2` and `hex`: its protocol signs a timestamp, method, encoded path, exact query and SHA-256 body digest, separated by newlines. Match the provider's actual protocol when adapting it; there is no universal HMAC canonicalization.

```rust
use concord::http::HeaderValue;
use concord::reqwest::{Request, Response};
use concord::sign::{body_bytes, check_origin, SignError};
use concord::transport::{TransportError, TryInfo};
use concord::{Base, Middleware, Next, Secret};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Proof {
    origin: Base,
    key: Secret,
}

impl Proof {
    pub fn new(origin: Base, key: Secret) -> Self {
        Self { origin, key }
    }

    fn sign(&self, request: &mut Request) -> Result<(), SignError> {
        check_origin(request, &self.origin)?; // before exposing the key
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(SignError::failed)?
            .as_secs();
        let digest = hex::encode(Sha256::digest(body_bytes(request)?));
        let text = format!(
            "{}\n{}\n{}\n{}\n{}",
            seconds,
            request.method(),
            request.url().path(),
            request.url().query().unwrap_or(""),
            digest
        );
        let key = self.key.expose();
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).map_err(SignError::failed)?;
        mac.update(text.as_bytes());
        let mut proof = HeaderValue::from_str(&hex::encode(mac.finalize().into_bytes()))?;
        proof.set_sensitive(true);
        let timestamp = HeaderValue::from_str(&seconds.to_string())?;
        request
            .headers_mut()
            .try_insert("x-proof-time", timestamp)
            .map_err(SignError::failed)?;
        request
            .headers_mut()
            .try_insert("x-proof", proof)
            .map_err(SignError::failed)?;
        Ok(())
    }
}

impl Middleware for Proof {
    async fn handle(
        &self,
        mut request: Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<Response, TransportError> {
        self.sign(&mut request)
            .map_err(|cause| TransportError::blocked().with_source(cause))?;
        next.send(request, info).await
    }
}

concord::api! {
    pub client Vault(origin: Base, key: Secret) {
        base origin;
        middleware Proof::new(origin.clone(), key.clone());
        GET balance() -> String { path "balance"; read concord::Text; }
    }
}
```

Tested: [`credential_signing.rs`](../../tests/real-apis/src/credential_signing.rs), run by [`signing.rs`](../../tests/real-apis/tests/signing.rs), which compares this block with the file.

`Vault::new(origin, key)` always installs the proof; a transport supplied with `with_transport` receives the signed request. `sign::body_bytes` borrows a buffered body or refuses a stream, and `sign::check_origin` checks scheme, host and effective port before the key is read. The HMAC recipe hashes synchronously, so a large buffered body occupies the calling thread until hashing finishes.

OAuth 1 is not shipped: its parameter normalization, nonce, HMAC-SHA1 encoding and authorization flow require a substantial adapter. Signed HTTP messages are not shipped either: a fixed HMAC profile can be a small recipe, but general RFC 9421 component selection, structured fields, keys and trailers need a separate implementation.

## A token that is renewed

**A token that expires is renewed by `concord::auth::Renewed`** (feature `auth` of `concord-api`). It is a middleware layer: it keeps the current token, places it on every request, and calls a function of yours when a new one is needed. That function obtains the token from the API's token endpoint, through a client of its own:

```rust
use concord::auth::{after_401, AccessToken, Renewed};
use concord::{Base, Retry};
use std::time::Duration;

let tokens = Tokens::new(application_key);                  // a client of the token endpoint, without `Renewed`
let base: Base = "https://documents.example.com/v2".parse()?;
let renewed = Renewed::new(&base, move || {
    let tokens = tokens.clone();
    async move {
        let reply = tokens.issue().await?;                  // your own flow: here, client credentials
        AccessToken::expires_in(reply.access_token, Duration::from_secs(reply.expires_in))
    }
})
.early_by(Duration::from_secs(20));                         // renew 20 s before the token expires

let documents = Documents::new()
    .with_middleware(renewed)                               // `Authorization: Bearer <token>` on every request
    .with_retry(Retry::standard().when(after_401));         // after a 401: one more attempt, with a new token
```

`Tokens` is a client described with `api!`, like any other: the rustdoc of `concord::auth` has the whole example, with its description. The feature is off by default:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api", features = ["auth"] }
```

| | |
|---|---|
| Where the token goes | `Authorization: Bearer <token>`, in a header marked sensitive. For another header or scheme, `.header(name, \|token\| HeaderValue::from_str(&token.expose()))` sets the whole value; it is marked sensitive too. |
| Where it is sent | Only to the origin (the scheme, the host and the port) of the `base` given to `Renewed::new`. A path prefix in that `base` does not narrow or widen this. A request to another origin is blocked before any renewal, so a `Renewed` belongs to a client whose requests all go to that origin. |
| When it is renewed | When there is no token yet, and when the token has expired or has no more than `early_by` left (zero by default). The function takes no argument and returns a `Result<AccessToken, RenewalError>`: `AccessToken::expires_in(value, lifetime)`, and `?` on the token client's error. It must use a client without this `Renewed`. |
| One renewal at a time | The calls that need a token at the same moment share one renewal, and its token or its failure. Clones of a `Renewed` share one token and one renewal: configure it fully, then clone it ([what a clone shares](middleware.md#what-a-clone-shares)). Concord starts no task: a call that is dropped while it waits leaves the renewal to the others, and when the last one is dropped, the renewal is dropped with it. |
| How long a renewal may take | `renewal_timeout`, 30 seconds by default (zero is refused). After that, the calls that wait fail with `RenewalError::TimedOut`. A renewal is a wait in a middleware: the idle timeout does not count it, and the call timeout, `timeout`, does ([a layer that waits](middleware.md#a-layer-that-waits)). |
| A `401` | It is returned as it is. With no retry rule, the request gets one attempt, and `Renewed` passes it on at most once; with `concord::Http` and no middleware of your own that sends again, nothing below sends it again ([what can send a request twice](retries.md#what-can-send-a-request-twice)). With `after_401`, the engine can send it a second time, with the new token, and returns that response. With other retry rules of the call, it is sent as many times as they allow. The token that the request carried is marked as rejected, so the next request gets a new one. A response whose final URL, as the transport reports it, is at another origin (a transport that followed a redirect) rejects nothing, `401` included. |
| A rejection in a `200` | For an API that reports an expired token in another status, `rejected_when(…)` says once what that response looks like ([a token rejected inside a `200`](#a-token-rejected-inside-a-200)). |
| When a renewal fails | The call is a Transport error of class blocked, and nothing is sent. Its `source()` is a `concord::auth::RenewalError`, which `RenewalError::find(&e)` returns from the error of the call; for `RenewalError::Failed`, the `source()` of that is your function's error, such as the token client's `concord::Error`. A panic in your function is `RenewalError::Panicked` for every call that waits. The next call tries again. |
| A token that you already hold | `with_token(token)` starts with it, for example a token saved by an earlier run. It returns `Err(RenewalError::Expired)` for a token that has expired. |
| Saving a new token | `on_renewed(\|token\| …)` is called once for each new token, before any request uses it. It must return promptly. `token.value()` and `token.remaining()` are what to save. |
| Printing | A token is never printed: not in `Debug`, not in an error. What your own function puts in its own error is yours. |

**`Renewed` never sends a request twice.** To try again after a `401`, add the rule `after_401` to the client's retry policy. It asks for one more attempt after a `401` on the first attempt, and the engine then decides as it does for every retry ([three questions decide a retry](retries.md#three-questions-decide-a-retry)): a `POST` that is not marked `idempotent;`, and a `Body::stream`, are not sent again. The new attempt goes through every middleware layer, so it carries the new token and a signer signs it again. `Retry::none().tries(2).when(after_401)` adds this one attempt and no other retry. `.not_applied_on(401)` lets an unmarked `POST` go again too: write it only when the API guarantees that a request rejected with `401` did nothing.

This is what distinguishes it from a middleware of your own that sends the request again after a `401` (`next.send` a second time, on a copy). Such a second send is not an attempt: the retry policy does not decide it, `e.tries()` does not count it, and it repeats a `POST` that is not marked. With `Renewed`, `after_401` and `concord::Http`, every repetition is an attempt of the engine, under the same rules as any other retry; a middleware layer or a `reqwest::Client` of your own can still send below the engine ([what can send a request twice](retries.md#what-can-send-a-request-twice)).

Added with `with_middleware`, `Renewed` runs before every middleware layer of the description, so a signer written there signs each request with its token in place ([the order of layers](middleware.md#the-order-of-layers)).

**What it leaves to you.** Concord implements no flow that obtains a token (OAuth2 and the like): the function given to `Renewed::new` is where that flow goes, written with a client of its own for the token endpoint (a token endpoint takes its secrets as a body of entries: [a body that is sent](bodies.md#a-body-that-is-sent)). `Renewed` keeps only the current access token. If the server replaces the refresh token after each exchange, use `Rotating` ([below](#a-refresh-token-that-rotates)).

## A refresh token that rotates

**Use `concord::auth::Rotating` when an exchange consumes a refresh token and returns its replacement** (feature `rotating`, off by default). Use `Renewed` when a cancelled renewal can simply be started again. Reusing a consumed refresh token can lose access to an account; `Rotating` owns the order of exchanging and saving it, for one account shared by all its clones.

`Rotating::new(&base, store, exchange)` returns the middleware and a `RotationDriver`. The application must keep polling that driver independently of resource calls. In a long-lived program, the application starts and owns its task:

```rust
use concord::auth::{after_401, CredentialStore, Exchange, Rotating, RotationError, Stored, TokenPair};
use concord::Retry;
use std::time::Duration;

let recovery_store = store.clone();
let (auth, driver) = Rotating::new(&base, store, move |exchange: Exchange| {
    let tokens = tokens.clone();                           // a separate token client, with retries disabled
    async move {
        let reply = tokens.exchange(exchange.refresh()).await?;
        Ok(TokenPair::expires_in(reply.access_token, reply.refresh_token,
            Duration::from_secs(reply.expires_in)))
    }
});
let driver_task = tokio::spawn(async move {               // the application starts this task; Concord starts none
    let result = driver.await;
    if let Err(RotationError::Unsaved { pair, .. }) = &result {
        // First ensure previous writes can have no later effects after a panic.
        if recovery_store.save(Stored::Ready(pair.clone())).await.is_ok() {
            return Ok(());
        }
    }
    result                                               // keep the pair in the error if recovery failed
});
let documents = Documents::new()
    .with_middleware(auth.early_by(Duration::from_secs(20)))
    .with_retry(Retry::standard().when(after_401));

while jobs.recv().await.is_some() {
    let result = documents.title().await;
    // Handle the result. A dropped or timed-out call does not cancel renewal.
}
drop(documents);
driver_task.await??;                                       // handle any Unsaved error at the application boundary
```

The rustdoc of `concord::auth::Rotating` has the whole example: the token endpoint, a local store, and the program that polls the driver. The token client must not send an exchange twice: one call to the token endpoint per exchange, no retry policy, no middleware that sends again, and `concord::Http`, which resends nothing and follows no redirect, or a `reqwest::Client` of your own built with `.retry(reqwest::retry::never())` and `.redirect(reqwest::redirect::Policy::none())`. Bound that client's request with its own `with_timeout`: the driver never times out the exchange or store operations. It drops an initial `load` when the last call waiting for it is dropped; a save is never dropped while the driver remains polled.

| | |
|---|---|
| The store | Implement `CredentialStore::load` and `save`. Its exhaustive `Stored` enum is either `Ready(TokenPair)` or `Refreshing`. A successful save acknowledges atomic, durable replacement. After a save returns, **even with an error**, that operation must have no further effects on storage. Its outcome may be unknown, but must be final. A timed-out HTTP PUT is not generally a suitable store: it may commit after it returned an error. |
| The order | Save `Refreshing` before invoking the exchange. Save the replacement pair before either token is used. An unclear save is followed by a reload; a failed last save retains the new pair and retries only saving it, never the old exchange. A reload that finds the pair still saves it again: seeing a record does not acknowledge durability. An empty stored refresh token is refused before the marker; an empty or unchanged exchange result leaves the marker, so the next call requires reauthorization. A loaded marker without a known replacement also requires reauthorization. |
| An exchange that was not sent | A failed exchange leaves the marker, because the server may have consumed the refresh token. Only your exchange can know that its request never left; it says so by returning `RenewalError::not_sent(cause)` ([below](#an-exchange-that-was-not-sent)). Then the pair from before the exchange is saved again over the marker, without a usable expiry (`expires_at()` is `None`): its access token was being replaced, so the next call, in this program or in any owner that loads the store later, exchanges the same refresh token before it sends an access token. The calls that waited fail with that error without being sent. If that save fails, this instance stays blocked and its next call requires reauthorization; the store may hold the marker or the restored pair, and an owner that loads it later requires reauthorization, or exchanges. If a panic destroys the exchange's future, nothing is saved again, the call receives `RenewalError::Panicked`, and the marker stays. |
| Expiry | An exchange returns `TokenPair::expires_in(access, refresh, lifetime)` without reading the clock or handling overflow. A store restores `TokenPair::new(access, refresh, expires_at)`, with an `Option<SystemTime>`. `None` means access cannot be used; a valid replacement refresh token is still saved before the call fails. The relative lifetime uses the renewal timer, including virtual time in tests. Time spent saving counts against it. After saved access cannot be used, the next call renews with the saved refresh token. |
| The wait | `renewal_timeout`, 30 seconds by default, bounds each resource call's wait. If the first driver poll after the call began waiting is at or after its deadline, or never happens, the call fails with `RotationError::NotPolled`; another expired wait has `RenewalError::TimedOut` as its cause. A begun renewal goes on; its replacement is saved or returned to the driver’s owner as `Unsaved`. The same name on `Renewed` bounds its shared renewal future instead: cancelling that renewal does not consume a single-use refresh token. |
| Dropping | Renewal begins when saving its marker begins. Dropping the last waiting call during the first load leaves nothing begun; after the marker save begins, dropping calls leaves the driver working. Dropping the driver stops the credential and wakes waiters with `RotationError::Stopped` unless their deadline has already passed; it cannot finish saving after it has been dropped. Dropping the last credential owner lets a polled driver drain its begun work, try a pending save once more, then finish. If recovery fails, it returns the pair in `Unsaved`. A hung store or exchange can prevent that drain. |
| Ownership and failure | One value and its clones own the account; another process needs an application-owned lease. A store panic stops the credential with `Stopped`, but an existing call whose deadline has passed still reports its timeout; the driver returns `Unsaved` with a known replacement, or `Stopped` when it has none. Before a new owner can use the account after a panic or cancellation, the application must establish that old store operations can have no later effects. |
| Headers, origin and retry | These work as for `Renewed`: a sensitive Bearer header by default, `header` for another scheme, the origin checked before any store access, a `401` returned unchanged, which rejects only the access token that the request carried, and a response from another origin, which rejects nothing. `after_401` asks the engine for another attempt under its usual repeatability rules. The middleware never resends a resource request itself. Configure it fully, then clone it: as with `Renewed`, a change of `header`, `early_by`, `renewal_timeout` or `rejected_when` on a clone changes the settings that every clone shares ([what a clone shares](middleware.md#what-a-clone-shares)). `rejected_when` works as for `Renewed`, and the next call runs an exchange ([a token rejected inside a `200`](#a-token-rejected-inside-a-200)). |
| Errors | `RotationError::find(&error)` follows the call error's sources. `Load`, `Save`, and `Renewal` retain a `RenewalError` and its original cause: for `Renewal`, the `source()` of that `RenewalError` is the error that your exchange returned ([below](#an-exchange-that-was-not-sent)). `Unsaved { pair, cause, .. }` gives the application the exact replacement for its own recovery path; after a panic, first make sure that no earlier store operation can still take effect, then save it. Concord’s own error formatting and `Debug` print no token, URL, cause text or panic payload. What an author puts in a store error is the author’s responsibility; traversing its sources can print that text. |

Completing a renewal wakes all its waiting calls, so notification work grows with the number of waiters.

**A failed exchange reaches the call** as `RotationError::Renewal`, which holds the `RenewalError` of the exchange; the `source()` of that is your exchange's own error, such as the token client's `concord::Error`:

```rust
if let Some(RotationError::Renewal(renewal)) = RotationError::find(&error) {
    let exchange_error = std::error::Error::source(renewal);   // what your exchange returned
}
```

### An exchange that was not sent

On a call to the token endpoint described with `api!`, `.map_err(RenewalError::from_call)?` in place of `?` returns `RenewalError::NotSent` when `error.tries() == 1 && error.request_not_sent()`: the call failed on its first and only attempt, and Concord states that nothing of it was sent. A failure states it when the request was blocked before it was sent, for example by a `Limit` on the token client; when `concord::Http` recognizes the failure as one of its connector's (the connection was not made: refused, a host name not resolved, a failed TLS handshake or proxy, its connect timeout); or when a transport or middleware of your own states it with `TransportError::not_sent()`, which it may do after it opened a connection too, as long as no byte of the request was written and none can still be written. The call's error reports it only when every send of the call ended with such a failure, none is still outstanding, and no earlier attempt or page may have written anything ([C9.5](../reference/contracts.md#c9-the-transport-and-tests)). Every other error is `Failed`: with `Http`, a connection that broke after it was made; a `Connect` error from a `reqwest::Client` used as the transport; a timeout of the call or of the response; any response; and an error after a retry or on a later page. The class of an error alone never decides whether a request may be sent again.

**What "not sent" does not cover.**

- `false` means "maybe sent", and includes "unknown": Concord states the fact only where it can show it. Some failures leave it `false` although no byte left: a Build error, a `reqwest::Client` of your own as the transport, a timeout of the call or of the response, a failure that `concord::Http` does not recognize as one of its connector's (an error chain that it does not expect, HTTP/3), a copy of the attempt's `TryInfo` that a middleware still holds when the attempt ends, and an earlier attempt or page that may have sent something. To trace an attempt, keep `info.endpoint()` and `info.number()`, not the `TryInfo`.
- It is about one call. `from_call` needs one attempt, so give the token client no retry policy. A call made with `concord::transfer::at` states it about itself only. Another call that carried the same refresh token is not covered.
- A middleware layer or a transport of your own that states `not_sent()` or `blocked()`, or forwards such a failure, first accounts for every send that it made or still holds, through the chain, beside it, or with a transport of its own (the rustdoc of `concord::Middleware` says which sends Concord sees). A false statement makes the same refresh token be used twice.
- For `concord::Http`, the statement rests on how `reqwest`, `hyper-util`, `hyper` and `h2` report a connection that could not be made, read in the versions of Concord's own `Cargo.lock` (`reqwest` 0.13.5, `hyper-util` 0.1.21, `hyper` 1.12.0, `h2` 0.4.20). The `Cargo.lock` of your application decides the versions that it builds with.

**A wrong "not sent" costs the account.** It makes `Rotating` send the same refresh token again. If the server had in fact received it, that token is consumed, and a provider that sees a consumed refresh token used again usually revokes every token of the account: the user must sign in again, and every other session of the account ends with it. Say "not sent" only when you know that no byte of the request was written.

An exchange written with `reqwest` directly can say it for a failed connection, under these conditions only: the exchange sends one request; the client follows no redirect and does not retry (`reqwest`'s own retries of a request are off); the request body is buffered (a form, not a stream); the client uses `reqwest`'s own connector (the example needs `reqwest`'s feature `form`), without a connector or connection layer of your own; and nothing is sent as TLS early data. Then a connection that could not be made fails before the request is written:

```rust
let http = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .retry(reqwest::retry::never())
    .build()?;
// In the exchange:
let reply = http.post(token_url)
    .form(&[("grant_type", "refresh_token"), ("refresh_token", &*exchange.refresh().expose())])
    .send()
    .await
    .map_err(|e| if e.is_connect() && !e.is_timeout() {
        RenewalError::not_sent(e.without_url())                // no connection: nothing was written
    } else {
        RenewalError::failed(e.without_url())
    })?;
```

The promise to save or return a replacement begins when the exchange future yields its pair. Concord cannot protect a pair that the author's exchange code loses before yielding it. Concord catches a panic when it destroys an author's value; if destroying that panic's payload also panics, Concord cannot contain the second panic.

**What it cannot promise.** A process can die after the provider issued a replacement and before the local save. No client can make those two systems commit atomically. On restart the marker prevents reuse of the uncertain old token, but cannot recover the lost replacement; the account may need reauthorization. The application still supplies the initial grant, the token protocol, durable storage, exclusive account ownership and any provider recovery. Concord starts no task, and an unpolled or dropped driver cannot finish a renewal.

## A token rejected inside a `200`

**Some APIs never answer `401` for a token that has expired or was revoked.** Slack answers `200` with `{"ok": false, "error": "token_expired"}`; Telegram reports its errors in an `ok` field too. The arm on the `2xx` turns that body into the API's problem, but an arm cannot reach the credential, and the credential, a middleware layer, sees the response before its body is read. Without more, the next calls go out with the same dead token until its known expiry.

So say once, on the credential, what such a rejection looks like. The function receives a `concord::auth::Seen`: the status, the headers and the whole body of the response.

```rust
use concord::auth::{after_401, Seen};
use concord::Retry;

#[derive(serde::Deserialize)]
struct Envelope { ok: bool, error: Option<String> }

fn token_rejected(seen: &Seen<'_>) -> bool {
    seen.status() == 200
        && serde_json::from_slice::<Envelope>(seen.body()).is_ok_and(|reply| {
            !reply.ok
                && matches!(reply.error.as_deref(), Some("token_expired" | "token_revoked" | "invalid_auth"))
        })
}

let slack = Slack::new()
    .with_middleware(auth.rejected_when(token_rejected))   // a `Renewed` or a `Rotating`
    .with_retry(Retry::standard().when(after_401));
```

| | |
|---|---|
| When the function runs | Once for each response whose body is read to its end, when the response comes from the origin of the credential's base (its final URL, as the transport reports it), and its body holds at most 64 KiB and is complete. A body that ends with an error never is. Of one that ends cleanly: a response that never carries a body (a `1xx`, `204`, `205` or `304`, a response to a `HEAD`, a successful response to a `CONNECT`, by the method the credential sent) is complete when it gave no byte, whatever length it announces, and the function then sees its headers; it is not shown if it gave a byte. Any other response is complete when it gave exactly the length that the reader expects (the `Content-Length`, or else the exact size hint), or, with neither, all it had: no byte against a positive length announced is a cut body. It is not called for a `401`, which always rejects the token. |
| How the body is read | For a value or a problem that the reader decodes: as the body ends, before it is decoded, so before the call returns. For a stream of records (`JsonLines`, `Sse`): when the body signals its end, which can come before the reader has yielded its last records, or only on a later poll after the last bytes; dropping the stream before the body has signalled its end skips the function. For `send_raw()`: when you have read the body to its end; never if you do not. A call given to `transfer::at` is read like any call. A body that is not read to its end is not shown: a dropped call, a reader that stops early, an endpoint that does not read its body. An incomplete or over-long body is not shown either. |
| What `true` does | The token that the request carried is rejected, as after a `401`. Nothing is renewed at once: each later attempt that needs the token, of any call, joins the renewal in flight or starts one (with `Rotating`, an exchange), and the retry setting of each call bounds its attempts. An answer that arrives after the token was replaced rejects nothing: the newer token stays. The response is attributed to the token that the credential placed; a layer after it that removes or replaces the `Authorization` header owns that change. |
| The call that received it | It returns what its arm makes of the response, here the problem `token_expired`, after one attempt under this policy: `rejected_when` itself schedules no retry, and `after_401` retries a `401` only. A retry rule of the call that applies to the response, such as the standard rule for a `503` on a `GET`, can send the call again, and that attempt carries the renewed token. Nothing bounds the renewals across calls when a server rejects every token: each attempt that needs a token joins a renewal or starts one, and waits for it at most `renewal_timeout`. |
| The body | The function sees it once; the credential then drops it, and keeps and prints none of it. What your function does with it is yours. A panic in the function rejects nothing, but Rust's panic hook still prints its message: never put text of the body in a panic message. |
| What it costs | Without `rejected_when`, a response passes through untouched. With it, every response from the credential's origin is observed, also when the function never matches: up to 64 KiB of its body is copied into one buffer, dropped once the function has seen it, or as soon as the body is longer. Frames, trailers, the size hint and the end of the body pass through unchanged. The function must return promptly. |

### Known limits

- A transport that reports a wrong final URL, or a later layer that rewrites the `Authorization` header, can make a rejection count for the wrong credential, or hide it: the credential judges the origin by the URL that the transport reports, and attributes the response to the token that it placed.
- A later layer that changes the request's method can hide a response from the function: the credential judges a response without a body by the method that it sent.
- Without an announced length, a body that the connection ends early can look complete: an HTTP/1 body delimited by the close of the connection, or an HTTP/2 stream reset with no error.
- A body that is not read to its end, stalls, is larger than 64 KiB, ends with an error, or cannot be copied for want of memory is not shown. A rejection stated only in trailers is not seen.
- A `401` from the credential's own origin always rejects its token, and a function that matches too widely discards a good token. A function that panics rejects nothing.
- Calls already in flight still carry the old token, and a late rejection never discards a newer one.
- Nothing bounds the renewals across calls when a server rejects every token.
- A wrong "not sent" can cost every token of the account ([an exchange that was not sent](#an-exchange-that-was-not-sent)). An uncertain save, a stopped driver, or two owners of one store without a lease can each end in a new sign-in.

## What Concord does not ship

| Not shipped | Why | The escape hatch |
|---|---|---|
| Provider-specific signatures, OAuth 1 and signed HTTP messages | Their canonicalization and protocol differ; the shipped signer implements AWS Signature Version 4. | A middleware layer using `sign::body_bytes`, `sign::check_origin` and the protocol's cryptographic crates ([above](#a-signature)). |
| The flows that obtain a token (OAuth2 and similar), durable account stores and provider recovery | They depend on the provider and the application. `Rotating` coordinates exchange and saving, but cannot atomically commit remote issuance and local storage. | Supply the exchange to `Renewed::new` or `Rotating::new`, and a `CredentialStore` for [a refresh token that rotates](#a-refresh-token-that-rotates). |

See [scope and limits](../design/scope.md) for why these are left out, and what else is.
