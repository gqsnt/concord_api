# Families I and N — codex

Repository inspected: `/home/canarit/projects/Concord_Next/work/wt/rv-p1`, commit `ba10189b`. All source references below are relative to that repository; ranges include comments and blank lines unless a different measure is stated. These are alternative designs, not an implementation or an additive feature list. Proposed syntax and generated APIs are explicitly distinguished from current code.

I read the charter, the I/N seeds, the construction problem, the glue inventory, the current language guides, the witness sources and the three simple controls. The consolidated `work/inventory/FRICTION.md` was absent, so I used the SDK-local friction, maintenance and first-use records. Historical notes were checked against current source: Nager and Frankfurter now use generated `send()`, and PokéAPI's trial `follow_with_limit` was reverted.

## Evidence that changes the premise

- `IntoValue` lives in `crates/concord-core/src/value.rs`, re-exported by `crates/concord/src/lib.rs:248–254`. Its `#[diagnostic::on_unimplemented]` already exists at `value.rs:52–56`. Adding that attribute is not a new design and does not fix E0283 inference.
- The strongest actual input failure is Frankfurter's date: `examples/frankfurter-sdk/FRICTION.md:16–22` and current `tests/protocol.rs:43–49`. The workaround is a type annotation, not a handwritten SDK forwarder.
- Current conversion is semantic, including aliases: identity for every `T`, borrowed strings for `String`, and borrowed native headers for `HeaderValue` (`value.rs:7–88`). `examples/renamed/src/lib.rs:8–12,27–38` proves dependency renaming and native type aliases matter. The requested generic `V: IntoValue<u16>` forwarder is an adversarial downstream example; the inspected Rust sources contain no literal occurrence of that bound. It must still be tested.
- Nager's classified glue is 74 nonblank lines: 66 error and 8 result, **zero input, forwarding, construction or paging glue**. PokéAPI has 13 forwarding and 10 paging lines; Frankfurter has 122 error and 8 result lines. An input-signature change cannot honestly claim those error/result deletions.
- No SDK source or manifest uses `concord-extras`. Two compatibility consumers do: `checks/consumers/extras-core/{Cargo.toml:11,src/lib.rs:11}` and `extras-legacy/{Cargo.toml:11,src/lib.rs:3}`. A facade is not therefore unused by the whole repository.
- No example SDK calls `first_page_then`; its Rust uses are library implementation, doctests and pagination tests. This is an in-tree usage finding, not evidence about published downstream users.
- Capability crates are already used: **20 SDK manifests use auth, 16 retry, 4 ratelimit**. For example, Sentry's policy uses `Retry::receive_with` and holds admission state through final decoding (`examples/sentry-sdk/src/policy.rs:118–165,244–295`); Strava and Xero use all three crates. Deleting these as “pilots” would recreate behavior and break users. Keep them optional Rust libraries; their status needs a supported-contract decision, not DSL retry/auth/limiter switches.
- `concord_core::Error::meta()` is **not deprecated** in this tree (`crates/concord-core/src/error.rs:268–276`). It means “head of the still recoverable response,” whereas `head_meta()` means “head arrived.” Checks deliberately depend on the difference. SDK-owned methods also named `meta()` are separate APIs.
- Square already implements both `SendWith<T>` and `SendOperation<T, SquareOperation>` (`examples/square-payments-sdk/src/lib.rs:105–133`). Its second implementation only forwards to the first. Any universal blanket adapter must migrate this actual SDK before it can compile.

The zero SDK-deletion counts below are intentional. I and much of N reduce inference friction or library surface; A/B/C/F remain necessary to remove the bulk of measured SDK glue. Counts for alternatives must not be added together.

## I1. Exact declared inputs everywhere — smaller than you think

**Fact it expresses:** The provider input has the declared Rust type, independent of how that type is imported or aliased.

**Mechanism:** Generate `arg: T` and `set_x(value: T)` everywhere, including parameter-group trait setters, body setters and initializer return expressions. Keep `maybe_x(Option<T>)` and config fields as they already are. No String-name whitelist, type inspection, new DSL attribute or input-mode statement. This removes the conversion layer instead of making authors select it. Keep the old public `IntoValue` trait temporarily as an ordinary downstream migration utility; stop generating bounds on it.

**Declaration (author writes), before → after:** Frankfurter, `examples/frankfurter-sdk/src/lib.rs:50–52`, unchanged:

```rust
// Before
GET historical_pair(base: String, quote: String, query date: Date, query providers?: String) -> Rate {
    path "v2/rate/{base}/{quote}";
}
// After: identical declaration
GET historical_pair(base: String, quote: String, query date: Date, query providers?: String) -> Rate {
    path "v2/rate/{base}/{quote}";
}
```

Renamed's existing initializer must migrate explicitly (`examples/renamed/src/lib.rs:30`):

```rust
// Before
header("accept") accept: Native = &ACCEPT,
// After
header("accept") accept: Native = ACCEPT.clone(),
```

**Call site (caller writes), before → after:** Based on Frankfurter `tests/protocol.rs:46–49`:

```rust
// Before
api.historical_pair("EUR", "USD", "2024-01-15".parse::<Date>().unwrap())
    .set_providers("ECB,TCMB");
// After
api.historical_pair("EUR".into(), "USD".into(), "2024-01-15".parse().unwrap())
    .set_providers("ECB,TCMB".into());
```

`Default::default()` and `.into()` now get the declared target too. A forwarder migrates without strengthening its public bound:

