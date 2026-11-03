# Track B: Rust API and values (calling API, value semantics, ownership)

## Source identity and model/effort

- SNAP = `/home/canarit/projects/concord_v4_refine_work/snapshots/calling-8f7adc2`. Its HEAD is `f159c26`. `.concord-review.json` records `original_commit` 8f7adc2019f3ee31af087c69286d559bca1c173a (tree 26e6a14b…). The working tree was clean before my edits.
- Control used by every experiment: a pristine detached worktree of SNAP HEAD at `EXP/snap-control`, made with `git worktree add --detach`. EXP's crates depend on this worktree. SNAP itself now carries only the D08 prototype, described below.
- EXP = `/home/canarit/projects/concord_v4_refine_work/exp/calling`.
- Toolchains: rustc 1.97.1 (8bab26f4f), rustc 1.85.1 (4eb161250) and nightly 1.101.0 (e9bbadf68, used for `-Zunpretty=expanded` and one future-compatibility check).
- Model: Claude Opus 5.5 (`claude-opus-5-5`). The requested effort was xhigh. **Effort is not observable from inside the session.**
- The host agent process restarted once, at about 00:43. I checked that SNAP and EXP were intact. I then re-ran every build, test and probe whose result is cited here. All outputs under `EXP/out/probes` were regenerated after the restart.
- The coordinator's fresh-use frictions are folded in: (1) goes into D01/D10, (2) into D02, (3) into D10.

## Summary

| ID | Recommendation | Smallest implementation |
|---|---|---|
| **D01** root construction | **CHANGE (root only).** A client with at least one required value is built as `Api::new(http, ApiConfig { … })`, where `ApiConfig` is a generated struct with `pub` fields. Scope and endpoint entries stay positional. Every required value also gets `set_x` (fresh-use friction 1). | Emit one `pub struct {Client}Config` with fields in declaration order, carrying the values' `///` docs and a redacting `Debug`. `new` destructures the struct. Add the type name to the collision check. Emit `set_x` for required values. |
| **D02** optional vocabulary | **KEEP** `set_x` / `maybe_x`, and fix the generated docs and one diagnostic note. | Change the doc text of `set_`/`maybe_` (state the initial value; say "`None` makes it absent", not "leaves"). Extend the `IntoValue` note to mention `maybe_<name>(None)`. |
| **D03** default lifecycle | **KEEP** eager evaluation once per owner construction, with no reset. **REJECT** a re-evaluating reset and lazy evaluation. | Docs only: mention the initializer in the generated docs (shared with D02). |
| **D08** bounds and ownership | **CHANGE.** An endpoint request is `Clone` exactly when its value types and its body value are `Clone`. A body no longer decides this. The prototype is implemented and tested on 1.97.1 and 1.85.1. Client and scope values stay `Clone`. | A manual `impl Clone for Endpoint where for<'__concord> T: Clone, …` replaces `#[derive(Clone)]` (+45 lines in `emit.rs`; diff in `EXP/out/d08_prototype.diff`). |
| **D09** snapshots/storage | **KEEP** the copy-on-write `Arc` handles. **CHANGE the wording** that over-claims immutability (a defect). | Docs: "structural snapshot; values are formatted at `prepare`". |
| **D10** non-wire state | **KEEP** liveness, with the wrapper as the route. **DEFER** read accessors and a `local` value. **KEEP** Debug as quoted wire text (friction 3), and document it. Rotation is solved by D01's `set_x` for required values. | Docs: a short wrapper example. No new syntax. |
| V05–V07 | **PASS** on the wire (loopback). The three-state `x?: Option<U>` is correct but easy to confuse (friction 2), so the D02 docs change applies. | — |

Changes to the control that I recommend, in priority order:

1. **D08** conditional `Clone`. It fixes the V10 contract failure (a body-dependent restriction).
2. **DEF-1**: docs of required values are dropped. D01's config struct fixes this for the root; scope and endpoint navigation methods need an argument list.
3. **D01**: the root config struct, plus `set_x` for required values.
4. Documentation fixes from D02, D03 and D09.

## DEFECTS found in the control

These are separate from design opinions. Each has a reproduction.

