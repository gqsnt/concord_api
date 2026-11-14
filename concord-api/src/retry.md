**A worked example.** An API says: "a `5xx` on a `GET` may be retried at most 4 more times,
after 1, 2, 4 and 8 seconds". The author writes that rule once, as a function next to the
description, and a test runs it on virtual time with `concord_testkit::Script`:

```rust
use concord::Json;
use concord_testkit::Script;
use std::time::Duration;
use tokio::time::Instant;

concord::api! {
    pub client Stations {
        base "https://api.test";
        read Json;
        GET list() -> Vec<String>;
        HEAD exists() -> bool { status { 2xx => true, 404 => false } idempotent false; }
    }
}

/// Every 5xx, 5 tries in all, after 1 s, 2 s, 4 s and 8 s.
pub fn retry() -> stations::Retry {
    stations::Retry::none().tries(5).when(|tried| {
        let server_error = tried.status()?.is_server_error();
        server_error.then(|| Duration::from_secs(1 << (tried.number() - 1)))
    })
}

# #[tokio::main(flavor = "current_thread", start_paused = true)]
# async fn main() -> stations::Result<()> {
// In a test: `#[tokio::test(start_paused = true)]`.
let script = Script::new();
for status in [500, 502, 503, 500] {
    script.answer(status);
}
script.answer(200).body("[]");
let api = Stations::new().with_transport(script.clone()).with_retry(retry());

let start = Instant::now();
api.list().await?;
let sent = script.requests();
let at: Vec<Duration> = sent.iter().map(|request| request.at() - start).collect();
let tries: Vec<u32> = sent.iter().map(|request| request.info().number()).collect();
assert_eq!(at, [0, 1, 3, 7, 15].map(Duration::from_secs)); // waits of 1, 2, 4, 8 s
assert_eq!(tries, [1, 2, 3, 4, 5]);

script.answer(503);
let error = api.exists().await.unwrap_err();   // `idempotent false;`: not tried again
assert_eq!((error.tries(), error.is_status(503)), (1, true));
# Ok(())
# }
```

What this shows:

- **`Retry::none()` and then `when`.** `none()` allows one attempt and has no standard table.
  `.tries(5)` allows five, and from then on only the rules decide: a `when` rule that returns
  `Some(wait)` makes another attempt, and when every rule returns `None` (a `404` here) there
  is none. If the policy were built from `Retry::standard().tries(5)` instead, a `None` would
  leave the decision to the standard table (`408`, `429`, `502`, `503`, `504`, a failed connection, a timeout, a body
  that broke off while it was read), with its random waits.
- **Is the wait exact?** A wait from `when` is a minimum. The attempt waits for the longest
  of that wait, the response's `Retry-After`, and a random delay of up to 1 s before attempt 2,
  2 s before attempt 3, then 4, 8, … at most 60 s, also after `none()`. A rule that asks for at
  least that much, as this one does, gets exactly its wait; a rule that asks for 1 s each time
  waits between 1 and 2 s before attempt 3. A wait over 60 s, or one that would end after the
  call's `timeout`, means no further attempt.
- **The method decides first.** A rule sees the failure, not the method: a `POST` without
  `idempotent;`, or any endpoint with `idempotent false;`, is never retried, whatever a rule
  says.
- **Where the policy lives.** A description holds no policy, and `stations::Retry` is only
  the type. The author ships the policy next to the description, as `retry()` here, and the client
  uses it only when a caller passes it: `Stations::new().with_retry(retry())`. A caller who
  wants more keeps it and adds to it: `with_retry(retry().when(mine))`.