```rust
// Before: adversarial downstream forwarder over the real Nager endpoint
fn holidays<V: concord::IntoValue<u16>>(api: &NagerHolidays, year: V) -> NagerHolidaysHolidays {
    api.holidays("AT", year)
}
// After: the caller-facing generic contract can remain
fn holidays<V: concord::IntoValue<u16>>(api: &NagerHolidays, year: V) -> NagerHolidaysHolidays {
    api.holidays("AT".into(), concord::IntoValue::into_value(year))
}
```

**SDK code that disappears:** None in the inspected input witnesses: **0 SDK glue lines**. Frankfurter's `tests/protocol.rs:47` loses `::<Date>` but gains explicit string conversions; this is an inference trade, not a net glue reduction. Internally, conversion calls/bounds disappear from `crates/concord-macros/src/emit.rs:194–204,779–824,873–888,928–946,2524` (roughly 15–25 affected code lines, mostly replacements). After a separate major-release removal of the compatibility trait, `crates/concord-core/src/value.rs:7–88` loses about 60 nonblank lines plus re-exports. This is the minimum comprehensive change that removes most of the *input-conversion machinery*, not Nager's 74 lines of unrelated glue.

**Hardest counterexample:** Renamed's `Native` remains exactly the native type. `&HeaderValue` becomes `value.clone()`; the native bytes and sensitivity flag must be preserved. For `type Country = String`, `"AT"` is rejected uniformly for both spellings; `"AT".into()` works for both. Generic `IntoValue<u16>` forwarders need the explicit conversion shown above. Plain numeric literals still infer to the exact numeric parameter; numeric widening and implicit newtype construction remain disallowed.

**Simple control (Nager):** `src/lib.rs:22–37` is byte-for-byte unchanged. Calls change from `holidays("AT", 2026)` to `holidays("AT".into(), 2026)`. PokéAPI and Frankfurter declarations also remain unchanged. This does spend simple-caller convenience; if C3 is interpreted to freeze those literal calls as well as declaration shape, reject I1.

**Charter check:**

- C1: No declaration word is added; `T` keeps its ordinary Rust meaning.
- C2: Conversion is explicit Rust at the ownership boundary; provider placement and format stay declared once.
- C3: All three control declarations keep their shape; existing borrowed-string calls break, an explicit acceptance condition.
- C4: Aliases and renamed dependencies behave identically; no syntactic type guessing.
- C5: Exact argument errors point at the argument and its declared type; expansion spans for initializers must point at the authored expression.
- C6: One rule: “pass the declared type.” No conversion matrix on every endpoint.
- C7: `reqwest`, `prepare`, `send_raw`, `into_parts` and open formats remain available.
- C8: No runtime change; ownership conversion occurs before request construction.
- C9: Require compile-pass cases for inline `parse`, `.into`, defaults, aliases and move-only bodies; compile-fail cases for wrong numerics/newtypes; byte-and-sensitive-flag assertions for cloned headers. These are proposed checks, not executed evidence.

**New concepts:** None in the declaration; exact Rust parameters replace generated `IntoValue` bounds.

**Breaking changes:** Borrowed strings/headers at all generated supply points; literal/borrowed default expressions; generic forwarder bodies; function-item signatures and generic setter abstractions. Removing the public trait is a later, separately enumerated break. Config fields and `maybe_*` already require exact types.

**Why it might be wrong:** Most ordinary calls use string literals; solving one date annotation can add several `.into()` calls.
It is a deliberately broad compatibility change with zero demonstrated SDK-glue deletion in these witnesses.

## I2. Add named, exact input records without changing positional calls

**Fact it expresses:** An endpoint's required inputs form a typed product in their declared order, with the declared names and types.

**Mechanism:** Generate an additional input record and entry method only where required arguments exist: `<EndpointType>Input` and `<name>_input(record)`. Its fields are exact types. Preserve the current positional method and converting setters. The record is consumed, so it adds no `Clone`, `Default`, `Send` or lifetime requirement. Generated code destructures it exhaustively and constructs the same endpoint. This is an additive route to contextual inference, rather than a new conversion policy.

**Declaration (author writes), before → after:** Nager, `examples/nager-holidays-sdk/src/lib.rs:35–37`, unchanged:

```rust
// Before
GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
    path "api/v4/Holidays/{country_code}/{year}";
}
// After: identical declaration
GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
    path "api/v4/Holidays/{country_code}/{year}";
}
```

Proposed generated surface, not authored DSL:

```rust
pub struct NagerHolidaysHolidaysInput {
    pub country_code: String,
    pub year: u16,
}
// fn holidays_input(&self, input: NagerHolidaysHolidaysInput) -> NagerHolidaysHolidays
```

**Call site (caller writes), before → after:**

```rust
// Before, contextual expressions require annotations
api.holidays("AT", "2026".parse::<u16>()?);
// After, optional named route
api.holidays_input(NagerHolidaysHolidaysInput {
    country_code: "AT".into(),
    year: "2026".parse()?,
});
// The existing short route stays valid:
api.holidays("AT", 2026);
```

For Frankfurter, the corresponding `FrankfurterHistoricalPairInput` gives `.parse()?` a `Date` field target while leaving every existing call intact.

**SDK code that disappears:** **0 demonstrated SDK lines** in Nager `src/lib.rs:35–37`, Renamed `src/lib.rs:16–38` or Frankfurter `src/lib.rs:42–52`; none handwrites this input record. Do not count generated fields as removed glue. A future SDK record/forwarder that merely copies required arguments could be replaced, but no deletion is booked without a real matching witness. The immediate benefit is target inference and discoverable argument names.