**DEF-1: doc comments on required values, and on a required body, are silently dropped from the generated API.**
- `positional_params` (emit.rs:91–115) ignores `value.docs`. Only `controls` emits docs, and it does so only for non-positional values.
- Reproduction 1: `examples/shapes` S2 declares `/// The API key; sent on every request, never shown by Concord.` on `api_key`. That text does not occur anywhere in the expansion (`EXP/out/shapes_expanded.rs`; `grep "The API key"` finds nothing, while the initialized `api_version` doc is present).
- Reproduction 2: rendered rustdoc of `EXP/values` `R8` and `Search`. The value docs ("Tenant slug", "The API key", "Admin API key") have 0 occurrences in `struct.R8.html` and `struct.Search.html`. `ShopTenant`'s entry method reads only "Enters scope `Shop.tenant`".
- Consequence: an author's documentation of a required input never reaches the caller, and `deny(missing_docs)` cannot notice.
- Fix: for the root, the D01 config struct carries the docs as field docs. For scope and endpoint entry methods, emit a `# Arguments`-style bullet list (`- \`tenant\`: Tenant slug.`) in the method doc. Required bodies are handled the same way.

**DEF-2: contract case V10 fails in the control. A body declaration changes which unrelated inputs are allowed.**
- Reproductions: `EXP/out/probes/*/d08_nonclone_endpoint_value.txt` shows a move-only value rejected on a bodyless endpoint (`E0277 Nonce: Clone`, raised from the `derive(Clone)`). The same value is accepted on a body endpoint (`d08::Bounds…post(nonce: Nonce, body …)` compiles). `d08_clone_body_endpoint.txt` shows a body endpoint whose data is all `Clone` still cannot be cloned (`E0599 no method named clone … perhaps you need to implement it`).
- Fix: the D08 prototype.

**DEF-3: the docs claim deep immutability (V09/U26).**
- SEMANTICS §3 says "Client and scope handles are immutable `Arc` snapshots". lib.rs:83–84 says "Handles are immutable snapshots: … requests already created keep their values."
- `EXP/values/tests/d09_snap.rs::snapshot_is_structural_not_deep`: a request created before a mutation of an `Arc<Mutex<String>>`-backed value sends the **new** text. So does a copy-on-write copy, because formatting happens at `prepare`.
- Fix: wording (see D09).

**DEF-4 (docs, minor): misleading generated doc for `maybe_x` on an initialized optional.**
- The doc says "`None` leaves it absent". For `locale?: String = "en"`, `maybe_locale(None)` *removes* the initial `en` (`tests/d02.rs::set_and_omit_initialized_optional`: `limit=20`, no `locale`).
- No generated doc mentions an initializer at all. Rendered: "Sets `limit`." for `limit: u32 = 20`.

Not defects, but notable:
- **N-1:** the `send()` future of an endpoint holding a `!Send` value is `!Send`, while `prepare()?.send()` is `Send`. The async fn captures `self` (`d08_rc_endpoint_send_future.txt`). See D08.
- **N-2:** a borrowed value type `&str` gives E0106 with the suggestion `GET item<'a>(…)`, which is not valid DSL (`d08_borrowed_value.txt`). `&'static str` works (`d08_static_str_value`, exit 0).

---

## D01 Root construction (U03, U04; V01, V11, V12)

**Problem and counterexample.** With positional entry, same-typed required values can be swapped silently. Reordering same-typed declarations swaps every call site silently too.

The security-relevant real shape is common: Algolia, Shopify and Zendesk all put a subdomain next to a secret, and all of these are `String`s. `Search(app_id: String, #[sensitive] api_key: String)` with `subdomain "{app_id}-dsn"` behaves as follows when swapped:
- `Search::new(http, api_key, app_id)` compiles.
- The secret becomes the host label `sekret0key-dsn.algolia.net`, so it is resolved by DNS.
- It is printed by Concord's own Debug: `url: "https://sekret0key-dsn.algolia.net/…", x-algolia-application-id: sekret0key`, and `Search { app_id: "sekret0key", … }`.
- Evidence: `tests/d01.rs::control_string_swap_moves_secret_into_host_and_debug`.

Concord behaves correctly for the declaration. The ambiguity lies in the constructor shape.

