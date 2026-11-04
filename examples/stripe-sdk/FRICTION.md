# Friction log — Stripe v1 SDK slice on Concord

Written while implementing the SDK (`examples/stripe-sdk`) and its caller
(`examples/stripe-app`) as an author who did not design Concord or `concord-pagination`. Each
entry: the provider contract, the public Concord path used, the extra code, and whether it is
intentional flexibility or friction. Proposed library changes are proposals only; no Concord
crate was edited. Line counts are of the committed files (`wc -l`, comments and docs included).

Summary: the declaration itself (19 operations, 313 lines of `api!`) was uneventful. The extra
code concentrates in four places: a bracketed form format (F1, ~550 lines), a repeated
per-request header override (F2), a trait over generated list requests (F3) and the
response-metadata send path every SDK re-implements (F4). Nothing needed a handwritten request
or a URL rewrite.

## F1 — Stripe's nested form bodies need a custom format (friction; the `Encode` trait made it possible)

- **Provider contract.** v1 writes are `application/x-www-form-urlencoded` bodies whose keys spell
  structure: `items[0][price]=..`, `metadata[order_id]=6735`, `recurring[interval]=month`,
  `expand[]=customer` (repeated), and an *empty* value unsets a field (`description=`,
  `metadata[key]=`, `metadata=`). See `PROVIDER.md`.
