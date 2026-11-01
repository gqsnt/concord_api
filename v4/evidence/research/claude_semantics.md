<!-- Verbatim final report of a Claude subagent (Opus 5.5), extracted from its transcript. -->

# Concord V4 value and type-system design: three models, with Model A recommended

Labels: **F** = fact I observed (compiled or ran, or read in V3 source). **H** = hypothesis or judgement.
- I did not read `/home/canarit/projects/concord_api_v4/v4/` and did not edit any repository file.
- **F:** the toolchain is actually **rustc 1.99.0** (2026-09-28), not 1.97.1.
- **F (V3 background):** V3 only accepts initializers that are literals or constant paths. See `/home/canarit/projects/concord_api_v3ref/crates/concord-macros/src/syntax/values.rs:536-595` and `docs/spec.md:269-277`.
- **F (V3 background):** V3 has 5 controls per value (`set`, `replace`, `set_if_some`, `omit`, `reset`; `docs/spec.md:286-316`) and a sealed `IntoValue` (`crates/concord-core/src/value.rs`).

## 1. The three models

The same declaration is used for all three models:
```
client Shop { values { tenant: String; version: u32 = 1; locale?: String = "en"; trace?: String; }
  scope regional(region: Region) {
    GET items(filter: Option<String> via NoneAsEmpty) -> Vec<Item> {
      values { cursor?: String; milestone?: Option<u32> via NoneAsNull; stamp: u64 = next_stamp(); } } } }
```

### Model A: required values passed positionally at entry, plus prefixed control methods (recommended)

```rust
impl Shop {
  pub fn new(http: reqwest::Client, tenant: impl IntoValue<String>) -> Shop;
  pub fn set_version(self, v: impl IntoValue<u32>) -> Self;   pub fn reset_version(self) -> Self;
  pub fn set_locale(self, v: impl IntoValue<String>) -> Self;  pub fn omit_locale(self) -> Self;  pub fn reset_locale(self) -> Self;
  pub fn set_trace(self, v: impl IntoValue<String>) -> Self;   pub fn maybe_trace(self, v: Option<String>) -> Self; pub fn omit_trace(self) -> Self;
  pub fn regional(&self, region: impl IntoValue<Region>) -> shop::Regional;
}
impl shop::Regional { pub fn items(&self, filter: impl IntoValue<Option<String>>) -> shop::regional::ItemsRequest; }
impl ItemsRequest {
  set_cursor(impl IntoValue<String>) / maybe_cursor(Option<String>) / omit_cursor()
  set_milestone(impl IntoValue<Option<u32>>) / maybe_milestone(Option<Option<u32>>) / omit_milestone()
  set_stamp(impl IntoValue<u64>) / reset_stamp()
  // built-ins (owned by the composition design): send(), native-request access. None may start with set_/maybe_/omit_/reset_.
}
```

Call site:
```rust
let shop = Shop::new(http, "acme").set_version(2).omit_locale();
let req = shop.regional(Region::Eu).items(None).maybe_cursor(next.clone()).set_milestone(None);
```

Rules:
- **Required values:** every required value is positional, in declaration order, in its owner's entry call: `new`, the scope method, or the endpoint method.
- **No separate builder:** the client is its own builder.
- **Immutable handles:** every handle is an immutable value. Controls take `self` and return `Self`. Copy-on-write sits on top of `Arc` snapshots.
- **Concept count, about 8:** positional required values, `?`, `= E`, the four control prefixes, immutable handles, local names, closed conversions, `via` for `Option`.
- **Method count:** at most 3 per value (2 typical), down from V3's 5.

### Model B: plain data with pub fields

```rust
pub struct ShopArgs { pub tenant: String }                // required values: a struct literal
impl Shop { pub fn new(http: reqwest::Client, a: ShopArgs) -> Shop; }
pub struct ItemsRequest { pub cursor: Option<String>, pub milestone: Option<Option<u32>>, pub stamp: u64, /* private */ }
impl ItemsRequest { pub fn with(self, f: impl FnOnce(&mut Self)) -> Self; }
```

Call site: `shop.regional(r).items(None).with(|r| { r.cursor = next.clone(); r.milestone = Some(None); })`

- Concept count is about 7, but root and scope values need a different mechanism (see the counterexamples in section 4), so in practice B becomes a hybrid.
- There is no per-value method at all.

### Model C: typed keys with fixed generic verbs

