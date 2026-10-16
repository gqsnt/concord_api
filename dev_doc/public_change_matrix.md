# V3 public contract inventory

**Status:** current `3.0.0-beta.1` surface, updated 16 October 2026.
The workspace supports V3 only. [Version policy](../docs/version_policy.md)
and [ADR 0027](adrs/0027-v3-only-product.md) define that boundary.
The facade source and maintained public-API/generated-surface tests are the
executable inventory; this page summarizes the entry points and caller laws.

## Facade and modules

| Entry point | V3 responsibility |
|---|---|
| `concord::api!` | One canonical, closed client declaration with nested owners and endpoints |
| Facade root | Common requests, values, controls, cancellation, errors, metadata, streams, secrets and representations |
| `concord::inspection` | Value-free `ContractDescription`, owners, endpoint facts and provenance |
| `concord::endpoint` | Generated request contracts, storage requests, bound runtime requests and runtime operation facts |
| `concord::body` | `PreparedBody`, `BodyDescription`, representations, bounded streams and optional multipart |
| `concord::response` | Static representation negotiation, sealed acquisition drivers, absence and stream outcomes |
| `concord::codec` | Explicit encode/decode roles and buffered representations; `NotFound` for buffered absence |
| `concord::pagination` | `Pages`, `CollectPrefix`, `CollectAll`, typed controller Init/Cursor, prefix and traversal outcomes |
| `concord::rate_limit` | Explicit policy IDs, typed canonical keys, current schemas, atomic admission and limiter capability |
| `concord::auth` | Material/provider roles, bounded provider HTTP, exact origins, installed credential versions and storage outcomes |
| `concord::error` | Stable failure categories, dispatch facts, operation counts and safe diagnostics |
| `concord::retry`, `runtime`, `hooks` | Managed retry policy, finite runtime configuration and read-only observation |
| `concord::records` | Feature-gated bounded record streams and CSV/NDJSON adapters |

## Generated caller laws

| Surface | Contract |
|---|---|
| Required constructor inputs | Exact authored payload type in declaration order; no synthesized setter |
| `set_x` | Select a payload through `IntoValue<T>` |
| `replace_x(Option<T>)` | Replace the complete optional slot; `None` means absence |
| `set_x_if_some(Option<T>)` | `None` keeps the existing selection |
| `omit_x` / `reset_x` | Explicit omission / restoration of the retained initialized value, where the value shape supports it |
| `IntoValue<T>` | Closed conversions: identity, `&str` to `String`, and `&String` to `String` |
| Clone | Body-free requests and intents are cloneable; body-bearing ones require a fresh request/factory |
| `RequestControls` | Common call options, cancellation and control summaries on named requests and terminal wrappers |
| Timeout controls | Caller selection remains under the operation's existing finite caps |
| `.await` | One complete declared response/page or an acquired managed stream, according to the representation |
| `.pages(n)` | Lazy bounded page traversal |
| `.collect_prefix(n)` | Bounded aggregate with an explicit prefix/traversal outcome |
| `.collect_all(NonZeroU32)` | Complete aggregate or a failure when completeness cannot be established within the bound |
| `.response()` / `.accepted_bytes()` | Response metadata / explicit byte acquisition where the endpoint grants that capability |
| `.store_as_x()` | Explicit installation request returning `Installed` or `Superseded` under the latest-started intent protocol |
| `Client::contract()` / `.description()` | Static symbolic facts without executing payload defaults, body code, providers or transport |

## Runtime endpoints

| Surface | Contract |
|---|---|
| `owner.bind(endpoint)` | Fallible V3 binding through the selected client/scope projection |
| `owner.plan_runtime(&endpoint)` | Public planning through that same projection |
| Query/header programs | The same ordered replacement, conditional, append, removal and sensitivity laws as generated declarations |
| Paths and DNS | Bounded atom encoding under the selected logical origin |
| `body_repr::<B, T>(input)` / `response::<R, O>()` | Explicit directional representation selection |
| Authentication | Safe removal or disabling of inherited placements; private authority comes from the declared owner |
| Runtime configuration | Primitive numeric setters validated at build; finite operation, stream and traversal ceilings |

## Extension roles

| Role | Contract |
|---|---|
| `RequestRepresentation` | One preparation returns the core-owned `PreparedBody` with actual media, length and replay facts |
| `ResponseRepresentation` | Static `ACCEPT` plus a declared response driver |
| `EncodeBody` / `DecodeBody` | Native V3 codec roles used by explicit representation adapters |
| `RequestAtom` / `AtomEncoder<T>` | Explicit bounded scalar protocol encoding; `DisplayAtom` is an opt-in formatting adapter |
| `PaginationController<P>` | Private controller state with public typed Init/Cursor, complete page input and bounded continuation decisions |
| `RateKeyPart` / `RateKeyEncoder<T>` | Named encoding identity and canonical framed bytes |
| `RateLimiter` | Atomic all-or-none admission and truthful capability; V3 shared state is checked against its registered schema |
| `CredentialProvider` | Exact declared HTTP origins; acquisition deadline and cancellation cover callbacks and local installation |
| `OperationObserver` | Bounded, borrowed, sanitized facts; no operation mutation or veto |

## Maintained inventory

- `crates/concord/tests/public_api/` checks facade namespaces and public imports.
- `crates/concord/tests/generated_identifier_hygiene.rs` checks caller identifiers
  against the generator's reserved internal namespace.
- `crates/concord-macros/src/generated_surface.rs` checks the pure V3 generated
  member snapshot in `crates/concord-macros/tests/fixtures/generated_surface/`.
- `tests/downstream/` checks authored API and consumer code in separate crates.
- Facade/core UI suites verify language boundaries and public trait obligations.
- [Generated-client documentation](../docs/generated_client.md) and
  [request/value controls](../docs/requests_and_values.md) explain ordinary use.

Earlier transition inventories remain available in Git history. They do not
add a supported version or a conversion path to the current product.