Credential pairs of the same type are the normal root shape for many providers: Twilio sid/token, Datadog api/app key, Plaid client_id/secret, Jira site/email/token.

**Control evidence** (real macro; probes run on 1.97.1 and 1.85, identical exits):
- Missing input at n=3: `E0061 … argument #4 is missing`, with help `/* project_id */`. This is good (`d01_pos_missing_r3`).
- Missing **middle** String at n=8 (`region` forgotten): rustc reports **three** errors.
  - `{integer}` is not accepted for `String` (at `56`).
  - `&str` is not accepted for `u64` (at `"shop"`).
  - `argument #9 is missing` with help `/* env */`.

  This blames the wrong input. The cause is the generic `impl IntoValue<T>` parameters, which defeat rustc's argument alignment (`d01_pos_missing_r8_middle`).
- Type swap: two `IntoValue` E0277 errors and no "swap these arguments" help (`d01_pos_type_swap`). A concrete-type constructor gets `help: swap these arguments` (`d01_pos_concrete_type_swap`).
- Same-type swap compiles and changes the wire: `org=34&project=12` (`tests/d01.rs::control_same_type_swap…`).
- Literals: `12` infers `u64` and `"acme"` is accepted for `String`. Widening is rejected: `u32` gives E0277 (`v12_widening`).
- Rustdoc: `new(http: Client, key: impl IntoValue<String>, tenant: impl IntoValue<String>, …)`. Value docs are absent (DEF-1).

**Alternatives built.** All produce identical wire output (`tests/d01.rs::r3_callers_agree`, `r8_callers_agree`). Code is in `EXP/values/src/d01.rs` and `d01_typestate.rs`.

| | Positional (control) | **Struct `XConfig` (ALT-S)** | Ordered steps (ALT-P) | Typestate (ALT-T) | Runtime builder (ALT-B) |
|---|---|---|---|---|---|
| Missing input | E0061; mis-blames at n=8 | `E0063 missing fields \`account_id\`, \`app\`, \`env\` and 2 other fields in initializer of \`R8Config\`` (one error) | `E0599 no method named \`org_id\` found for struct \`R3NeedsTenant\`` | custom `required input \`project_id\` of \`R8\` was not supplied` (one per missing, verbose) | runtime `Err(Missing([...]))` |
| Same-type swap | silent | names at the call site; impossible with field shorthand | names visible | names visible | names visible |
| Author reorders declarations | silent swap | no effect | callers fail to compile | no effect | no effect |
| Duplicate input | n/a | E0062 (standard rustc; not probed) | n/a | `E0599 … private field, not a method` (misleading) | last wins |
| `"x"` for `String` | yes | **no**: `E0308`, with a `.to_string()` hint | yes | yes | yes |
| `12` for `u64` | yes | yes | yes | yes | yes |
| Rustdoc | one long signature, no value docs | one struct page with field docs | n tiny stage types | n generics per impl block | builder methods + error type |
| Generated construction code, n=1/3/8 (bytes, gen_d01.py, approximate) | 97/162/314 | 185/281/488 | 178/621/1768 | 1046/2507/7061 (O(n²)) | 266/667/1581 |
| Call length, n=1/3/8 (chars) | 20/29/60 | 45/77/171 | 24/54/115 | 36/66/127 | 37/67/128 |

Generated-code sizes and compile cost are reasoned from emitted code, not timed.

**Scope and endpoint entries do not need the same mechanism.**
- Arities in `examples/shapes` and the quick start are 0–2: `regional(region)`, `org(org)`, `product(id)`, `repo(owner, name)`.
- The call reads in URL order. The swap risk there is the intrinsic positional risk of any Rust function.
- Nested scopes already name each level (`account(a).zone(z)`).
- A struct per scope or endpoint would double the generated types for little gain. Keep positional there; newtypes remain the remedy for same-typed endpoint IDs.

**Recommendation: CHANGE the root to a generated config struct.**
- Shape: `pub fn new(http: reqwest::Client, config: ApiConfig) -> Api` when the client declares at least one required value. With none, it stays `new(http)`.
- Contents: `pub struct ApiConfig { /// docs  pub key: String, … }` in declaration order, with a `Debug` that redacts exactly as the client's does. No `Default`.
- Optional and initialized root values keep `set_x`/`maybe_x`.
- The macro must reserve `{Client}Config` against generated type names: a scope named `config` under `Api` also generates `ApiConfig`.
- Migration: every `Api::new(http, a, b)` becomes `Api::new(http, ApiConfig { a: a.into(), b })`.