```rust
impl ItemsRequest {
  pub fn set<K: Key<Self>, V: IntoValue<K::Ty>>(self, k: K, v: V) -> Self;
  pub fn maybe<K: Optional<Self>>(self, k: K, v: Option<K::Ty>) -> Self;
  pub fn omit<K: Optional<Self>>(self, k: K) -> Self;
  pub fn reset<K: Initialized<Self>>(self, k: K) -> Self;
}
pub mod shop::regional::items { pub struct Cursor; pub struct Milestone; pub struct Stamp; }
```

Call site: `eu.items(None).maybe(items::Cursor, next.clone()).set(items::Milestone, None)`

- The verb surface is fixed at 4 methods, plus one zero-sized key type per value.
- Concept count is about 9 and needs more Rust-generics literacy from users.

Required-input enforcement is a separate axis. I compiled all four options: positional (A and C), struct literal (B), typestate with `on_unimplemented`, and a runtime aggregated error.

## 2. Decision tables

### Per-declaration semantics in the recommended Model A

| Declaration | Slot | Initial state | Supplied at | Controls | Wire when absent |
|---|---|---|---|---|---|
| `x: T` | `T` | caller | positional at owner entry | none | always one occurrence |
| `x?: T` | `Option<T>` | absent | — | `set_x`, `maybe_x`, `omit_x` | no occurrence |
| `x: T = E` | `T` | E, retained | — | `set_x`, `reset_x` | always one occurrence |
| `x?: T = E` | `Option<T>` | Present(E), retained | — | `set_x`, `omit_x`, `reset_x` (no `maybe_x`) | no occurrence after omit |
| `x: Option<T> via Enc` | `Option<T>` | caller | positional | none | always one occurrence; Enc spells `None` |
| `x?: Option<T> via Enc` | `Option<Option<T>>` | absent | — | `set_x(Option<T>)`, `maybe_x(Option<Option<T>>)`, `omit_x` | outer absent: none; `Some(None)`: Enc spelling |

Transitions (exact):
- `set_x(v)` makes the slot Present(v).
- `maybe_x(o)` sets the slot to exactly `o`, so `None` means absent.
- `omit_x()` makes the slot absent.
- `reset_x()` restores the retained initial value and never re-evaluates E.

Why there is no `maybe_x` on `x?: T = E`: there, `None` could mean omit, keep, or reset, and the design refuses to guess.

`set_if_some` is deliberately unsupported (V07); write `if let Some(v) = o { r = r.set_x(v) }`.

Initializer rule: E is any closed Rust expression (it cannot refer to other declared values). It runs exactly once per owner construction (`new`, each scope entry, each endpoint call), in declaration order. It runs even if the value is overridden immediately. It never runs on clone, set, omit, reset, materialization or send. `#[default]` becomes unnecessary because users write `= Default::default()`.

### Model comparison

| Criterion | A: positional + prefixed methods | B: pub fields | C: typed keys |
|---|---|---|---|
| Missing required value | E0061 that names the parameter (**F**) | E0063 "missing field `tenant`" (**F**) | same as A, or typestate |
| Same-typed swap | **compiles silently** (**F**) | prevented | silently swapped if positional (like A); prevented with typestate |
| `x?: T` vs `x: Option<T>` | distinct by construction | both `Option<T>` if required values become fields (ambiguous) | distinct |
| `x?: Option<T>` | `set_x(None)` vs `omit_x()` | `Some(None)` vs `None` (explicit) | `set(K, None)` vs `omit(K)` |
| Methods per value | ≤3 | 0 | 0, plus 1 type |
| Reset | retained, no re-evaluation | none; user re-types the default | retained |
| V11 collisions | prefixes are injective (first letters s/m/o/r); navigation names cannot use those prefixes | impossible: fields and methods are separate namespaces (**F**) | impossible for values; key types can collide with user imports |
| V12 conversions | sealed `IntoValue` | exact type, so `.into()` noise | sealed; inference works (**F**) |
| IDE discoverability | best (`req.set_` lists values) | good (fields) | poor (must know the module) |
| Root/scope snapshot | `Arc` copy-on-write, cheap | pub fields force flattened copies | same as A |
| Macro complexity | low–moderate | low for endpoints, mixed for root | moderate (trait impls per value) |
| Wrong-owner use | method does not exist (E0599) | field does not exist | custom E0277 (**F**) |

Rows that apply to every model:

| Topic | Decision |
|---|---|
| Default evaluation | E1: once per owner construction, retained (**F**, exp2) |
| V10 snapshots | handles are immutable values; a later change to a parent produces a new value, and existing requests keep their old snapshot (**F**) |
| C05 ancestor naming | a bare name means the current owner only; ancestors are written `client.x` or `regional.x`; adding a parent value never rebinds a child (**H**: it is a macro rule) |
| `Option` on the wire | no default `Atom` for `Option<T>`; `via Enc` is required; an encoder must emit exactly one scalar; only `?` can suppress an occurrence (**F**, exp5) |

