Implemented [the async adapter](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/b34-retry/crates/concord-retry/src/concord.rs:202), rustdoc, README and nine unit tests. PayPal adoption is retained. Adyen and Xero are unchanged. No commit was made.

The new API, behind feature `concord`, is:

```rust
pub async fn receive_with<T, A, E, H, Hook, C, N, S, Sleep, J>(
    self,
    prepared: Prepared<T>,
    mode: ReplayMode,
    hook: H,
    classify: C,
    now: N,
    sleep: S,
    jitter: J,
) -> ReceiveRun<T, A, E>
where
    H: FnMut(reqwest::Request, usize) -> Hook,
    Hook: Future<Output = Result<(reqwest::Request, A), E>>,
    C: FnMut(&Result<Received<T>, concord_core::Error>, &mut A) -> Decision,
    N: FnMut() -> Duration,
    S: FnMut(Duration) -> Sleep,
    Sleep: Future<Output = ()>,
    J: FnMut(usize, Duration) -> Duration;
```

- The owned request lets the hook await acquisition without boxed futures. Returning `Err(E)` prevents dispatch.
- `A` carries token generation or a lease into classification. Final context remains available through decoding; intermediate context is dropped before backoff.
- `ReceiveRun` contains `outcome`, `context`, `refusal` and `report`. Refusal preserves the SDK error alongside the previous completed outcome.
- `ReceiveReport` contains `retry: Report`, `completed`, `refused` and `no_response`. Here, `retry.attempts` counts dispatches; subtracting `completed` detects cancellation. `no_response` preserves transport history across later responses.

The deadline includes hooks. Diagnostics withhold caller values. Existing `receive`, `run`, their accounting types and every existing assertion remain unchanged.

Final physical line measurements, **removed / added**:

| SDK | Production (`src/`) | Tests | Markdown docs | Net |
|---|---:|---:|---:|---:|
| PayPal | 61 / 37 | 0 / 0 | 0 / 0 | −24 |
| Adyen | 0 / 0 | 0 / 0 | 0 / 0 | 0 |
| Xero | 0 / 0 | 0 / 0 | 0 / 0 | 0 |

PayPal additionally replaces one manifest line to enable `concord`.

Dropped adoptions:

- **Adyen:** the existing [assertion at line 943](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/b34-retry/examples/adyen-checkout-sdk/tests/first_use.rs:943) requires two factory starts when the second attempt is refused. Dispatch accounting reports one. Stopped and reverted immediately. Discarded candidate: source **34 / 28**, docs **2 / 2**, manifest **1 / 1**.
- **Xero:** its dependency does not enable `concord`; adoption requires a manifest edit outside the permitted `src/policy.rs`. Discarded prototype: source **52 / 47**, tests/docs **0 / 0**.

All required test, strict Clippy and strict rustdoc commands passed for all four packages after the reversions. Retry additionally passed with `--all-features`.

Test totals, including doctests: retry **24 default / 43 all-features**, PayPal **48**, Adyen **45**, Xero **48**. Workspace formatting and `git diff --check` passed. Builds used four jobs, the default target directory, offline mode and one Cargo command at a time.