# concord-auth

Sensitive credentials and a bounded, scoped token cache over plain async acquisition closures.
No runtime, HTTP client, Concord facade or macro dependency; there are no optional features.
Provider documents, realm trust, scope selection, signing, persistence, replay permission,
retries and admission remain outside this crate.

```rust
use concord_auth::{Token, TokenCache, basic, bearer};
use std::{convert::Infallible, num::NonZeroUsize, time::Duration};

assert_eq!(basic("user", "")?.to_str()?, "Basic dXNlcjo=");
assert!(bearer("synthetic-token")?.is_sensitive());
# Ok::<(), Box<dyn std::error::Error>>(())
```

Basic encodes UTF-8 `user:password` using standard padded Base64. Empty user/password are allowed;
the separator is always present. A colon in the username is refused, while password colons are
allowed. Unicode control characters in either field are refused. SDKs retain additional rules
(Twilio refuses empty fields). `base64` 0.23.1 was already in Cargo.lock and used by Twilio and
ClickHouse; the mature encoder replaces the registry's handwritten implementation.

Bearer and other scheme/token constructors reject empty tokens, spaces, controls (including HTAB
and DEL), and non-ASCII bytes. Other schemes must be HTTP tokens. These are single-token
credentials, not an authentication-document parser; further provider grammar belongs in the SDK.
All returned headers are sensitive. Debug never renders credentials, keys, clocks, or acquisition
errors. Error Display is static, and `RefreshError::source()` deliberately does not expose an
untrusted error formatter. `Acquire(Arc<E>)` permits explicit SDK inspection. Header bytes remain
available through the native header API; redaction is not encryption, zeroization, or persistence.

## Scoped cache

```rust
use concord_auth::{Token, TokenCache, bearer};
use std::{convert::Infallible, num::NonZeroUsize, time::Duration};
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
// All scope dimensions are supplied by the SDK; the cache never interprets them.
let key = ("audience", "tenant", "principal", "repository:acme/app:pull");
let cache = TokenCache::<_, Infallible>::new(
    NonZeroUsize::new(128).unwrap(), Duration::from_secs(5),
);
let used = cache.get_or_refresh(key, || async {
    // SDK acquisition: fetch and validate the provider document here.
    Ok(Token::new(bearer("synthetic-token").unwrap(), Duration::from_secs(60)))
}).await?;
// Apply used.header() immediately before the request. If that request gets a 401:
cache.invalidate(&key, &used.generation());
# Ok(()) }
```

The cache's immutable instance context may fix scope dimensions instead of repeating them in
`K`. For example, the registry's instance fixes destination and principal/auth policy, while
`Access` distinguishes repository pull, push, catalog and none. Changing its auth creates a new
cache. Callers must not reuse a cache/key across incompatible audiences, tenants or principals.

Each key holds a ready snapshot or a `futures_util::future::Shared` owned acquisition. A short
blocking mutex protects only the map; it never spans acquisition invocation, polling or I/O.
There is no spawned task and no Tokio dependency. If the starter or a waiter is cancelled,
remaining callers continue the same flight. If all callers cancel, the stored future pauses and
the next caller resumes it. A provider timeout belongs inside that future. Explicit `remove`
can discard an abandoned flight; any existing waiters still complete, but cannot resurrect the
removed generation. Do not capture the cache in its own acquisition closure (an Arc cycle), and
do not panic in the closure/future (`Shared` is poisoned by panic; remove that entry to recover).
Cancellation itself does not poison it.

Successful acquisition results and failures are shared by all attached waiters, including errors
that do not implement Clone. Failure removes the map entry so a later call tries again. Generation
identity is an opaque allocation, never a wrapping counter; an old 401 cannot remove a newer ready
token or an in-flight refresh. Explicit removal is a stronger application action and may allow a
new acquisition while old waiters finish.

Capacity is mandatory and nonzero. At capacity, a new key evicts an arbitrary completed entry;
active acquisitions are never evicted implicitly. If every slot is in flight, `Capacity` refuses
the new acquisition. This bounds retained keys/futures without inventing LRU policy. `remove`
provides explicit lifecycle management. Snapshots and existing waiters can outlive eviction.

The injected clock returns monotonic `Instant`s. Lifetime starts immediately before acquisition,
so time spent obtaining a token consumes its validity. The **full** untrusted lifetime is checked
with `Instant::checked_add` before subtracting the refresh margin: even a large margin cannot
hide overflow. `LifetimeOverflow` refuses the token without caching or returning it. A ready token
is reused only while `now < start + lifetime.saturating_sub(margin)`. Zero lifetime, elapsed
acquisition lifetime, or margin >= lifetime returns the acquired snapshot to existing waiters but
never reuses it for a subsequent call. This preserves the registry's one-use behavior; an SDK
requiring a positive remaining validity can refuse such provider documents during acquisition.
There is no refresh/retry loop to spin on unusably short lifetimes.

## Ordinary SendWith policy

A policy belongs in the SDK because it decides scope, destination trust, error interpretation and
semantic replay permission. This compiled recipe returns the unread final response; the SDK can
then decode its success or bounded provider error document. Its generated client can select it
with `send auth;` as in AUTHOR_GUIDE §5.13. `prepare()` itself remains undecorated.