## 3. Experiments

All crates are under `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/`. Each was built with `CARGO_TARGET_DIR=<that dir>/target cargo run|build --offline -q [--features X]`.

### exp0: is `default_field_values` usable on stable?

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp0_dfv/main.rs`, built with `rustc --edition 2024`.
```
error[E0658]: default values on fields are experimental
error[E0015]: cannot call non-const function `next` in constants
```
Result: B cannot use `Args { id, .. }` defaults on stable, and that feature is const-only anyway.

### exp1: missing required input in each enforcement model

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp1_required/src/main.rs`

Positional (A):
```
error[E0061]: this function takes 3 arguments but 2 arguments were supplied
97 |     let _ = model_a::Api::new(http.clone(), "acme");
   |             ^^^^^^^^^^^^^^^^^---------------------- argument #3 is missing
help: provide the argument
97 |     let _ = model_a::Api::new(http.clone(), "acme", /* region */);
```

Struct literal (B):
```
error[E0063]: missing field `tenant` in initializer of `ApiInput`
```

Plain typestate:
```
error[E0599]: no method named `build` found for struct `model_c_plain::ApiBuilder<model_c_plain::Unset, model_c_plain::Set<String>>`
    = note: the method was found for `model_c_plain::ApiBuilder<model_c_plain::Set<String>, model_c_plain::Set<String>>`
```

Typestate with `on_unimplemented`:
```
error[E0277]: required root value `tenant` was not supplied
106 |     let _ = model_c_diag::builder().region("eu").build();
    |                                                  ^^^^^ call `.tenant(...)` before `.build()`
    = note: declared as `tenant: String` in `client Api`
```

Runtime error: `D: missing required root values: tenant (call .tenant(..))`

Same-typed swap under positional arguments: `SWAP COMPILED: tenant=eu&region=acme&v=1`

