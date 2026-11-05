# Stripe SDK maintenance exercise (DX-01, `DATA_AND_VALIDATION.md` §6)

## Task statement

Staged SDK evolution (not a claim that Stripe changed): **add an optional nested form field and a
typed expansion path to the Stripe slice** (`examples/stripe-sdk`). Evidence to retain:
omitted-field wire compatibility (the frozen consumer's request bytes must stay identical),
changed/requested form bytes, validation errors, and whether callers must learn internal encoding
details. Procedure: `v4/extensions-real-apis/MAINTENANCE_BRIEF.md` (three commits: frozen
consumer, SDK change, updated consumer).

Performed by a maintainer who did not write the SDK, reading only its rustdoc, `PROVIDER.md`,
the briefs and Stripe's public documentation. The SDK's `FRICTION.md` was not read before the
stage-3 commit; any comparison with it is added afterwards in a separate, labelled section.
Same model family as the SDK author (a Claude agent), so the independence is limited to "did not
write it and did not read its author's friction notes".

## Commits and commands

| Stage | Commit | Content |
|---|---|---|
| 1 frozen consumer | `6a7ed00521b93129e0cb879accbdbf2347ba91eb` | `src/before.rs`, `tests/before.rs`, `Cargo.lock` |
| 2 SDK change | `f90923e4b310472477165f017c9e67e65e606b3e` | `examples/stripe-sdk/{src,tests,PROVIDER.md}` |
| 3 updated consumer | `ca6c7b0cda6914a9c8104903fd6edb0853189793` | `src/after.rs`, `tests/after.rs`, this file |

Executable check: `cargo test --locked -p concord-check-maintenance-stripe` — at stage 1 runs
`tests/before.rs` (3 tests); at stage 2 the same 3 unchanged tests against the changed SDK; at
stage 3 `tests/before.rs` (3) and `tests/after.rs` (4). SDK and app at stage 2 and 3:
`cargo test --locked -p concord-example-stripe-sdk -p concord-example-stripe-app`. All runs with
Rust 1.99.1, `CARGO_BUILD_JOBS=6`, plus `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` on the three packages. Results are in the table at the end.

Baseline observation (before stage 1): `cargo test --locked` refused to run at the base commit
`e7bfd29` — "cannot update the lock file ... because --locked was passed" — because the commit
that added the five `checks/maintenance-*` stub packages (`a5d5b13`) did not add them to
`Cargo.lock`. Resolved offline (`cargo metadata --offline`), which added exactly those five
entries; the stage-1 commit carries them plus this package's dependencies.

## Stage 1 — frozen initial consumer

`src/before.rs` is a CRM-mirroring application: `mirror_contact` (`POST /v1/customers`),
`correct_contact` (`POST /v1/customers/{id}`), `fetch_profile` (`GET /v1/customers/{id}`),
`find_by_email` (`GET /v1/customers?email=..&limit=..`), each through the SDK's `send()` so the
`Request-Id` and `Idempotent-Replayed` are kept. Bodies are filled the way `params.rs` documents
("Fields are public so that an application can fill the parts it needs"): struct literals with
`..CustomerCreate::default()` / `..CustomerUpdate::default()`.

`tests/before.rs` runs it against a loopback fake with two connected accounts (`acct_ALPHA`,
`acct_BETA`) and asserts the exact form bodies as literal byte strings, the decoded pairs, the
target, `Stripe-Account`, `Stripe-Version`, `Idempotency-Key`, `Content-Type`, and how a Stripe
parameter error reaches the caller. The response fixtures contain every member of the documented
customer object, including `invoice_settings` with a `default_payment_method` id, which the SDK
did not model at stage 1.

Frozen bytes (`tests/before.rs`):

```text
POST /v1/customers          name=Ada+Lovelace&email=ada%40example.test&address%5Bcity%5D=London&address%5Bcountry%5D=GB&preferred_locales%5B%5D=en-GB&metadata%5Bcrm_id%5D=crm-0001
POST /v1/customers          name=Grace+Hopper&email=grace%2Bbilling%40example.test&address%5Bcountry%5D=US&metadata%5Bcrm_id%5D=crm-0002
POST /v1/customers/{id}     name=Ada+King&phone=&metadata%5Bimport_batch%5D=&metadata%5Blegacy_ref%5D=
GET  /v1/customers/cus_ALPHA0001
GET  /v1/customers?email=grace%2Bbilling%40example.test&limit=5
```

Stage 1 compiled at the first attempt; no compiler errors, no annotations beyond the ordinary.

## Stage 2 — SDK change

### What changed

Provider facts first (Stripe reference for `2026-10-28.endive`, retrieved 2026-11-05; recorded in
`examples/stripe-sdk/PROVIDER.md`): customer create and update take `invoice_settings` with
`default_payment_method` ("ID of a payment method that's attached to the customer, to be used as
the customer's default payment method for subscriptions and invoices"); the customer object's
`invoice_settings.default_payment_method` is "nullable, expandable"; the legacy-cards migration
guide shows the curl spelling `-d "invoice_settings[default_payment_method]={{PAYMENTMETHOD_ID}}"`
and maps defaults to `pm_xxx`.

- `params::InvoiceSettingsParams { default_payment_method: Option<PaymentMethodId> }` (derives
  `Default`), and `invoice_settings: Option<InvoiceSettingsParams>` on `CustomerCreate` and
  `CustomerUpdate`, declared after `preferred_locales` and before `metadata`.
- `ids::PaymentMethodId` (prefix `pm_` when constructed by the application; received ids keep the
  SDK's no-prefix rule).
- `models::PaymentMethod` (`id`, `type`, `card {brand, last4, exp_month, exp_year}`, expandable
  `customer`, nullable `metadata`), `models::CustomerInvoiceSettings`, `Customer::invoice_settings`
  (`#[serde(default)]`), and the accessor `Customer::default_payment_method() ->
  Option<&Expandable<PaymentMethod>>`.
- Re-exports `PaymentMethodId`, `PaymentMethod`; two `expand[]` doc comments in the declaration.
- SDK tests: `tests/form.rs::invoice_settings_is_a_nested_optional_field` (pairs and bytes on
  create/update, omitted == empty, prefix refusal); `tests/wire.rs::customer_default_payment_method_in_id_object_and_null_forms`
  (retrieve plain/expanded/null, list with `data.`).

Two decisions the documentation forced: (1) no `Patch` for `default_payment_method` — the
reference documents an empty value only for `invoice_settings.custom_fields` ("When updating,
pass an empty string to remove previously-defined fields"), so the SDK does not claim
`invoice_settings[default_payment_method]=` unsets it; (2) `pm_` required on construction —
the migration guide says Card or Source ids "can no longer" be passed as `payment_method`
(read in the French rendering Stripe served).

### Diff of stage 2

`git diff --stat 6a7ed00 f90923e`:

```text
 checks/maintenance-stripe/MAINTENANCE.md | 10 ++++
 examples/stripe-sdk/PROVIDER.md          | 12 +++++
 examples/stripe-sdk/src/ids.rs           |  6 +++
 examples/stripe-sdk/src/lib.rs           | 13 +++--
 examples/stripe-sdk/src/models.rs        | 73 +++++++++++++++++++++++++-
 examples/stripe-sdk/src/params.rs        | 21 ++++++--
 examples/stripe-sdk/tests/form.rs        | 50 ++++++++++++++++++
 examples/stripe-sdk/tests/wire.rs        | 89 ++++++++++++++++++++++++++++++++
 8 files changed, 264 insertions(+), 10 deletions(-)
```

By kind (added non-blank lines, comments counted separately):

| Kind | Code | Doc comments |
|---|---|---|
| `concord::api!` declaration | 0 | 3 (`expand[]` examples) |
| Re-exports (rustfmt rewrapped two `pub use` lists) | +5 / −4 | 0 |
| Id type (`stripe_id!` + `public_debug!`) | 4 | 2 |
| Request params (struct + two fields + import) | 7 / −1 | 10 |
| Response models (`PaymentMethod`, card, settings, `Object` impl, field; −2 = rewrapped import) | 37 / −2 | 20 |
| Wrapper (`Customer::default_payment_method`, with its `impl` block) | 5 | 4 |
| Encoder (`form.rs`), expandable, context, `send()`, pagination | 0 | 0 |
| SDK tests | 122 | 7 |

### Old call sites

- `src/before.rs` and `tests/before.rs` compiled **unchanged** and all 3 tests passed against the
  changed SDK, including the literal byte comparisons: omitting the new field leaves the bytes
  identical, and the frozen reader decodes responses that now carry a modelled
  `invoice_settings`.
- The app (`examples/stripe-app`) needed **no change**: it fills `CustomerCreate` /
  `CustomerUpdate` with `..Default::default()`; its 7 integration tests and doctest passed.
- The SDK's existing tests passed unchanged.

**Compatibility break for a style the SDK does not document but allows.** All fields are `pub`
and the structs are not `#[non_exhaustive]`, so a caller that names every field breaks. Probe
(a temporary example in this package, not committed) — compiled against the stage-1 SDK (`Finished`),
and against the stage-2 SDK:

```text
error[E0063]: missing field `invoice_settings` in initializer of `CustomerCreate`
 --> checks/maintenance-stripe/examples/compat_probe.rs:6:5
  |
6 |     CustomerCreate {
  |     ^^^^^^^^^^^^^^ missing `invoice_settings`

error[E0027]: pattern does not mention field `invoice_settings`
  --> checks/maintenance-stripe/examples/compat_probe.rs:19:9
   |
19 |       let Customer {
   ...
32 | |     } = customer;
   | |_____^ missing field `invoice_settings`
   |
help: if you don't care about this missing field, you can explicitly ignore it
   ...
31 +         livemode: _, invoice_settings: _ } = customer;
help: or always ignore missing fields here
   ...
31 +         livemode: _, .. } = customer;
```

Both errors point at the right cause and name the new field; the pattern error even proposes
`..`. The literal error has no such hint (the fix is `..CustomerCreate::default()`). Adding
`#[non_exhaustive]` would not help: it forbids struct expressions from other crates entirely,
including `..Default::default()`, so it would break the app and the frozen consumer. The SDK's
"public fields + `Default`" style makes field additions compatible for the documented spelling
and source-breaking for exhaustive literals and patterns; `SubscriptionCreate`, `ProductCreate`
and `PriceCreate` (no `Default`, constructor + mutation) behave the same for literal callers.

## Stage 3 — updated consumer

`src/after.rs` keeps the frozen module beside it and adds: `mirror_contact_with_default`
(create, optional `invoice_settings[default_payment_method]`), `set_default_payment_method`
(update with the nested field and `expand[]` in the **form body**), `billing_profile` (retrieve
with `expand[]=invoice_settings.default_payment_method`), `billing_profiles_by_email` (list with
`expand[]=data.invoice_settings.default_payment_method`). The default payment method is shown
as `DefaultPayment::{Unset, IdOnly, Card {..}, Other {kind}}`. Compiled at the first attempt.

`tests/after.rs` (4 tests):

- `omitted_field_keeps_frozen_bytes_and_set_field_adds_one_pair` — the frozen
  `before::mirror_contact` and `after::mirror_contact_with_default(.., None, ..)` send the frozen
  literal body, byte for byte, with the same key (so a replay across the SDK upgrade is the
  same request for Stripe's idempotency comparison); with the field set, exactly one pair is
  added in declaration order:

  ```text
  name=Ada+Lovelace&email=ada%40example.test&address%5Bcity%5D=London&address%5Bcountry%5D=GB&preferred_locales%5B%5D=en-GB&invoice_settings%5Bdefault_payment_method%5D=pm_SYNTHGB02&metadata%5Bcrm_id%5D=crm-0001
  ```

- `nested_update_and_typed_expansions_on_two_accounts` — update body
  `invoice_settings%5Bdefault_payment_method%5D=pm_SYNTHGB02&expand%5B%5D=invoice_settings.default_payment_method`;
  retrieve target `/v1/customers/cus_BETA0001?expand%5B%5D=invoice_settings.default_payment_method`
  (a SEPA debit → `Other { kind: "sepa_debit" }`); list target
  `/v1/customers?email=ada%40example.test&limit=5&expand%5B%5D=data.invoice_settings.default_payment_method`
  (a card → `Card { brand, last4, expires }`); `Stripe-Account` per request; the frozen reader
  on the same fake still works.
- `local_validation_of_the_new_values` — see Validation below.
- `stripe_parameter_error_names_the_bracketed_wire_key` — see Validation below.

The fake expands only for the documented spelling of each operation (`data.` on lists) and
reads `expand[]` from the query or the form body, decoding with the test server's own decoder.

## Findings

### Validation errors

| Input | Where refused | Error (verbatim `Display`) |
|---|---|---|
| `PaymentMethodId::new("card_SYNTHlegacy")` | locally, before any body exists | `payment method id must start with \`pm_\`` (`IdErrorReason::Prefix("pm_")`) |
| `PaymentMethodId::new("pm_with space")` | locally | `IdErrorReason::Character` |
| `ExpandPath::new("data.invoice_settings.default_payment_method.customer.invoice_settings")` | locally (5 segments) | `expansion path must have 1-4 non-empty dot-separated segments` |
| `invoice_settings.default_payment_method` on **list** (missing `data.`) | **not refused**: prepares `?expand%5B%5D=invoice_settings.default_payment_method` | — (Stripe decides; not verified live) |
| unknown/unattached payment method | Stripe (`400`, synthetic fake) | `Stripe.customers.update: Stripe returned HTTP 400 (invalid_request_error/resource_missing), request req_SYNTHerr2`; `ApiError::param()` = `invoice_settings[default_payment_method]` |

The `Display`/`Debug` of the SDK error withholds the message and param (the test asserts the
payment method id is not shown). The bracketed `param` spelling in the fake is an inference from
Stripe's form convention: the errors page describes `param` ("the parameter related to the
error") without a nested example. Whether Stripe counts `invoice_settings` as one of its "four
levels" of expansion (the SDK counts dot segments) is also not documented on the pages read.

### Did callers have to learn internal encoding details?

- **Writing the field: no.** The caller sets
  `invoice_settings: Some(InvoiceSettingsParams { default_payment_method: Some(id) })`; the
  generic encoder produces `invoice_settings[default_payment_method]` and its percent-encoding.
  The caller never writes a bracket. An `InvoiceSettingsParams` with nothing set sends nothing,
  the same as `None` (tested), so there is no third "empty object" state to learn.
- **Reading a parameter error: yes.** Stripe reports `param` in wire spelling. To attach the error
  to the Rust field the caller must know that `invoice_settings.default_payment_method` (Rust)
  is `invoice_settings[default_payment_method]` (wire). The SDK offers no mapping; the public
  `form::to_pairs` lets a caller compute the key from a value, which is a workaround, not an API.
- **Requesting the expansion: provider vocabulary, not SDK encoding.** The caller writes the
  dotted path as text and must know Stripe's list rule (`data.` prefix). The SDK validates the
  shape only; nothing ties a path to an operation or to the field it expands, and the consumer
  defines its own constants (`DEFAULT_PAYMENT_METHOD`, `LIST_DEFAULT_PAYMENT_METHOD`) because the
  SDK has none.
- **Reading the expansion: `Expandable<T>` must be matched even when expansion was requested.**
  The type cannot say whether the request asked for it, so `DefaultPayment::IdOnly` exists in the
  consumer (and is what `create` without `expand[]` returns).
- **Byte order: only for byte-level tests.** Pair order follows field declaration order, so the
  new pair lands between `preferred_locales` and `metadata`. Callers never see this; tests that
  compare bytes (and Stripe's idempotency comparison of a replay) do, and it is stable.

### Concepts the caller had to learn

`InvoiceSettingsParams` (a nested struct mirroring Stripe's nesting), `PaymentMethodId`,
`Customer::default_payment_method()`/`Expandable<PaymentMethod>` (id vs object), `PaymentMethod`
(`kind` as text, `card` optional), Stripe's expansion-path vocabulary and the `data.` list rule,
and that `param` in errors is the wire key. Nothing about `StripeForm`, `Patch` (not used for the
new field) or the `api!` declaration.

### Documentation read

| Question | Read | Answered? |
|---|---|---|
| How are bodies filled, what does `None`/`Patch`/empty `Vec` send? | `params.rs` module docs, `form.rs` module table | yes, precisely; predicted the new bytes before running anything |
| Will a nested struct encode without touching the encoder? | `form.rs` table ("nested struct / map → `metadata[order_id]`") | yes |
| How to model an expandable field? | `expandable.rs` docs, existing `Product::default_price`, `Invoice::subscription()` | yes, by example |
| Id prefix rules | `ids.rs` module docs | yes (constructed vs received ids) |
| Where to record provider decisions | `PROVIDER.md` tables | yes |
| Accounts, errors | `context.rs`, `send.rs` docs | yes |
| Does an empty value unset `default_payment_method`? | Stripe customer update reference | **no** (documented only for `custom_fields`) → no `Patch` |
| Are legacy `card_` ids accepted? | migration guide (served in French) | yes: no longer as `payment_method` |
| Does `param` use bracket spelling? Does `invoice_settings` count as an expansion level? | errors page, expanding objects page | **no** |

No Concord `docs/` page was needed: nothing in the change reached Concord's interfaces.

### Compiler errors

None in stages 1–3 for the code that was kept: the SDK change, the frozen consumer, the updated
consumer and the tests compiled at the first attempt, and clippy (`-D warnings`) was clean; rustfmt
rewrapped two re-export lists and some test lines. The only compiler errors are the deliberate
probe above (E0063/E0027), and they point at the right cause.

### Casts, annotations, adapters, native escapes

- No casts, no native `reqwest` use, no handwritten request; every call is a generated operation
  sent through the SDK's `send()`.
- `ExpandPath::new(..)` returns `Result` even for a static string: the consumer writes
  `.expect("a static expansion path is valid")` (no `const` constructor).
- Adapters in the consumer: `default_payment_method.map(|id| InvoiceSettingsParams {
  default_payment_method: Some(id) })` and the `Expandable` match in `DefaultPayment::of`.
- Duplicated code: the consumer's `CustomerCreate` construction appears in `before.rs` and
  `after.rs` because the frozen module may not be edited (an artefact of the procedure, not of
  the SDK). The SDK itself gained no duplicated context or encoding code.
- Hidden collection: none (single-page reads, as in the frozen consumer).

### Metadata not preserved

`PaymentMethod` keeps the card summary only: SEPA, bank-account and other type details and
`billing_details` are dropped (`Other { kind }`), as are `invoice_settings.footer`,
`custom_fields` and `rendering_options` — the SDK's policy of modelling only used fields. The
response type cannot record whether an expansion was requested.

### Counter-evidence (what was easy)

- The generic `StripeForm` encoder handled the new nesting with **zero encoder changes**; the
  omitted-field guarantee came for free from "`None` emits nothing", including for a present but
  empty nested struct.
- `Expandable<T>` + `Object` made a new expansion target a model plus a six-line `Object` impl;
  the declaration already carried `expand[]: Vec<ExpandPath>` on every operation, so the new path
  needed **no declaration change**, and `expand[]` in a POST body worked the same way.
- The documented `..Default::default()` style kept the app and the frozen consumer compiling.
- The SDK's docs (form table, params conventions, `PROVIDER.md` decision tables) answered every
  SDK question; only provider questions needed Stripe's pages.
- Wire assertions with the test server and two contexts were direct to write.

## Proposals (not implemented; SDK-level, no Concord library change indicated)

1. **Typed expansion paths tied to the target** (removes the `data.` rule and the stringly path):

   ```rust
   // today
   .set_expand(vec![ExpandPath::new("data.invoice_settings.default_payment_method")?])
   // proposed (SDK constants with a list form)
   .set_expand(vec![expand::customer::DEFAULT_PAYMENT_METHOD.in_list()])
   ```

   It would still not make `Expandable::Id` unrepresentable after expansion; that needs a
   response type per expansion set, which is not worth it for this slice.
2. **Wire key of a field for error mapping**, e.g. `InvoiceSettingsParams::DEFAULT_PAYMENT_METHOD_KEY:
   &str = "invoice_settings[default_payment_method]"`, so that
   `error.param() == Some(InvoiceSettingsParams::DEFAULT_PAYMENT_METHOD_KEY)` replaces a literal
   copied from the encoder's output.
3. **Field additions vs exhaustive literals**: if source compatibility for every caller style
   matters more than literal ergonomics, setters (`CustomerCreate::default().invoice_settings(..)`)
   would let fields be added without E0063; the current style is a reasonable trade-off and
   documented as such in this record.

## Results

| Command | Stage 2 (`f90923e`) | Stage 3 |
|---|---|---|
| `cargo test --locked -p concord-check-maintenance-stripe` | `before`: 3 passed | `after`: 4 passed, `before`: 3 passed |
| `cargo test --locked -p concord-example-stripe-sdk` | form 7, pages 5, wire 8, doctests 2 — all passed | same |
| `cargo test --locked -p concord-example-stripe-app` | ST-A 4, ST-B 3, doctest 1 — all passed | same |
| `cargo clippy --locked --all-targets -- -D warnings` (3 packages) | clean | clean |
| `cargo fmt --check` (3 packages) | clean | clean |

Live Stripe: not run (no credentials; the exercise is offline by design). Timings were not
measured.

## After stage 3: comparison with the SDK author's `FRICTION.md`

Read for the first time after `ca6c7b0` was committed (this section and the stage-3 SHA above are
the only later edits). The author's entries and what this exercise observed about them:

| Author entry | Exercised here? | Observation |
|---|---|---|
| F1 custom bracketed form format (`form.rs`, 728 lines) | yes | Its maintenance payoff: the new nesting needed zero encoder lines, and omitted-field compatibility followed from its `None` rule. Supports the author's "flexibility" half; adds no new friction to F1. |
| F2 per-endpoint `Stripe-Account` override + `OnAccount` macro list | no | No operation was added. A new operation (e.g. `GET /v1/payment_methods/{id}`) would repeat the two declaration lines and need a line in `context.rs`'s `on_account!` list — not tested. |
| F3 no common trait over generated list requests | no | No list added; the existing customer list took the new `expand[]` value unchanged. |
| F4 SDK copy of the native response-metadata recipe (`send()`) | used, unchanged | Kept `Request-Id`/replay metadata and the typed error document for both consumers. |
| F5, F6, F7 | no | Not touched by the change. |
| "the declaration itself was uneventful" | yes | Consistent: zero declaration code lines changed. |

Not in the author's log (found only by maintaining): the exhaustive-literal/pattern break when a
field is added (E0063/E0027); Stripe error `param` values in wire spelling with no SDK mapping to
Rust fields; expansion paths as unchecked text with the `data.` list rule and no link to the
`Expandable` field they affect; no `const` constructor for static `ExpandPath`s. All four are
SDK-design or provider-vocabulary issues; none points at a Concord library change.