Why this one:
- It is the only option that is ordinary Rust data.
- It gives one complete missing-input error.
- It makes swaps visible, and impossible with field-init shorthand from same-named config variables.
- It is immune to declaration reordering.
- It surfaces the value docs, which fixes DEF-1 for the root.
- It adds one type.

**Also CHANGE (fresh-use friction 1): emit `set_x` for required values**, so the rule becomes "every value has `set_x`; optional ones also `maybe_x`".
- `tests/d01.rs::rotating_a_required_key` shows the problem. Rebuilding the client to rotate a key silently returned to the declared base `http://r1.invalid`, because `with_base_url` and earlier setters were lost.
- A hand-written `set_key`, exactly the existing `controls` body, keeps the staging base. Existing requests keep the old key, so the snapshot rule is unchanged.
- The UI test `required_has_no_setter` must flip. Applying the same rule to scopes and endpoints is optional but keeps one rule.

**Rejected variants:**
- A threshold (positional at exactly one value): two shapes to learn, and adding a second value flips the API shape.
- Typestate: O(n²) code, generic-heavy rustdoc and a misleading duplicate-call diagnostic.
- Runtime builder: compile-time checking is lost for no gain.
- Ordered steps: the runner-up. They keep `IntoValue` literals and are shorter, but they are an unusual pattern, add n public types per client, and have no `build()` boundary.

**Unsupported counterexample left:** `.into()` noise for string literals in the config struct, which mostly appears in tests and examples. The one-token root also gets longer: `GitHub::new(http, GitHubConfig { token })`.

## D02 Optional control vocabulary (U06, U10; V05, V07)

**Problem.** A caller must set, omit and pass through optional values and predict the result. The likely prediction errors are:
- whether `maybe_x(None)` on `x?: T = E` means "default" or "absent";
- `set_x(None)` vs `maybe_x(None)` on `x?: Option<U>` (friction 2).

**Control evidence** (`EXP/values/tests/d02.rs`, `v05.rs`):
- `maybe_locale(None)` on `locale?: String = "en"` gives `limit=20`: absent, not the default.
- Bare `None` infers in `maybe_x` (`bare_none_infers`) and in `set_mark(None)` for `mark?: Option<String>`.
- Passing through an Option the caller already has takes one call: `maybe_cursor(page.next_cursor.clone())`.
- `set_cursor(None)` on `cursor?: String` gives `E0277 \`Option<_>\` is not accepted for a value of type \`String\``. The note does not point to `maybe_cursor` (`d02_set_none_on_optional`).
- The control accepts a value and a child with the same name (`client Api(region?)` + `scope region`), and a value named `send`/`prepare` (`d02_control_*`, exit 0).

**ALT-1 (bare `x(v)` + `maybe_x`, bon-like):**
- Shorter, and reads like reqwest's `RequestBuilder` (`reading_order_at_the_call_site`).
- But it collides with navigation (`E0592 duplicate definitions with name \`region\``) and with built-ins (`E0592 … \`send\``). Declarations that are legal today would have to be rejected.
- It also consumes the conventional getter name: C-GETTER `fn x(&self)` gives `E0592 duplicate definitions with name \`locale\`` (`d02_bare_getter_conflict`).
- Navigation and values become the same kind of word at the call site.

**ALT-2 (`set_x` + `clear_x()`):**
- `clear_locale()` reads unambiguously.
- But passing an existing Option becomes a `match` (`pass_through_an_option…`, 6 lines instead of 1). This hurts the most common task, cursor pagination (U30).

**Recommendation: KEEP `set_`/`maybe_`.** Fix the docs instead of the names.
- `maybe_x` doc: "Sets whether `x` is present: `Some(v)` is `set_x(v)`; `None` makes it absent (no occurrence; the initial value `E` is not used)." This states the `set_x(None)` ≠ `maybe_x(None)` relation that friction 2 trips on.
- `set_x`/`maybe_x` docs on an initialized value: "Initially `<E tokens>`, evaluated when the owner is created."
- Extend the `IntoValue` `on_unimplemented` note with "an optional value is made absent with `maybe_<name>(None)`".
- From recall, not verified here because bon is not in the offline cache: the popular `bon` crate uses the same `maybe_` name, but on a member with a default, `None` there means "use the default". Concord's documentation must therefore state the difference explicitly.

