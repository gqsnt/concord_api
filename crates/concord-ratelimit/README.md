# concord-ratelimit

Process-local admission for SDK-supplied scope keys. A local fixed window gives a predictable
maximum request count per interval without background refill; it is not a claim about a distributed
provider quota and there is no cross-process coordination. Map routes to provider bucket keys in the
SDK; two routes may supply the same key. Header names, fractional-second parsing, bucket/major
resource rules and global-429 interpretation remain SDK responsibilities.

Admission atomically charges all distinct keys when `acquire` completes. Call it immediately before
dispatch; cancellation while queued has no charge, and a completed admission is not refunded. A
lease limits concurrent work and is independent of rate charging; hold it through body or stream
consumption when appropriate. Release it before retry backoff. Clones share a strict FIFO queue
across all keys and leases: this avoids starvation but an unrelated key can wait behind a blocked
head. Refusal never bypasses a queued waiter. Cooldowns never shorten; overlapping window
observations conservatively retain the smaller remaining count and later reset. No background task
or mutex spans I/O. Capacity includes configured and observed keys; no active state is evicted.
Remove expired, inactive keys explicitly. Empty key sets still respect the global gate.

With `tokio`, use `TokioClock::default()`; otherwise implement `Clock` with monotonic time and a
cancellable runtime sleep. `after` checks overflow. A sleep implementation must not return before
the duration has elapsed. SDKs must use the same limiter clock epoch for absolute observations.

For composition with `concord-retry`, use `Retry::run` and put both `acquire` and `lease` in the
attempt factory, after retry's wait. Drop the lease before returning the attempt outcome to Retry,
so no permit spans backoff. For `concord-auth`, wrap the cache's token acquisition closure with an
admission under a separate SDK-defined token-service key; application traffic uses its own keys.
A token cache hit needs no token-service admission. Replay permission and response classification
remain the SDK's decisions.

A concrete composition (the SDK supplies `fetch_token` and `send_once`, and translates their
errors instead of using `unwrap`):

```rust,ignore
let run = Retry::new(retry_policy, Replay::Permitted).run(
    |attempt| {
        let limiter = limiter.clone();
        let cache = cache.clone();
        async move {
            let token = cache.get_or_refresh(token_scope, || {
                let limiter = limiter.clone();
                async move {
                    limiter.acquire(&["token-service"], Mode::Wait).await?;
                    let _lease = limiter.lease("token-service", Mode::Wait).await?;
                    fetch_token().await // returns Token, with a provider-specific error
                }
            }).await?;
            limiter.acquire(&["application"], Mode::Wait).await?;
            let _lease = limiter.lease("application", Mode::Wait).await?;
            send_once(attempt, token.header()).await // exactly one request
        }
    },
    classify, monotonic_now, sleep, jitter,
).await;
```

The attempt returns only after its lease drops. If the SDK returns a stream instead, move the
lease into a wrapper that owns the stream and drop it on completion or cancellation. The executable
composition test checks token cache hits, separate keys, reacquisition on each attempt and no lease
during backoff. The stream test holds a lease through `drain`.

## Discord migration estimate

The wave-1 file's reported 165 generic-and-provider code lines in `151–354` contain approximately 60 generic code lines for window, cooldown, global gate and clock/admission mechanics.
This count selects nonblank, noncomment lines 167–170, 188, 213–214, 219–233, 235–247,
250–253, 260–261, 267–269, 275–282, 315–318 and 321–327. Some mix provider error
construction with mechanics, so it is an estimate rather than a measured migration diff. Its route-to-bucket alias table stays in the SDK because
this crate intentionally receives already mapped scope keys. The 47-line send policy and 49-line
header/decimal helpers remain provider code, though their admission calls become shorter. Discord
still chooses route and major-resource identity, learns bucket aliases, parses fractional seconds,
projects headers/body and precedence, classifies global 429s and owns error/write semantics.