```rust
use concord::{Endpoint, Received, SendWith};
use concord_auth::{Token, TokenCache};
use std::{future::Future, sync::Arc};
use reqwest::{StatusCode, header::AUTHORIZATION};
type Error = Box<dyn std::error::Error + Send + Sync>;

struct AuthPolicy<F, E> {
    cache: Arc<TokenCache<String, E>>,
    key: String, // SDK-scoped destination/audience + tenant + principal + access
    acquire: F,  // owns provider inputs, never captures this cache
    replay_permitted: bool, // SDK operation contract, not inferred from Clone or HTTP status
}
impl<T, F, Fut, E> SendWith<T> for AuthPolicy<F, E>
where
    T: Send,
    F: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Token, E>> + Send + 'static,
    E: Send + Sync + 'static,
{
    type Output = Result<Received<T>, Error>;
    async fn send<Op: Endpoint<T> + Send>(&self, op: Op) -> Self::Output {
        // This SDK binds the endpoint and policy to the same validated destination.
        let (client, mut request, decoder) = op.prepare()?.into_parts();
        let template = self.replay_permitted.then(|| request.try_clone()).flatten();
        let used = self.cache.get_or_refresh(self.key.clone(), self.acquire.clone()).await?;
        request.headers_mut().insert(AUTHORIZATION, used.header().clone());
        let received = decoder.execute(&client, request).await?;
        if received.status() != StatusCode::UNAUTHORIZED { return Ok(received); }
        self.cache.invalidate(&self.key, &used.generation());
        let Some(mut request) = template else { return Ok(received); };
        drop(received);
        let used = self.cache.get_or_refresh(self.key.clone(), self.acquire.clone()).await?;
        request.headers_mut().insert(AUTHORIZATION, used.header().clone());
        let received = decoder.execute(&client, request).await?;
        if received.status() == StatusCode::UNAUTHORIZED {
            self.cache.invalidate(&self.key, &used.generation());
        }
        Ok(received) // second 401 is final; no third acquisition or application attempt
    }
}
```

Refresh permission does not grant replay permission. A one-shot body sends once, without polling
or buffering it to manufacture a clone. For a permitted replay, the clone retains body semantics
and any idempotency key. The HTTP client must disable native retries and select an explicit
redirect policy for exact wire accounting and credential destination control.

## Composing with concord-retry

Use **one** owner of repetition. When `concord-retry` owns the budget, prepare the endpoint and
execute exactly one request per attempt; do not call the above policy's `send()` inside Retry.

`Retry::receive`'s synchronous attempt hook runs after the wait and can read `cache.get(key)` to
apply the current ready credential. It cannot await a missing/expired token or return an acquisition
error. For asynchronous acquisition, use `Retry::run`'s async attempt factory: clone/take the
request template, await `get_or_refresh`, apply its header, and call `decoder.execute` once. Keep
the used generation for the classifier; on 401, invalidate exactly that generation and return
`Decision::RetryAfter(None)`. `Replay`, `try_clone`, count and deadline still govern whether a
new application attempt may start. Classify acquisition failure explicitly rather than silently
sending without credentials. Choose `Replay::Never` for a noncloneable body even when refresh
itself is allowed. Required replay preflight can instead refuse before any acquisition or I/O.

Executable examples in `tests/policy.rs` exercise both the synchronous receive hook (a token
rotates during backoff) and the async run factory (401, invalidate, wait, acquire, send). They
verify operation identity, unchanged body/idempotency key, replay refusal for one-shot bodies,
and bounded 401 handling. Token-service request counts are asserted separately from application
requests. Retry reports count started attempt factories, so a factory that fails acquisition may
have zero application wire sends; SDKs needing wire counts should record `decoder.execute` starts
separately. Body consumption after the final head is outside both retry and auth.

## Public API and why each item exists

| Item | Justification |
| --- | --- |
| `basic(user, password)` | Correct shared Basic encoding and sensitivity with explicit colon/empty rules. |
| `bearer(token)` | The common sensitive token header without repeated formatting and validation. |
| `authorization(scheme, token)` | Other SDK-selected single-token schemes without a scheme enum or provider framework. |
| `CredentialError` | Value-free typed syntax refusals that SDKs can map to their own diagnostics. |
| `Token::new(header, lifetime)` | An owned acquisition result separating untrusted lifetime from cached validity. |
| `Snapshot::{header,generation}` | Couples the applied credential to the exact generation a response may invalidate. |
| `Generation` | Nonreusable identity makes delayed 401 invalidation safe across replacement and eviction. |
| `TokenCache<K,E>::{new,with_clock,get,get_or_refresh,invalidate,remove}` | One bounded coordinator with explicit scope keys, typed errors, lifecycle and injectable time. |
| `RefreshError<E>` | Distinguishes shared provider failure, checked-lifetime refusal and occupied capacity without leaking errors. |

The generic error parameter avoids erased provider documents and avoids requiring errors to Clone.
The clock is a closure rather than another public trait; acquisition is a plain owned async closure
rather than a provider interface. The crate has no retry, provider configuration or signing types.

Adoption: registry uses the shared cache (1024 keys, zero margin), sensitive Bearer and Basic,
while keeping realm trust/challenges/scopes/documents. It clones the request template for its one
challenge replay and invalidates only the generation actually sent, including a final 401.
Its existing checked lifetime refusal also remains in acquisition to keep token-response metadata.
Its shared `AuthError` preserves the private source with Arc and keeps public accessors unchanged.
A token-request preparation failure now maps to `Auth(TokenTransport)`; validated token inputs
normally cannot trigger it. Twilio and ClickHouse use shared Basic with their original public
validation/error behavior. No other SDK is migrated.

License: MIT OR Apache-2.0.