**Hardest counterexample:** Aliases and native-header values need no classification: their fields are exact `T`. A native-header field takes `ACCEPT.clone()`, while the old setter still accepts `&ACCEPT`. The generic Nager `V: IntoValue<u16>` forwarder continues to use the old positional method unchanged. Exact named fields do not solve inference in existing `set_x(Default::default())` calls; use a typed local or exact `maybe_x(Some(...))` where applicable.

**Simple control (Nager):** Declaration and existing calls unchanged. Extra generated names are available but optional. PokéAPI and Frankfurter also gain no authored declaration line or required import.

**Charter check:**

- C1: Record contents derive only from existing required provider inputs; no mode selector is authored.
- C2: Facts stay in one declaration; Rust expression evaluation remains caller code.
- C3: Existing declarations and navigation work unchanged; extra rustdoc surface is the cost.
- C4: Type name is the existing endpoint type plus `Input`; method is endpoint name plus `_input`.
- C5: Missing/wrong fields produce Rust errors naming authored field names; reject generated-name collisions at the conflicting declaration.
- C6: One line of shared documentation: “use the input record for named arguments.”
- C7: Records create the same ordinary endpoint, retaining all exits and custom formats.
- C8: No I/O or initializer evaluation before the same owner-entry point; never evaluate defaults twice.
- C9: Require old-call compatibility, named-field inference, field-order-independent wire equality, move-only bodies, `cfg` and name-collision checks; verify generated `Debug` does not expose sensitive fields (or omit `Debug`).

**New concepts:** One optional generated record and entry method per applicable endpoint/scope; ordinary Rust struct fields.

**Breaking changes:** No intended existing-call break. New generated-name collisions must be diagnosed; adding a required provider parameter changes the record, just as it changes the positional method. Do not later turn optional values into public record fields: that would make additive optional provider inputs break exhaustive literals.

**Why it might be wrong:** It adds a second input surface to a project trying to remove surface, and does not fix setter inference.
The named route is longer for tiny APIs and has no measured glue saving in these witnesses.

## I3. Give initializer expressions their declared target — the narrow repair

**Fact it expresses:** A declared initial value is a value of its declared type, evaluated once when that owner is created.

**Mechanism:** Change only generated default functions from `fn default() -> T { IntoValue::into_value(expr) }` to `fn default() -> T { expr }`. Calls, setters, config fields and `maybe_*` retain their current contracts. Initializers are authored Rust with a target; there is no need for the default function to erase that target through a generic conversion.

**Declaration (author writes), before → after:** Renamed, `examples/renamed/src/lib.rs:29–31`:

```rust
// Before
pub client Headers(
    header("accept") accept: Native = &ACCEPT,
    header("x-tag") each tags?: Vec<concord4::reqwest::header::HeaderValue>,
) { /* existing body */ }
// After
pub client Headers(
    header("accept") accept: Native = ACCEPT.clone(),
    header("x-tag") each tags?: Vec<concord4::reqwest::header::HeaderValue>,
) { /* existing body */ }
```

The `Renamed` numeric default (`src/lib.rs:16`) remains `version: u32 = 4`. An alternative author expression `= Default::default()` would now infer `u32`; that is a proposed capability, not a claim about current source.

**Call site (caller writes), before → after:** Renamed `src/lib.rs:63–64`, unchanged:

```rust
// Before
let api = Headers::new(concord4::reqwest::Client::new());
let p = api.note("hi").prepare().unwrap();
// After
let api = Headers::new(concord4::reqwest::Client::new());
let p = api.note("hi").prepare().unwrap();
```

`api.clone().set_accept(&ACCEPT)` also remains legal. Frankfurter's inline date parse still requires `::<Date>`: this design intentionally fixes declaration-expression context only.

**SDK code that disappears:** **0 SDK glue lines**. Renamed `src/lib.rs:30` is replaced, not deleted. `crates/concord-macros/src/emit.rs:791` loses one generic-conversion expression; existing default docs about borrowed conversion become shorter. This is smaller than changing the entire caller API, but cannot satisfy a claim to remove most SDK glue.

**Hardest counterexample:** Renamed's borrowed `Native` initializer needs `.clone()` even through its alias; header setters keep accepting borrowed values and retain bytes/sensitivity. Borrowed initializers for `type Name = String` similarly become owned expressions. Generic `V: IntoValue<u16>` callers are unaffected.

**Simple control (Nager):** No declaration or call changes; the control endpoint has no initializer. PokéAPI and Frankfurter declarations also remain unchanged.

**Charter check:**

- C1: No new declaration concept; an initial value remains an existing provider default.
- C2: The Rust initializer expression is target-typed without adding DSL behavior.
- C3: All three simple controls stay unchanged.
- C4: Every initializer obeys the same rule, including aliases, macros and qualified types.
- C5: Use the expression's authored span; a wrong initializer must not report a hidden default-function name as its main location.
- C6: “Initializers have the declared type” replaces a special conversion rule.
- C7: All escape paths and formats remain unchanged.
- C8: Preserve once-per-owner evaluation, no capture and snapshot semantics.
- C9: Require target-inference passes, explicit-clone header wire/sensitivity tests, no-capture failures and the existing initializer-count/forwarded-fragment regressions.

**New concepts:** None; one fewer conversion boundary.

**Breaking changes:** Defaults relying on `&str`/`&String` → `String` or `&HeaderValue` → owned-header conversion must use explicit `.into()`/`.clone()`. No caller-signature change.

**Why it might be wrong:** It leaves the documented Frankfurter problem untouched and makes initializers differ from setters.
The compatibility cost may exceed the very small implementation simplification.

## I4. Exact bodies, converting navigation values

**Fact it expresses:** The request body is a value consumed by the declared request format; path/query/header inputs remain navigation values.