**Unsupported:** a dedicated `clear_x()` and a "set if some" (`if let` remains the route).

## D03 Default lifecycle / reset (U08; V03, V04)

**Evidence** (`EXP/values/tests/d03.rs`; counters private to the fixture, one test thread):
- Client: 1 evaluation even though `set_session` overrides it immediately.
- Scope: 1 evaluation per entry (2 entries gave 2).
- Endpoint: 1 evaluation per call.
- 0 evaluations on clone, prepare and send. A failing `send` was run too.
- `x-request-id` is 4000 across a clone, two prepares and a clone+prepare.
- Body endpoint: the idempotency key is retained via the native `Request::try_clone`. Re-creating the request evaluates a new key (3001), which is the documented rule.
- ALT-R (re-evaluating `reset_req`): it runs the initializer again and yields a *different* value, so it does not restore the original default.
- ALT-L (lazy at prepare, hand-written `LazyProbe`): clone+prepare gives two different request IDs. A retry built by cloning would therefore send a different idempotency key. That is a correctness hazard.

**Recommendation: KEEP. REJECT a re-evaluating reset and lazy evaluation.**
- Returning to the default means using the original handle or a fresh entry.
- With D08, cloning a body endpoint keeps the evaluated key, so a retry that keeps its idempotency key no longer needs the native route.

**Unsupported:** "reset to the originally evaluated value". It would need retained storage per initialized value, and there is no task for it.

## D08 Type bounds and request ownership (U25; V10)

**Audit of generated requirements in the control:**

| Requirement | Where | Evidence |
|---|---|---|
| `Clone` on client/scope values | `#[derive(Clone)]` on the hidden state, needed by `Arc::make_mut` | `d08_nonclone_client_value`: E0277 points at the value's type in the DSL and suggests `#[derive(Clone)]` |
| `Clone` on endpoint values, only without a body | `#[derive(Clone)]` | DEF-2 |
| `Display` | every placement (`Parts::query/header`, `encode_segment`), and generated Debug (`DebugDisplay`) of non-sensitive scalars; `each` needs `&T: IntoIterator<Item: Display>` | UI `option_has_no_wire_spelling`. Debug adds no requirement beyond wire use. |
| `Send`/`Sync` | none required; auto traits propagate | `Rc<str>` at the client makes the client `!Send` (`d08_rc_client_not_send`). A Send-but-`!Sync` value at the client makes it `!Send` (`Arc<State>` needs `Sync`; `d08_send_not_sync_client_value`). `Prepared` is always Send. |
| `'static` | implicit: generated types have no lifetime parameters | `&'static str` works; `&str` gives E0106 with an invalid suggestion (N-2) |
| Bodies | `Encode::Value` moved into `encode` | `Json<Arc<Payload>>` works (serde `rc`). 0 payload clones. |