- **Concord path.** `concord::Form` (feature `form`, `serde_urlencoded`) is flat by design: a
  nested struct, a map field or a sequence is a `Build` error ("the value cannot be encoded as a
  form"). The SDK implements its own format through the public trait and names it once on the
  client: `request StripeForm;`.

  ```rust
  // examples/stripe-sdk/src/form.rs
  impl<T: Serialize> Encode<T> for StripeForm {
      const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");
      fn encode(value: T) -> Result<reqwest::Body, CodecError> {
          let pairs = to_pairs(&value).map_err(|error| {
              CodecError::new("the value cannot be encoded as a Stripe form").with_source(error)
          })?;
          Ok(reqwest::Body::from(encode_pairs(&pairs)))
      }
  }
  ```
- **Extra code.** `form.rs` is 728 lines: a complete serde `Serializer` (top level, values,
  struct/map fields, sequence elements with `key[]` for scalars and `key[i]` for objects, map
  keys) of about 505 lines, `Patch<T>` / `MetadataPatch` for omitted-vs-empty, a `FormError`
  whose `Display` names rules only, and a 15-line `application/x-www-form-urlencoded` byte
  serializer (the SDK may not depend on `url`, and `concord` does not re-export
  `form_urlencoded`). Tests: `examples/stripe-sdk/tests/form.rs`.
- **Minimal reproduction** (the existing compat case shows it too, `examples/compat/tests/forms.rs`):

  ```rust
  #[derive(serde::Serialize)]
  struct Update { metadata: std::collections::BTreeMap<String, String> }
  concord::api! { pub client X { base "https://x.test"; request concord::Form;
      POST update(body b: Update) { path "u"; } } }
  // X::new(http).update(Update { metadata: [("k".into(), "v".into())].into() }).prepare()
  //   -> Err(kind Build): "body: the value cannot be encoded as a form"
  ```
- **Assessment.** Friction: every bracket-convention API (Stripe, Rails/PHP-style backends,
  Shopify REST) needs the same 500 lines. Flexibility: the open `Encode` trait made it a pure SDK
  concern with no core change, and the format composes with `request` statements, sensitivity
  and `prepare()` like a built-in. **Proposal:** a bracketed-form policy in `concord-extras`
  (`Form<Brackets>`-style: nested keys, `[]` vs indexed sequences, an explicit "empty value"
  marker type), or at least a maintained recipe; and/or re-export `form_urlencoded`'s byte
  serializer next to `Form`.

## F2 — A per-request three-state header must be declared on every endpoint, and typed ids cannot be placed (friction)

- **Provider contract.** `Stripe-Account: acct_...` selects the connected account per request;
  the platform's own account is "no header". An SDK context usually has a default account, and a
  single request must be able to inherit it, drop it, or replace it.
- **Concord path.** The author guide's §4.6 pattern, which works exactly as documented: the client
  places `#[sensitive] header("stripe-account") account?: HeaderValue`, and **each** endpoint
  declares

  ```rust
  #[sensitive] account_override?: Option<HeaderValue>,
  // ...
  headers { override "stripe-account" = each account_override else inherit; }
  ```
- **Extra code.** 19 endpoints × (1 value + 1 `override` line) = 38 declaration lines that differ
  only by endpoint; plus an `OnAccount` trait and a macro implementing it for the 19 generated
  types (`context.rs`, lines 175–211) so callers can write `.on_account(&AccountOverride::...)`
  instead of `set_account_override(Some(header_value))` / `maybe_account_override(None)`.
  `ToHeaderValue` is a closed set, so a typed `AccountId` cannot be placed as a header: the
  values are `HeaderValue`s built (and flagged sensitive) by the SDK; the typing lives in the
  `AccountOverride` wrapper.
- **Minimal reproduction:**

  ```rust
  concord::api! { pub client Api(#[sensitive] header("stripe-account") account?: HeaderValue) {
      base "https://x.test";
      scope customers {
          // There is no way to say "every endpoint below has a per-request override of
          // stripe-account"; each endpoint repeats the value and the override.
          GET list(#[sensitive] account_override?: Option<HeaderValue>) {
              headers { override "stripe-account" = each account_override else inherit; } }
          GET retrieve(id: String, #[sensitive] account_override?: Option<HeaderValue>) {
              path "{id}";
              headers { override "stripe-account" = each account_override else inherit; } }
      } } }
  // `header("stripe-account") account?: AccountId` does not compile: AccountId is not
  // ToHeaderValue; a template "{account}" can replace but cannot express "omit".
  ```

  Checked on this tree: the typed placement fails with `error[E0277]: `AccountId` cannot be sent
  as a header value` ("the trait `ToHeaderValue` is not implemented for `AccountId`").
- **Assessment.** Friction (repetition and an extra trait per SDK); the semantics are right and
  `#[sensitive]` stays sticky through the override, which the wire tests check. **Proposal:** a
  declaration-level way to give every endpoint of a scope (or client) a per-request value with an
  `override .. else inherit` rule, so the per-request choice is declared once; optionally an opt-in
  `ToHeaderValue` for `Display` newtypes (validated like a template).

## F3 — Generated list requests share no trait and expose no values (friction)

- **Provider contract.** All v1 lists page alike (`limit`, `starting_after`, `ending_before`,
  `has_more`); one adapter should serve every list operation.
- **Concord path.** Each generated request type has inherent `maybe_starting_after`,
  `maybe_ending_before` and `prepare`, with a different id type per list. The SDK defines
  `pages::ListRequest` and implements it with a macro for the 7 list operations
  (`pages.rs`, lines 91–134).
- **Extra code / workaround.** Generated requests have setters but no getters, so the adapter
  cannot ask a template whether it already sets a cursor. `CursorList::new` prepares a clone of
  the template (no I/O) and inspects the native URL's query pairs; the same prepared request
  yields the cursor *binding* fingerprint (method, path, filter pairs, `Stripe-Version`,
  `Stripe-Account`), which lets the adapter refuse a resume cursor from another account or
  filter — something `concord-pagination` documents as "not detected":

  ```rust
  let prepared = template.clone().prepare_list()?;
  if has_cursor(prepared.request().url()) { /* ProtocolError::TemplateHasCursor */ }
  let binding = fingerprint(&prepared);
  ```
- **Assessment.** Friction, small (≈45 lines), but every provider adapter will write the same
  macro. Preparing to inspect works and uses only public API, at the cost of encoding the whole
  request once. **Proposal:** generated requests could implement a common trait exposing
  `prepare` (and the output type), and/or read accessors for their values.

## F4 — Response metadata and error documents: every SDK re-implements the native recipe (friction; documented path)

- **Provider contract.** `Request-Id`, `Idempotent-Replayed: true`, `Stripe-Should-Retry` and
  `Stripe-Rate-Limited-Reason` are response headers the workflows must report; error responses
  carry a typed JSON document.
- **Concord path.** `Prepared::into_parts()`, one `client.execute`, `decoder.finish(Err(e))` for a
  transport failure (URL removed), header capture by borrowing, `Decoder::decode`, then for a
  `Status` error `into_response()` and a bounded `chunk()` loop and
  `<Json as Decode<_>>::decode_body`. This is the documented recipe of `examples/native`
  (`metadata.rs`, "error documents"), but it lives in example code, so the SDK carries its own
  copy: `send.rs` `send`/`api_failure`/`read_document`, ≈75 lines, plus the error type.
- **Assessment.** Intentional flexibility (the boundary is clean, no response is read twice, the
  unread response survives in the `Status` error), but repeated workaround across SDKs.
  **Proposal:** a small library helper, e.g. `Prepared::send_with(|head: &ResponseHead| -> M)`
  returning `(T, M)` or `(Error, Option<M>)`, so SDKs keep the decoder and Concord's error
  conversion without copying the recipe.

## F5 — Secrets in templates must have a `Display` that prints the secret (minor; flexibility used)

- The idempotency key is a template hole (`"idempotency-key" = "{idempotency_key}";`), so
  `IdempotencyKey: Display` prints the key (its `Debug` is redacted, and the value is
  `#[sensitive]`). For the secret key the SDK avoided this: the client value is the complete
  `Authorization` `HeaderValue`, built once by `StripeContext::new` with `set_sensitive(true)`, so
  `SecretKey` has no `Display` at all. Intentional flexibility (native `HeaderValue`), but the
  natural spelling `headers { AUTHORIZATION = "Bearer {key}"; }` would require a `Display` that
  leaks.

## F6 — `concord-pagination` from a provider adapter (mostly smooth)

- `CursorList::fetch(Option<Cursor>)` returns an owned future (`+ use<R>`, borrowing nothing from
  the list), so the factory is one line: `sequential(from, budget, |c| list.fetch(c))`. The SDK
  cannot return a ready-made `Sequential` without naming the closure and future types (or
  boxing), so it hands out the factory; acceptable.
- `Page { items, next, meta }` with `meta = ResponseMeta` carried request ids and rate-limit
  reasons through the traversal without loss; `SequentialReport::resume` gave the right restart
  point after a `429` (the failed page's cursor) and after a budget stop (the next cursor).
- `Items` drops page metadata (items only), so ST-A walks pages to keep request ids;
  `ItemCheckpoint`/`resume_items` were exercised separately (`tests/pages.rs`, mid-page stop and
  replay).
- Resume binding is the adapter's job (the crate documents it as caller-enforced): the Stripe
  cursor carries its own binding (F3). A pagination-level hook to validate a resume key against
  the plan would let other adapters do the same uniformly — proposal only.
- An invoice's embedded `lines` list is the first page of the line chain; the adapter's
  `after_embedded` applies the same `has_more` rules and the chain starts at `Resume::At`. No
  pagination API change was needed.

## F7 — Small observations (no change proposed)

- Query order follows declaration order, not setter order (`created[gte]` before `limit`
  although the caller set `limit` first). Documented by Concord; tests assert exact targets.
- `rustfmt` does not format `api!` bodies; the declaration is formatted by hand.
- `Client::validate_header_names()` caught nothing here (all literal header names), but costs one
  line in the tests (`tests/wire.rs`).
- The application binaries need `tokio` as a normal dependency (`rt` only, current-thread
  runtime built by hand); the library workflows stay runtime-agnostic. Approved by the
  coordinator.