**Mechanism:** Use exact `T` for required body arguments and body `set_*` methods, retaining the current `IntoValue` contract for nonbody values and their defaults. Select by the already visible `body` placement, never by spelling `String`, guessing a domain type, or maintaining an alias list. This is a placement-based hybrid, with a narrower compatibility cost than I1.

**Declaration (author writes), before → after:** Renamed, `examples/renamed/src/lib.rs:33–37`, unchanged:

```rust
// Before
base "http://renamed.invalid";
request Text;
response Text;
GET ping() -> String { path "ping"; }
POST note(body note: String) { path "note"; }
// After: identical declaration
base "http://renamed.invalid";
request Text;
response Text;
GET ping() -> String { path "ping"; }
POST note(body note: String) { path "note"; }
```

**Call site (caller writes), before → after:** Renamed `src/lib.rs:63–64`:

```rust
// Before
api.note("hi");
// After
api.note("hi".into());
// Newly inferable body expressions:
api.note(Default::default());
```

Binary `api.upload(vec![1, 2])` (`examples/renamed/src/lib.rs:58`) stays unchanged. Structured body defaults gain contextual inference without changing the body model or encoder.

**SDK code that disappears:** **0 measured SDK lines**. Renamed `src/lib.rs:37` stays; test `src/lib.rs:64` changes, not disappears. Remove body-only conversion code/bounds from `crates/concord-macros/src/emit.rs:810–814,928–946,2524` (about 5–8 affected generated-code expressions). Keep `IntoValue` for navigation and defaults. Cross-family benefit is cleaner ownership for G's validated body types and custom formats, with no claim that provider validation or forwarding behavior vanishes automatically.

**Hardest counterexample:** String aliases used in path/query/header values keep accepting `&str`; the same alias as a body requires an owned value uniformly. Borrowed native headers and the generic `IntoValue<u16>` forwarder are unchanged. A body declared as an actual reference type still accepts that exact reference; Concord must not add a blanket owned/`Clone` restriction. Frankfurter's query `Date` still needs an annotation.

**Simple control (Nager):** Declaration and caller syntax unchanged. PokéAPI and Frankfurter have no request bodies here, so both are unchanged too.

**Charter check:**

- C1: The existing `body` fact determines the ownership boundary; no generated-surface switch.
- C2: Formats consume the declared body value; validation and conversion remain Rust.
- C3: All three controls are entirely unchanged.
- C4: Placement visibly determines the rule; aliases never change it.
- C5: Body mismatch errors name the authored body type at the call; nonbody inference diagnostics retain today's limitations.
- C6: Two short rules, tied to body versus navigation; no type-name exception table.
- C7: Open `Encode<T>`, move-only/streaming bodies and all native exits remain available.
- C8: No execution or replay change; exact ownership does not grant replay permission.
- C9: Require inferred structured defaults, reference bodies, optional-body absence versus encoded `None`, move-only and stream bodies, plus all unchanged nonbody counterexamples.

**New concepts:** No new keyword, trait or type; exact body supply versus converting nonbody supply.

**Breaking changes:** Borrowed text supplied to owned text bodies and generic `IntoValue<T>` body forwarders need explicit conversion. Body function-item signatures change. Navigation-value generic forwarders do not.

**Why it might be wrong:** Body versus navigation is an ownership distinction Concord can explain, but callers may experience it as inconsistency.
It does not address the actual date-query witness, so it cannot replace I1 or I2 for that problem.

## N1. Retire the extra codec facade as a maintained choice

**Fact it expresses:** A declared format names an ordinary Rust codec type; the legacy and canonical codec paths identify the same types.

**Mechanism:** Make `concord` and `concord-codecs` the supported entry points. Freeze the already-published `concord-extras` facade at a compatible release, migrate the two compatibility consumers to canonical imports in the next major tree, and then remove the facade's workspace package. Do not remove the distinct `concord` Cargo feature named `extras`: Renamed actively uses that feature. No codecs move into macro/runtime privilege.

**Declaration (author writes), before → after:** Renamed, `examples/renamed/src/lib.rs:19–23`, unchanged:

```rust
// Before
PUT upload(body data: Vec<u8>) -> Vec<u8> {
    request Binary;
    response concord4::Binary;
    path "upload";
}
// After: identical declaration
PUT upload(body data: Vec<u8>) -> Vec<u8> {
    request Binary;
    response concord4::Binary;
    path "upload";
}
```

**Call site (caller writes), before → after:** Actual library consumer at `checks/consumers/extras-legacy/src/lib.rs:3–11`:

```rust
// Before
use concord_extras::{Json, csv::{CsvOptions, CsvReader, HeaderMode}, stream::ReadError};
let opts = concord_codecs::csv::CsvOptions::new(HeaderMode::Present, b',');
// After
use concord_codecs::{Json, csv::{CsvOptions, CsvReader, HeaderMode}, stream::ReadError};
let opts = CsvOptions::new(HeaderMode::Present, b',');
```

Renamed callers still write `api.upload(vec![1, 2]).prepare()?`.

**SDK code that disappears:** **0 SDK Rust or manifest lines**: none of the 55 SDKs depends on the package. Library surface removed: `crates/concord-extras/src/lib.rs:1–6`, `Cargo.toml:1–29`, `README.md:1–8`, plus its workspace membership (about 43 physical lines, excluding licenses). Two consumer dependency/import lines change rather than disappear; retain their useful codec assertions under canonical imports. The old-path equality check itself retires with that support promise. This is packaging/doc cleanup, not a glue-score win.