**Hidden clones** (`EXP/values/tests/d08.rs`, 1 MiB `Big` with counted `Clone`). There are none in common paths:
- 0 clones on `new`, client clone, scope entry, endpoint call, endpoint setter, `prepare` and `send`.
- 1 clone on an explicit endpoint `clone()`.
- 1 clone on a client or scope setter applied to a *shared* handle (copy-on-write copies that owner's values once).
- 0 clones on a setter of a unique handle.

**Feasibility of a conditional Clone on a concrete type** (`EXP/d08_feasibility/run.sh` → `EXP/out/d08_feasibility.txt`):
- `impl Clone for Ep where NotClone: Clone` is rejected on 1.85.1 and 1.97.1 (E0277, trivial bound, issue #48214).
- `impl Clone for Ep where for<'__c> NotClone: Clone` is accepted on 1.85.1, 1.97.1 and nightly 1.101 with `-W future-incompatible` and no warning. The impl simply does not apply. Calling `.clone()` gives `E0599 the method clone exists … trait bound NotClone: Clone was not satisfied`, pointing at the bound.
- Risk: this relies on rustc not checking global bounds under a binder. The stated direction of #48214 is to *accept* trivial bounds, which would only loosen this.

**Prototype in SNAP** (diff: `EXP/out/d08_prototype.diff`):
- `emit.rs` gets a manual `impl Clone for Endpoint where for<'__concord> T: Clone` for each value type, plus `<Body as Encode>::Value`. The bounds are deduplicated and spanned at the declared types. It replaces `#[derive(Clone)]`.
- Docs updated: SEMANTICS §3 (it adds "Clone says nothing about whether re-sending is safe"), the lib.rs retry comment and MIGRATION.
- New test `values.rs::v10_request_clone_follows_the_data`: a body endpoint is cloned, the idempotency key is retained, and a move-only value is accepted.
- New UI test `endpoint_clone_needs_clone_values`: `E0599 … the method clone exists for struct ApiOnce, but its trait bounds were not satisfied … note: trait bound Nonce: Clone was not satisfied` with a caret at `Nonce` in the DSL.
- Results:
  - `cargo test --workspace --locked`: all pass on 1.97.1.
  - `cargo clippy --workspace --all-targets -D warnings`: clean.
  - The MSRV suites from `verify.sh --msrv` pass on 1.85.1 (values: 14 passed).
  - Downstream consumer `EXP/proto` on 1.97.1 and 1.85.1: Json/optional-body/no-value endpoints clone, while a move-only body or value gives E0599 naming the type (`EXP/out/proto/*`).
- Rustdoc renders `impl Clone for ApiCreate where for<'__concord> u64: Clone, for<'__concord> <Json<Doc> as Encode>::Value: Clone`. This is honest but noisy, and `ApiOnce` lists a `Clone` impl whose bound never holds. That is a docs cost; the where-clause states the condition.

**Recommendation: CHANGE endpoints to conditional Clone. A body no longer disables it.**
- Clone of a body request clones the DTO. Whether to resend is the application's decision.
- KEEP the `Clone` requirement on client and scope values. Copy-on-write needs it; the error points at the type; the remedy is `Arc<T>`.
- **DEFER** making `send` a non-async `fn send(self) -> impl Future` that prepares eagerly. It would make the `send()` future `Send` for `!Send` endpoint values (N-1), but the case is rare, `prepare()?.send()` already works, and `async fn` reads better in rustdoc.
- **DEFER** a DSL-level error for non-`'static` references (N-2).

**Unsupported:** borrowed non-`'static` inputs. The routes are owned values, `Arc<str>` / `Json<Arc<T>>`, or `&'static`.

## D09 Snapshots/storage (U05, U09, U26; V08, V09, V13)

**Evidence:**
- Allocation counts (`tests/d09_alloc.rs`, counting global allocator; deterministic counts, not timings):

  | Operation | Allocations |
  |---|---|
  | Scope entry, `String` moved in | 1 (the `Arc`) |
  | Scope entry, `&str` passed | 2 |
  | Endpoint call | 0 |
  | Endpoint setter (`&str`) | 1 |
  | Scope setter, handle shared by a request | 3 (Arc + copied values) |
  | Scope setter, unique handle | 1 |
  | Client setter on a shared client | 2 |
  | `prepare` (2 segments, 2 query pairs, 1 header) | 14 |

- Late parent change (`d09_snap.rs`): an existing request and scope keep `lang=fr` and `v=1`, while new children see `de` and `v=2`.
- Reuse across tasks: four `tokio::spawn`s share one scope by Arc clone.
- Interior mutability: see DEF-3.
- The SNAP `c12` test (independent clients) passes in my SNAP run.

**Alternatives (reasoned).**
- Plain-struct scopes, where an endpoint holds a full copy of the scope: every endpoint call would copy all ancestor values. That is at least one allocation per String per call, versus 0 today.
- `Rc` handles are not `Send`.
- The current design is already minimal for reuse.
- Retention: a child keeps its parent's state alive after the parent handle has been replaced. This is expected and small.

**Recommendation: KEEP. CHANGE the wording** (SEMANTICS §3 and lib.rs:83):
- Replace "immutable snapshots" with copy-on-write wording, and add: "A snapshot is structural: it fixes which value each handle holds, not the content a value refers to. Values are formatted when `prepare` runs, so a value backed by shared mutable state (`Arc<Mutex<_>>`, atomics, a `Display` that reads a clock) is observed at that moment, including by requests created earlier."
- Optionally add one sentence that a setter on a shared handle copies that owner's values once.

**Unsupported:** deep freezing of values.

## D10 Non-wire client state (U24; V14) and Debug (friction 3)

**Evidence:**
- Control: a declared but unplaced value is a liveness error (UI `unused_value`).
- Wrapper route (`EXP/values/src/d10.rs`, `tests/d10.rs`): `App { api: Shop, ctx: Arc<AppCtx> }` and `AppTenant { scope, ctx }`. The application counts calls and holds a display name; nothing reaches the wire.
  - It is about 25 lines of ordinary Rust.
  - It also hosts caller-owned admission (U31) without hooks.
- ALT-LOCAL model (`local_model`): `local ctx: T` would need a stored field, a `ctx()` accessor, *and* a `client()` ancestor accessor on every scope, so that code holding only a scope can reach it. That is a new concept, plus questions about Debug and sensitivity for local values.
- Read accessors: bare `x(&self)` getters would collide with navigation and built-ins in declarations that are legal today (same `E0592` mechanism as D02). No task needs them: the caller supplied the values, and Debug covers diagnostics.
- Friction 1 (rotating a key) is answered by `set_x` for required values (D01), not by accessors.
- Friction 3: Debug prints `VocabSearch { q: Some("pen"), cursor: None, limit: "20" }`. Values are shown as their quoted `Display` wire text. This needs no `Debug` bound on value types, distinguishes `Some("")` from `None`, and is what goes on the wire.

**Recommendation:**
- KEEP liveness and document the wrapper pattern.
- DEFER accessors and `local` values. Revisit if a task shows a scope-only consumer that must reach typed application state.
- KEEP the Debug format, and document "Debug shows each value's wire text (its `Display`), quoted; sensitive values are `<redacted>`".

**Unsupported:** reaching application state from a bare scope or endpoint handle. Pass the wrapper instead.

## V05–V07 and the V-series

Wire results are from the loopback server (`EXP/values/tests/v05.rs`, which uses SNAP's `tests/support`). The client introduces `k=root`.

| Call | Raw target |
|---|---|
| `opt()` | `/opt?k=root` |
| `opt().set_cursor("")` | `/opt?k=root&cursor=` |
| `req_opt(None)` (required `Option` payload with `override "k" = each`) | `/ro` (inherited cleared) |
| `req_opt(Some("x"))` | `/ro?k=x` |
| `opt_place()` (optional, `override "k" = filter`) | `/op?k=root` |
| `opt_place().set_filter("")` | `/op?k=` |
| `nested()` (absent) | `/n?k=root` |
| `nested().set_mark(None)` (present `None`) | `/n` |
| `nested().set_mark(Some(""))` | `/n?k=` |
| `maybe_mark(Some(None))` | `/n` |
| `maybe_mark(None)` | `/n?k=root` |
| `maybe_k(None)` | `/opt` |

The three states are distinct, and only `override` makes absent vs present-`None` observable. Friction 2 lives exactly here: `set_mark(None)` → `/n`, but `maybe_mark(None)` → `/n?k=root`. The D02 docs change addresses it.

| Case | Status (control) | Evidence |
|---|---|---|
| V01 | PASS; diagnostic quality weak at n=8 (D01) | probes `d01_pos_missing_*`; SNAP UI `missing_required_argument` |
| V02 | PASS | SNAP `values.rs::v01_…empty_string` |
| V03 | PASS | `tests/d03.rs`; SNAP `v03_*` |
| V04 | PASS | `tests/d03.rs` |
| V05 | PASS | `tests/v05.rs` |
| V06 | PASS | `tests/v05.rs` (`/ro` vs `/op?k=root`) |
| V07 | PASS; docs misleading (DEF-4) | `tests/d02.rs`, `tests/v05.rs` |
| V08 | PASS | `tests/d09_snap.rs` |
| V09 | **FAIL (wording)** → DEF-3 | `snapshot_is_structural_not_deep` |
| V10 | **FAIL** (body-dependent restriction) → DEF-2; PASS with the D08 prototype | probes + `EXP/proto` + SNAP `v10_request_clone_follows_the_data` |
| V11 | Risk confirmed (silent swap, secret into host) → D01 | `tests/d01.rs` |
| V12 | PASS | `v12_widening`; SNAP UI `conversions_are_closed` |
| V13 | PASS | SNAP `values.rs::c12_*` (ran in the SNAP workspace test run) |
| V14 | PASS via wrapper | `tests/d10.rs` |

## Commands actually run

All builds use `CARGO_BUILD_JOBS=6`, `--offline`, and target directories inside SNAP or EXP.

```text
# expansion (control)
cd SNAP && CARGO_TARGET_DIR=EXP/target-nightly cargo +nightly rustc --locked --offline -p concord-example-shapes --lib -- -Zunpretty=expanded > EXP/out/shapes_expanded.rs
# control experiments (against EXP/snap-control)
cd EXP && python3 -I gen_d01.py EXP          # typestate module + EXP/out/d01_surface_sizes.txt
cd EXP && CARGO_TARGET_DIR=$PWD/target cargo test --offline -p calling-values [-- --nocapture]
cd EXP && CARGO_TARGET_DIR=$PWD/target-1.85 cargo +1.85 test --offline -p calling-values
cd EXP && ./run_probes.sh 1.97.1 1.85        # -> EXP/out/probes/<tc>/<probe>.txt (first line exit=)
cd EXP && CARGO_TARGET_DIR=$PWD/target cargo doc --offline --no-deps -p calling-values   # rustdoc inspection
EXP/d08_feasibility/run.sh > EXP/out/d08_feasibility.txt
# D08 prototype (SNAP working tree)
cd SNAP && CARGO_TARGET_DIR=$PWD/target TRYBUILD=overwrite cargo test --locked --offline -p concord --test ui --test values   # generated the new .stderr (reviewed)
cd SNAP && CARGO_TARGET_DIR=$PWD/target cargo test --workspace --locked --offline
cd SNAP && CARGO_TARGET_DIR=$PWD/target cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cd SNAP && CARGO_TARGET_DIR=$PWD/target-1.85 cargo +1.85 test --locked --offline -p concord --lib --test slice --test values --test composition --test bodies --test representations --test sensitivity --test hygiene --test tls
cd SNAP && CARGO_TARGET_DIR=$PWD/target-1.85 cargo +1.85 test --locked --offline -p concord-example-shapes -p concord-example-renamed -p concord-heap-check
cd EXP/proto && CARGO_TARGET_DIR=$PWD/target-<tc> cargo +<tc> check --offline --bin {ok_clones,fail_clone_move_only_body,fail_clone_move_only_value}   # tc = 1.97.1, 1.85
cd EXP/proto && CARGO_TARGET_DIR=$PWD/target-1.97.1 cargo +1.97.1 doc --offline --no-deps
```

Everything else here (generated-code sizes for D01, compile cost, the alternatives for D09, the bon convention) is reasoned, as stated in each section. I ran no timing benchmarks.

## Files

- Report: `/home/canarit/projects/concord_v4_refine_work/reports/calling.md`
- EXP:
  - `values/` holds the control fixtures, the hand-written alternatives and the tests.
  - `probes/src/bin/*.rs` are the compile probes, with outputs in `out/probes/{1.97.1,1.85}/`.
  - `proto/` is the D08 consumer of the SNAP prototype.
  - `d08_feasibility/` holds the HRTB feasibility files.
  - `gen_d01.py` and `run_probes.sh` are the generators/runners.
  - `snap-control/` is the pristine control worktree.
  - Other outputs: `out/d08_prototype.diff`, `out/shapes_expanded.rs`.
- SNAP (prototype only):
  - `crates/concord-macros/src/emit.rs`, `crates/concord/src/lib.rs`, `crates/concord/tests/values.rs`, `crates/concord/tests/ui/endpoint_clone_needs_clone_values.{rs,stderr}`, `v4/SEMANTICS.md`, `docs/MIGRATION.md`.
  - The new UI files are marked intent-to-add (`git add -N`), so they appear in `git diff`. Nothing is committed.
