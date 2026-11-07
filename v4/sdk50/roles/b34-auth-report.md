Implemented in the working tree on `sdk50/b34-auth`; no commit. QuickBooks adoption is retained. Xero and HubSpot probes were dropped because they grew the SDKs.

The [API](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/b34-auth/crates/concord-auth/src/cache.rs:218) adds optional provider state while preserving existing constructors, `Snapshot`, and `RefreshError`:

```rust
pub struct TokenCache<K, E, S = ()>;

pub fn with_state<S>(
    capacity: NonZeroUsize,
    refresh_margin: Duration,
) -> TokenCache<K, E, S>;

pub fn with_state_clock(
    capacity: NonZeroUsize,
    refresh_margin: Duration,
    now: impl Fn() -> Instant + Send + Sync + 'static,
) -> Self;

pub fn insert(
    &self, key: K, token: Token,
) -> Result<Snapshot, RefreshError<E>>;

pub fn replace(
    &self, key: K, state: S, token: Option<Token>,
) -> Result<Option<Snapshot>, RefreshError<E>>;

pub fn state(&self, key: &K) -> Option<Arc<S>>;

pub async fn get_or_refresh_with_state<F, Fut>(
    &self, key: K, acquire: F,
) -> Result<Snapshot, RefreshError<E>>
where
    F: FnOnce(Option<Arc<S>>) -> Fut + Send + 'static,
    Fut: Future<Output = (S, Result<Token, E>)> + Send + 'static;

pub enum RotationError<E> {
    NotIssued(E),
    NotStored(E),
}
```

Cache operations require `K: Eq + Hash + Clone` and `E, S: Send + Sync + 'static`.

- `insert` accepts externally obtained access tokens.
- `replace` installs a pair or refresh-only state and prevents older flights from overwriting either part.
- `state` retains provider state through expiry, invalidation, and acquisition failure.
- `get_or_refresh_with_state` publishes returned state and access together, including remembered rejection or quarantine state on failure.
- The stateful constructors preserve existing type inference and injected-clock support.
- `RotationError` lets the application classify the issuance/storage boundary explicitly. It does not infer remote consumption from a network failure.

The [README recipe](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/b34-auth/crates/concord-auth/README.md:119) covers durable pair publication, pending-record recovery, process ownership, and the unavoidable remote/local crash boundary.

Physical line measurements below are **removed/added**; production means `src/`, including comments.

| SDK | Kept production | Tests | Docs files | Dropped probe |
|---|---:|---:|---:|---:|
| QuickBooks | 87/77 (**−10**) | 0/0 | 0/0 | — |
| Xero | 0/0 | 0/0 | 0/0 | 5/9 (**+4**) |
| HubSpot | 0/0 | 0/0 | 0/0 | 53/64 (**+11**) |

QuickBooks removes its seed, epoch counter, and refresh-state mutex. Xero’s insertion probe retained the application-owned journal and added code. HubSpot’s state-publication probe retained its uncertainty guard and added code.

Xero’s existing assertions explicitly require erasing the old refresh token after 503 responses and refused connections. I stopped that behavior change and preserved fail-closed handling. No existing assertion was edited or removed.

All requested checks passed for `concord-auth` and all three SDK packages:

- `cargo test --locked -p <pkg>`
- `cargo clippy --locked -p <pkg> --all-targets -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p <pkg> --no-deps`
- `cargo fmt --all -- --check`

Nine new unit tests cover rotation races, cancellation, durability phases, capacity, overflow, redaction, and destructor locking. Commands ran sequentially with four build jobs, offline, using the worktree’s `target/`.

Constraint deviation: the discarded probes were measured using generated source copies in `/tmp`, outside your worktree-only read boundary.