**Hardest counterexample:** The two compatibility consumers are real users and prove type identity. Migrate them deliberately; do not call the package unused. Published users can keep their pinned old facade until a planned major upgrade. Renamed's `features = ["extras"]` at `examples/renamed/Cargo.toml:13` is **not** a dependency on `concord-extras` and must keep working.

**Simple control (Nager):** Declaration and calls unchanged; it imports `concord::Json`. PokéAPI and Frankfurter do too.

**Charter check:**

- C1: No new DSL words; format facts remain unchanged.
- C2: Codec implementations remain ordinary Rust and user-extensible.
- C3: All simple control declarations and dependency needs stay unchanged.
- C4: Canonical import choices are explicit; no magic remapping of paths.
- C5: Migration guidance names the removed package and replacement, not generated internals.
- C6: One fewer package choice to explain.
- C7: Custom codec crates remain equal peers; core-only consumers need not depend on the facade.
- C8: Pure re-export removal has no request/runtime behavior.
- C9: Run the canonical codec consumer matrix with defaults disabled and isolated features; preserve Renamed's dependency-renaming check and legacy type-identity evidence until the support cutoff.

**New concepts:** None; remove one compatibility package from the supported current surface.

**Breaking changes:** Next-major Cargo/import changes for legacy-facade consumers; no SDK declaration or wire change. Do not yank old releases or silently remove a still-supported compatibility promise.

**Why it might be wrong:** A six-line facade may cost less to retain indefinitely than the migration costs users.
Deleting a package from the workspace does not reduce any current SDK glue.

## N2. Remove the ambiguous core error `meta()` method

**Fact it expresses:** Response-head availability and unread-body recoverability are distinct facts about a failed HTTP attempt.

**Mechanism:** Retain `Error::head()` and `head_meta()` for head inspection; retain `response()` and `into_response()` for unread recovery. Remove only core `Error::meta()` in a major release after a real deprecation period. Do not rename it to `head_meta()` while preserving an old “no recoverable body” assertion: their semantics differ. Do not remove SDK-owned `meta()` accessors or `Received::meta()`.

**Declaration (author writes), before → after:** Nager, `examples/nager-holidays-sdk/src/lib.rs:23–25,35–37`, unchanged:

```rust
// Before
base "https://nagerholidays.com";
send KeepHead;
format Json;
GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
    path "api/v4/Holidays/{country_code}/{year}";
}
// After: identical declaration
base "https://nagerholidays.com";
send KeepHead;
format Json;
GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
    path "api/v4/Holidays/{country_code}/{year}";
}
```

**Call site (caller writes), before → after:** Real core call at `checks/phase-a/tests/received.rs:535`:

```rust
// Before: asking for owned head metadata
let meta = error.meta().expect("a status error keeps its response");
// After
let meta = error.head_meta().expect("a response head arrived");
```

For the distinct contract at `checks/core-concepts/tests/x9_metadata_terminal.rs:69–70,204`:

```rust
// Before: asking whether an unread response remains
assert!(error.meta().is_none());
// After
assert!(error.response().is_none());
```

**SDK code that disappears:** **0 SDK lines**. Nager already uses `source.head_meta()` at `examples/nager-holidays-sdk/src/error.rs:71`; its public `Error::meta()` at `:51–54` stays. Remove `crates/concord-core/src/error.rs:268–276` (9 physical lines, 4 implementation lines), and shorten the cheat-sheet row listing the three unread-response methods. Core check call sites migrate; all matching SDK `meta()` receiver types inspected were SDK error/metadata types or `Received`, not this core method.

**Hardest counterexample:** A decode or mid-body failure has a head and no recoverable response. The live checks at `x9_metadata_terminal.rs:55–74,197–204` require that distinction. Nager's metadata access must survive such errors; `head_meta()` does. A downstream SDK still using core `meta()` must choose its intended predicate during migration, rather than getting changed behavior by aliasing the methods.

**Simple control (Nager):** Unchanged declaration, policy, public SDK error and calls. PokéAPI and Frankfurter declarations are unchanged as well.

**Charter check:**

- C1: No declaration switch; this removes runtime API ambiguity.
- C2: Head facts remain typed; SDK classification behavior remains Rust.
- C3: No control gains a line or concept.
- C4: `head*` means head; `response*` means recoverable native response.
- C5: Actual deprecation should say which replacement answers which question. The current method is not already deprecated.
- C6: Eliminate the paragraph explaining why two owned metadata methods disagree.
- C7: Both unread response exits stay; no body is consumed to simplify metadata.
- C8: Pure API removal; no execution or buffering changes.
- C9: Preserve positive/negative head and response cases for Status, Metadata, Decode, Limit, mid-body Transport and pre-head failure; migrate assertions by meaning.

**New concepts:** None; one fewer core method.

**Breaking changes:** Calls/function references to **core** `Error::meta()` only, after deprecation; external intent cannot be inferred by search-and-replace. SDK methods with the same spelling are not automatically affected.

**Why it might be wrong:** The old name still answers a coherent question, and removing it saves almost no code.
Deprecation may generate more migration noise than the permanent four-line implementation costs.

## N3. Remove unused first-page discovery from the independent pager

**Fact it expresses:** A known independent page plan and a server-dependent continuation are different provider traversal contracts; a total count alone does not prove independent addressing.

**Mechanism:** In a major release, remove `first_page_then` and its discovery-only state from `Independent`, retaining `independent`, `sequential`, `Pages` and manifest jobs. No replacement DSL and no automatic “total means parallel pages” convention. The current SDK cohort has no call to migrate. This is a deletion proposal, not a promise that a manually split first fetch is semantically equivalent.