### exp2: evaluation points, clone, reset, and V10 snapshots

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp2_defaults/src/main.rs`

E1 (evaluate at owner construction):
```
Api::new() [eval #1 root.version]   api.clone() (none)   regional x2 [#2][#3]   eu.clone() (none)
eu.item x3 [#4][#5][#6]   r1.url() x2 / clone: stamp=4 each, no eval
r1.set_stamp(99).reset_stamp() -> stamp=4 (no eval)
eu.set_lang(de).omit_lang().reset_lang() -> lang=l2 (retained)
-- V10: r1 (old) = /eu/items/1?v=1&stamp=4&lang=l2 after eu.set_lang("de") and api.set_version(7)
   api_v7.regional.. = /us/items/5?v=7...   original api = ...v=1...
```

E2 (lazy, evaluate at every send):
```
send x2 -> stamp=13, stamp=14; clone+send -> 15; set(99).reset() -> 16
```

Shared-mutable anti-model:
```
before: ?v=1   after client.set_version(7), SAME request: ?v=7
```

Feature `fail_const_init`:
```
error[E0015]: cannot call non-const function `tick` in constants
error[E0015]: cannot call non-const method `<str as ToString>::to_string` in constants
```

### exp3: conversions (V12)

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp3_conv/src/main.rs`

`impl Into<T>` silently accepts unintended conversions: `user=UserId(42)` from a raw `u64`, and `email=Email("not-an-email")`.

`impl Into<u64>` rejects an unsuffixed integer literal:
```
error[E0277]: the trait bound `u64: From<i32>` is not satisfied
45 | ReqInto.set_limit(7);
```

An exact `String` parameter rejects a literal:
```
error[E0308]: mismatched types ... expected `String`, found `&str`
```

Sealed `IntoValue` gives targeted errors:
```
error[E0277]: `u64` is not accepted for a value of type `UserId`
error[E0277]: `u32` is not accepted for a value of type `u64`
error[E0277]: `Option<&str>` is not accepted for a value of type `Option<String>`
```
With the sealed trait, `set_limit(7)`, `set_name("lit")` and `set_note(None)` all compile.

### exp4: naming collisions, plus B and C prototypes

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp4_names/src/main.rs`

Bare setter next to a built-in:
```
error[E0592]: duplicate definitions with name `send`
```

Built-in as a trait method, bare inherent setter shadows it:
```
error[E0061]: this method takes 1 argument but 0 arguments were supplied ... shadow::Req.send(/* bool */)
```

Suffix naming scheme:
```
error[E0592]: duplicate definitions with name `set_x_opt`
```

Model B: a pub field `send` and a method `send()` compile together. Output: `B: send=Some(true) milestone=Some(None)...`

Model C: `C: cursor=Some("a") version=Some(1) milestone=Some(None)`. Misuse gives targeted errors:
```
error[E0277]: value `version` is not optional (declared without `?`)
error[E0277]: value `cursor` has no initializer
error[E0277]: the trait bound `lang: model_c::Key<item::ItemRequest>` is not satisfied
```

### exp4b: lowercase key types capture user locals

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp4b_keyshadow/src/main.rs`
```
error[E0530]: let bindings cannot shadow unit structs
6 |     let mut cursor: Option<String> = None;
```

### exp5: explicit `Option` wire encoding

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp5_optwire/src/main.rs`
```
V04 cursor?: String absent -> (no query)          present "" -> cursor=
V05 filter: Option<String> via NoneAsEmpty None -> filter=
V06 m?: Option<String> absent        NoneAsEmpty -> (no query)  NoneAsNull -> (no query)
V06                    present None  NoneAsEmpty -> m=          NoneAsNull -> m=null
V06                    Some("")      NoneAsEmpty -> m=          NoneAsNull -> m=
error[E0277]: `Option<String>` has no default wire spelling
   |  declare an explicit encoder: `x: Option<String> via SomeEncoder`
   = note: Concord never infers how `None` is written; `None` cannot mean "omit" -- use `x?: T` for omission
```

### exp6: clone and memory cost (counting allocator, release build)

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp6_cost/src/main.rs`

Representations measured:
- **R1:** one `Arc` per owner, values stored inline.
- **R2:** every child holds a flattened copy of all ancestor values.
- **R3:** one `Arc` per value.

```
size_of Item: R1 96B, R2 224B, R3 56B
R1 entry 4 / endpoint 3 / clone 3 / scope setter while shared 5 allocs
R2 entry 6 / endpoint 9 / clone 9
R3 entry 5 / endpoint 5 / clone 0 / reset 0
```

### exp7: typed-key inference and wrong-owner diagnostic

File: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/semantics/exp7_keyinfer/main.rs`

`.set(Limit, 7).set(Cursor, "abc").set(Milestone, None)` gives `["7", "\"abc\"", "None", "Some(3)"]`. A custom diagnostic works:
```
error[E0277]: `Lang` is not a value of `Req`
```

## 4. Counterexamples and rejected hypotheses

### Counterexamples

**Model A**
- **Swapped arguments compile:** `repo(owner: String, name: String)` with swapped arguments compiles silently (**F**, exp1). If the API author later reorders the declarations, every call site is silently swapped.
- **Endpoint named like a control:** an endpoint `set_lang` in a scope that has a value `lang` produces E0592 unless the macro rejects reserved prefixes on navigation names.
- **Endpoint named like a trait method:** a root endpoint named `clone` shadows `Clone::clone` (**F**: inherent methods win, exp4). These names need a reserved list.

**Model B**
- **Ambiguous `None`:** if required values were exposed as fields, `x?: T` and `x: Option<T>` would both be `Option<T>`, so `r.filter = None` and `r.cursor = None` would look the same in Rust but mean different things on the wire.
- **Root fields are expensive:** pub fields on the root force either flattened copies (R2: 9 allocs per clone) or an `Arc::make_mut` dance.
- **Weaker ergonomics:** no reset, no chaining without `with`, and `.into()` noise.

**Model C**
- **Lowercase keys:** they break user locals with the same name (E0530).
- **Uppercase keys:** they collide with users' DTO imports (for example a `Cursor` response type).
- **Discoverability:** IDE completion shows verbs, not values.

**All models**
- `NoneAsEmpty` makes present-`None` identical to `Some("")` on the wire. This is the encoder's explicit choice, but it can surprise.
- A present-empty `each` vector and an absent one are wire-identical in a query. The distinction is only observable through composition operations.

### Rejected hypotheses

| Hypothesis | Evidence for rejection |
|---|---|
| Lazy evaluation at send (E2) | clone+send yields different values (13–16); reset ≠ retained; breaks user-composed re-send determinism |
| `impl Into<T>` setters | literal `7` fails for `u64`; `u64` silently accepted for `UserId` |
| Exact-type `T` setters | `"lit"` rejected for `String` |
| Const-only initializers via a const item | `.to_string()` is not const; would need V3's syntactic seed restriction |
| `default_field_values` | unstable on 1.99 |
| Lowercase keys | E0530 |
| `_opt` suffix naming | E0592 |
| Bare setter names | E0592, or shadowing (E0061) |
| Shared mutable client | V10 violation shown |
| Flattened copies | allocation cost |
| `maybe_x` on initialized optionals | `None` is ambiguous |
| `set_if_some` | ordinary Rust covers it |
| `Atom` for `Option<T>`, or encoders that may return "absent" | would create two sources of absence |
| Root typestate as the default | **H:** extra generic parameters, which hurt hover text and storing builders; kept as the fallback |

## 5. Recommendation, and its strongest weakness

I recommend **Model A**: required values passed positionally at the owner's entry, with immutable copy-on-write snapshot handles. Representation should be R1 with an `Arc` per owner, endpoint-owned values included; R3 if a `T: Clone` bound is unwanted (**H**).

**Default evaluation:**
- Defaults are evaluated by E1 (once per owner construction) and retained for `reset`.
- Changing a parent creates a new value; scopes and requests already built keep their snapshot.
- The cost to users is real: rotating a credential means replacing the client value in their own cell (`ArcSwap`/`RwLock<Shop>`), and every handle clone is O(depth) refcount bumps.

**Controls and conversions:**
- Controls use four prefixes. Navigation names may not start with those prefixes. Request built-ins avoid them. The macro checks all generated names per type and reports any collision with both declaration spans (**H**: those spans are not yet tested).
- Setters use a sealed `IntoValue` (identity, `&str -> String`, `&String -> String`). It sees through type aliases because dispatch is by trait, not by type spelling, and newtypes must be constructed explicitly.

**Ancestor naming:** bare names belong to the current owner only. `client.locale` and `regional.locale` must be explicit. Controls exist only on the owner's own handle, so a child can never change a parent value.

**`Option` payloads:** they need `via Enc`, and only `?` can suppress an occurrence.

**Strongest weakness.** Model A's safety depends on user discipline exactly where it matters most: required inputs.
- Positional, same-typed arguments (`owner`/`repo`, `tenant`/`api_key`) swap without any compile error.
- An API author reordering declarations silently swaps every caller.
- With secrets, a swap can put a key into a non-sensitive slot, which Concord's own debug output will then print.
- Newtypes, which the sealed conversion encourages, are the only mitigation; nothing structural prevents it.
- Positional required values were adopted for teachability, and this is exactly the hazard that named or typestate entry removes.
- **H:** I also worry that A is "V3 minus two controls", so some convergence may be inertia rather than independent evidence.

## 6. Discriminating tests that could change the decision

- **T-SWAP:** in 3 representative APIs, count owners with 2 or more required values of the same type. If that is common and the ergonomics study rejects newtypes, switch those entry points to an `on_unimplemented` typestate (diagnostic shown in exp1) or named argument structs.
- **T-MAYBE:** measure how often callers already hold an `Option` for `x?: T`, for example in user-written pagination loops. If it is rare, drop `maybe_x` and accept `if let`.
- **T-RESET:** if no realistic example uses `reset_x`, drop it and the retained storage. V02 and V08 would then be "deliberately unsupported".
- **T-V03-COUNTER:** the counter must increase by exactly 1 per `new`, scope entry and endpoint call, and by 0 on clone, set, omit, reset, native-request build and send. Expected result as in exp2.
- **T-V10:** a request built before `shop.set_version(7)` or `eu.set_lang("de")` must still produce its old URL and headers.
- **T-V11:** a value `x` alongside a value `x_opt` must compile. An endpoint named `set_lang` in an owner with value `lang`, or a root endpoint named `new` or `clone`, must produce a macro error naming both origins.
- **T-C05:** root and scope both declare `locale`, and the scope's bare `locale` binds to the scope. Deleting the scope's `locale` must give an error suggesting `client.locale`, not a silent rebind.
- **T-V12:** `set_user(42u64)` for `UserId` must not compile. `set_limit(7)` for `u64` and `set_name("x")` must compile.
- **T-OPT-ATOM / V06 / C11:** an `Option` placement without `via` must not compile. The NoneAsNull and NoneAsEmpty results in exp5 must match. An omitted `cursor?` must differ from `filter: None`.
- **T-DIAG-SPAN (H, untested):** proc-macro-emitted E0061 and `on_unimplemented` diagnostics must point at the DSL tokens. If they do not, the diagnostic advantages of typestate and of Model C disappear.
- **T-INIT-HYGIENE:** an initializer must resolve items in the caller's module, and an initializer naming a declared value must be rejected. If this cannot be done cleanly, fall back to V3's literal and const-path initializers.
- **T-EACH-EMPTY:** absent versus present-empty `each`, under each composition operation (`when_present`, `replace`).
- **T-IDE / compile cost:** rust-analyzer completion and expansion time for a 50-endpoint API under A versus C. If C is clearly cheaper and users accept keys, C becomes competitive because its surface is fixed and collision-free.