**Declaration (author writes), before → after:** PokéAPI, `examples/pokeapi-sdk/src/lib.rs:84–86`, unchanged:

```rust
// Before
GET pokemon(id: String) -> Pokemon { path "pokemon/{id}/"; }
GET list(query limit?: u64, query offset?: u64) -> PokemonPage { path "pokemon/"; }
// After: identical declaration
GET pokemon(id: String) -> Pokemon { path "pokemon/{id}/"; }
GET list(query limit?: u64, query offset?: u64) -> PokemonPage { path "pokemon/"; }
```

**Call site (caller writes), before → after:** Real SDK caller path remains:

```rust
// Before
let first = api.list().set_limit(20).send().await?;
let next = api.follow(first.value.next.as_deref().unwrap()).await?;
// After: same provider-returned URL, same calls
let first = api.list().set_limit(20).send().await?;
let next = api.follow(first.value.next.as_deref().unwrap()).await?;
```

The only corresponding helper caller to remove is a library test, not an SDK: `crates/concord-pagination/tests/first_page.rs:51` currently constructs `first_page_then(1, options(10, 3), |page| server.call(page), plan)`. After removal there is no supported first-discovery entry point. Keep known-plan callers on `independent(keys, options, fetch)`.

**SDK code that disappears:** **0 SDK lines**. PokéAPI `src/lib.rs:207–228` remains useful URL validation/following behavior; no deletion credit. Library removals: `crates/concord-pagination/src/independent.rs:63–136` (74 physical lines, including helper docs and the `Plan` enum) and `:231–256` (26 physical lines), plus approximately 15–30 lines of first/plan fields, bounds and branches in `:166–184,194–228,258–289,322–324`. Replace the `Plan` wrapper with the existing ready iterator; do not delete whole shared ranges. Estimated net library reduction: roughly 100–120 physical lines after replacement. Retire helper-specific tests/docs; those are not SDK-glue savings.

**Hardest counterexample:** A real downstream user may depend on the helper's single budget, first-page-once emission, validation-before-emission, checkpoint and cancellation contract, all tested in `tests/first_page.rs`. Fetching a first page by hand and then calling `independent` does **not** automatically preserve those guarantees. If a current supported SDK user is found, reject this removal or first retain an explicit Rust adapter with the complete contract. Keep `Outcome::DiscoveryFailed`: manifest jobs still use it (`src/jobs.rs:312`).

**Simple control (Nager):** No pagination dependency or declaration/call change. PokéAPI and Frankfurter declarations do not grow; PokéAPI continues following links as returned, including server-chosen limits.

**Charter check:**

- C1: No library-algorithm selection statement enters the DSL.
- C2: Traversal stays explicit Rust outside endpoint facts.
- C3: All three control declarations remain unchanged.
- C4: Never infer independent addressing or a snapshot from a total count.
- C5: A removed import gets a targeted migration note; no fictitious equivalent is recommended.
- C6: One fewer traversal entry point and discovery-specific generic parameter.
- C7: Native requests and low-level remaining traversal engines remain usable.
- C8: No new actors; preserve laziness and bounds of surviving engines.
- C9: Keep independent/sequential/job bound, cancellation and resume tests; review helpers in diagnostics/resume tests rather than deleting those suites wholesale. No experiment is needed to prove zero in-tree SDK uses; semantic review is still required.

**New concepts:** None; remove one entry point, its planning closure and one engine state machine branch.

**Breaking changes:** `first_page_then` imports/calls and explicitly named discovery `Independent<..., P>` types. In-tree library tests/doctests change; no identified SDK call changes. Published downstream usage remains unknown.

**Why it might be wrong:** It deletes a well-tested guarantee merely because the current gallery does not exercise it.
Future SDKs could recreate a larger and less correct discovery adapter, making this a false economy.

## N4. Make operation identity unconditional; remove `operations Name;`

**Fact it expresses:** Every declared endpoint has an identity in the client's operation tree, regardless of whether its current sender uses that identity.

**Mechanism:** Always generate `<Client>Operation` using the current deterministic variant/`cfg` rules. Remove the opt-in statement. Generated policy calls always use `SendOperation<T, ClientOperation>`. Keep existing simple `SendWith<T>` policies working through one core blanket adapter that discards the operation and calls their existing implementation; use a non-async forwarding method so it introduces no unnecessary `Self: Sync` or `O: Send` bound. Existing operation-aware policies keep their explicit implementations. This removes the macro's dispatch mode, not provider classification behavior.

**Declaration (author writes), before → after:** Sentry, `examples/sentry-sdk/src/lib.rs:41–48`:

```rust
// Before
base "https://sentry.io";
path "api/0";
format Json;
limit BODY_LIMIT;
send policy -> Result<Response<_>, Error>;
/// Every Sentry operation, as the send policy classifies it (admission bucket, writes,
/// paging).
operations SentryOperation;
// After
base "https://sentry.io";
path "api/0";
format Json;
limit BODY_LIMIT;
send policy -> Result<Response<_>, Error>;
```

The operation enum keeps its current `SentryOperation` name; classification documentation belongs on the Rust classifier. Do not add retry/read/write defaults as part of this removal.

**Call site (caller writes), before → after:**

```rust
// Before
api.organization("acme").issue("42").get().send().await?;
// After: unchanged
api.organization("acme").issue("42").get().send().await?;
```

Sentry's `impl<T: Send> SendOperation<T, SentryOperation> for SendPolicy` (`src/policy.rs:124`) and its exhaustive match stay unchanged. Nager's `impl SendWith<Vec<Holiday>> for KeepHead` (`src/lib.rs:62`) also stays unchanged through the adapter. Square's real forwarding implementation migrates by deletion:

```rust
// Before: examples/square-payments-sdk/src/lib.rs:123–133
impl<T: Send> SendOperation<T, SquareOperation> for SquarePolicy {
    type Output = <Self as SendWith<T>>::Output;
    fn send<E: Endpoint<T> + Send>(
        &self,
        _operation: SquareOperation,
        endpoint: E,
    ) -> impl Future<Output = Self::Output> + Send {
        SendWith::send(self, endpoint)
    }
}
// After: this impl is deleted; the core blanket adapter forwards to the
// unchanged SendWith<T> implementation at src/lib.rs:105–118.
```

**SDK code that disappears:** Eight opt-in declaration statements: `examples/github-sdk/src/api.rs:68`, `datadog-events-sdk/src/api.rs:39`, `sentry-sdk/src/lib.rs:48`, `kraken-spot-rest-sdk/src/lib.rs:39`, `zendesk-support-sdk/src/lib.rs:60`, `paypal-sdk/src/lib.rs:73`, `strava-sdk/src/lib.rs:83`, `square-payments-sdk/src/lib.rs:59`. Also delete Square's pure adapter at `examples/square-payments-sdk/src/lib.rs:120–133` (**14 physical / 13 nonblank lines**, including its three doc lines); the single-attempt behavior at `:105–118` stays. Attached ceremony-only comments elsewhere can shrink (for example Sentry `:46–47`, Strava `:81–82`, Datadog `:37–38`); keep comments containing provider facts near their classifiers. **8 definite declaration lines plus the concrete Square forwarding block; 0 provider-classification-table lines removed.** Remove dispatch-mode branches from `crates/concord-macros/src/emit.rs:2112–2124,2441–2449` and the “no enum” early return at `:49–53`, balanced against a small blanket adapter. This crosses families E/G/H/L: less opt-in syntax, a forwarding implementation gone and less composition documentation, while exposing facts uniformly to Rust capability policies. These physical counts are not a claim that every line in the block has the classifier's forwarding label.

**Hardest counterexample:** **Square is an actual dual-trait user**, not a hypothetical coherence problem. Its explicit `SendOperation` implementation overlaps the blanket; deleting its forwarding block is a required migration. That is safe here because the implementation discards the operation and does nothing except `SendWith::send`. A downstream dual implementation with different operation-aware behavior cannot be deleted: retain its explicit `SendOperation`, move its operation-agnostic implementation to a separate wrapper/helper, and enumerate that API break. Existing Sentry/Strava operation-specific policies must retain exact enum names, exhaustive matches, returned types, admission lifetime and replay behavior. All eight current enum names match the proposed `<Client>Operation` convention. New generated type-name collisions need a declaration-site error; do not make callers discover it in expansion internals.

**Simple control (Nager):** No authored declaration, policy implementation or caller change. Its new enum may appear in rustdoc, but it must not require an `operations` line, adapter import or new generic bound. PokéAPI's `KeepHead` and Frankfurter's `SendPolicy` likewise remain valid without edits.

**Charter check:**

- C1: Delete a statement that requests generated machinery; endpoint identities already follow from provider operations.
- C2: Typed identity is available everywhere; retry/limiting/auth behavior and read/write classification remain Rust.
- C3: All three controls keep their authored shape, imports and policy implementations.
- C4: One visible naming convention: `<Client>Operation`; variants use the existing endpoint-path rule.
- C5: Update sender diagnostics for blanket-adapted policies to name the authored policy/output, not demand users implement an adapter they did not request.
- C6: Delete the explanation of which statement switches between sender traits.
- C7: Keep `Endpoint`, `Sends<O>`, custom policies and all request/native exits; test the same concrete `send` outputs.
- C8: Enum generation is compile-time only; it grants no replay permission or hidden behavior.
- C9: Require both simple and operation-aware policy compilations, `cfg` variants, exhaustive-match failures on an added endpoint, custom names/collisions, renamed dependencies and unchanged request bytes. Compile Square after removing only its forwarding impl and prove its error/result/wire behavior remains the same; retain a dual-trait overlap fixture for downstream diagnostics.

**New concepts:** No new DSL concept; an always-generated enum and an internal blanket adaptation rule. Both existing public sender traits remain during this design.

**Breaking changes:** Removal of `operations Name;`; clients using nonconventional names need a Rust alias/migration. Square's explicit adapter must be removed; downstream dual-trait implementations need individual migration. New generated-name collisions and possible generic trait-selection changes. The eight current SDK enum names need no rename.

**Why it might be wrong:** A blanket implementation spends downstream coherence freedom to save eight declarations, Square's adapter and a compiler mode.
Always generating enums adds public surface to tiny APIs even when they never use operation classification.

## N5. Stop storing a second copy of an error head — smaller than you think

**Fact it expresses:** Nager's failure metadata is exactly the HTTP head Concord already retains; it has no SDK-specific metadata fields.

**Mechanism:** An SDK-local Rust simplification requiring **no Concord feature**: retain only the original `concord::Error`, compute status from `source.head()`, and return an owned metadata snapshot from `source.head_meta()` when asked. This removes all mirrored-head storage and eager copying in the Nager error wrapper. Keep its provider classification and native recovery methods. The ownership change to its public accessor must be accepted explicitly; do not disguise it as source-compatible.

**Declaration (author writes), before → after:** Nager `examples/nager-holidays-sdk/src/lib.rs:24,35–37`, unchanged:

```rust
// Before
send KeepHead;
GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
    path "api/v4/Holidays/{country_code}/{year}";
}
// After: identical declaration
send KeepHead;
GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
    path "api/v4/Holidays/{country_code}/{year}";
}
```

Actual SDK Rust before (`examples/nager-holidays-sdk/src/error.rs:29–32,46–54,69–73`) → proposed after:

```rust
// Before
pub struct Error {
    source: concord::Error,
    meta: Option<Box<ResponseMeta>>,
}
// status(): self.meta.as_ref().map(|meta| meta.status())
// meta() -> Option<&ResponseMeta>: self.meta.as_deref()
// from(source): let meta = source.head_meta().map(Box::new); Self { source, meta }

// After
pub struct Error {
    source: concord::Error,
}
impl Error {
    pub fn status(&self) -> Option<StatusCode> {
        self.source.head().map(|head| head.status())
    }
    pub fn meta(&self) -> Option<ResponseMeta> {
        self.source.head_meta()
    }
}
impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        Self { source }
    }
}
```

Other existing methods and their documentation stay; the after fragment shows only changed pieces. `head_meta()` works even when the original body has already failed, unlike the narrower core `meta()` discussed in N2.

**Call site (caller writes), before → after:**

```rust
// Before: retain a borrowed metadata view
let meta: Option<&ResponseMeta> = error.meta();
// After: retain an independent owned snapshot
let meta: Option<ResponseMeta> = error.meta();
```

`error.status()`, `error.meta().unwrap().headers()["x-request-id"]` and `error.into_concord().into_response()` keep their intended meanings. Borrowing the new owned snapshot requires holding it in a local first.

**SDK code that disappears:** `examples/nager-holidays-sdk/src/error.rs:31` (duplicated field) and `:71` (eager head clone/box) disappear; `:48,:52–53,:72` are replacements, not deleted ranges. **2 definite removed SDK lines**, no new type or DSL line, and all of this wrapper's mirrored metadata state gone. The classifier counts these inside the 66 error-glue lines, but this proposal does not claim all 66 disappear. This mechanism also applies to A/B wrappers whose metadata is exactly core metadata and can reduce duplicate redaction/storage work relevant to J. PokéAPI is a counterexample: its metadata includes final URL (`src/lib.rs:92–98,274–286`), which core head metadata does not retain.

**Hardest counterexample:** A SDK or caller requiring `Option<&ResponseMeta>` cannot keep that API by forwarding to an allocating `head_meta()` call. Keep the cached representation for that compatibility promise, or choose a major accessor change. Do not apply this blindly to PokéAPI's final URL, provider-error snapshots that outlive a consumed source, parsed quotas, retry reports or an SDK's richer metadata. No auth/retry/ratelimit state is removed.

**Simple control (Nager):** Its declaration, success `HolidayResponse`, `KeepHead`, construction and endpoint calls remain unchanged; only the error metadata ownership signature changes. PokéAPI and Frankfurter declarations remain unchanged, and their richer/error-specific metadata is not force-migrated.

**Charter check:**

- C1: No declaration statement is needed for a duplicate-storage cleanup.
- C2: Provider error classification remains Rust; already typed HTTP facts are read from core once.
- C3: Simple declarations and happy-path callers do not grow; the error-accessor ownership break is explicit.
- C4: Metadata is exactly what `source.head_meta()` returns, with no fallback or fabricated status.
- C5: Ownership mismatch errors name public Rust types and the changed SDK accessor.
- C6: One storage location and one metadata source; no composition explanation added.
- C7: The original error and unread native response recovery remain intact.
- C8: No new actor or I/O; move the optional header clone from error creation to explicit owned-metadata inspection.
- C9: Check Status/Metadata unread bodies, Decode/Limit/mid-body head retention, pre-head absence and canary redaction through the new accessor. Verify repeated metadata inspection is understood to make repeated snapshots.

**New concepts:** None. An owned return type already provided by Concord replaces an SDK cache.

**Breaking changes:** Nager `Error::meta()` returns `Option<ResponseMeta>` instead of `Option<&ResponseMeta>`; reference-returning downstream helpers and explicit type annotations migrate. Repeated calls now clone repeatedly; cheap status inspection should use `status()`/`head()` instead.

**Why it might be wrong:** Two lines and an allocation do not necessarily justify a public ownership break.
Returning a borrowed view with the full current API would need a different shared core representation, which is a larger design.

## Selection and limits

For I, compare I1 against I2 on the real Frankfurter inline-date task, the three required adversarial cases and the unchanged control declarations. I1 has the smallest complete mechanism and the widest break list; I2 preserves current convenience but adds surface. I3 and I4 are bounded alternatives with honestly narrower benefits. Do not ship a syntactic “bare `String` is special, aliases are exact” shortcut: it abandons the alias guarantee and still leaves String `.into()` inference generic. Diagnostics-only work is already substantially present and cannot be credited as an inference solution.

For N, N5 is the smallest actual SDK-state deletion, N2 the smallest core API deletion, N1 mostly packaging, and N3 a decision to stop supporting a presently unused traversal shape. N4 offers the strongest cross-family simplification but needs coherence review. Keep auth, retry and ratelimit as explicit optional Rust libraries: moving their loops or lifecycle into the declaration would violate C2/C8 and would not solve the reverted construction problem. In particular, no idea here removes client setters or `with_base_url` to conceal a host/credential invariant; that remains family C's provider-fact design problem.

These proposals are grounded in source/usage inspection, not executed candidate implementations. Required C9 tests are specified per design; no new performance, compatibility or no-leak guarantee is asserted as already tested. Only this output file was written; no repository changes or other idea/design-lane outputs were used.
