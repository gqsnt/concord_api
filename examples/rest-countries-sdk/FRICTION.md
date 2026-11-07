# Friction

The Concord costs met while writing this SDK, each with the observable evidence, what had to be
written by hand, whether the cost is provider semantics or generic mechanics, and the smallest
Concord change that would remove it. Shared crates were not edited (frozen baseline
`c3fd4f1e`); the changes below are proposals only.

What did **not** cost anything is worth stating too: one `concord::api!` block (about 60 lines,
mostly doc comments) declares all five endpoint shapes. The generated client stays the object
callers hold. Run-time filter keys fit `query(..)` with a `QueryPairs` type (`Filters`). The shared
response options are one parameter group, `params ListOptions`. `send SendPolicy -> Result<_, Error>`
makes every `send()` return the SDK's error. `Sends<O>` plus the group's trait let one bounded walk
(`pages()`) serve every endpoint without a wrapper method per operation.

## 1. A comma that delimits a list travels as `%2C`

- **Evidence.** `tests/protocol.rs`, `documented_request_examples_reach_the_wire_as_documented`:
  the documented `?response_fields=names.common,codes.alpha_2,flag.emoji&limit=3` reaches the
  wire as `?response_fields=names.common%2Ccodes.alpha_2%2Cflag.emoji&limit=3`. The decoded
  pairs are equal, and the test checks both.
- **By hand.** Nothing. `Fields` is a `Display` newtype joining with `,` (AUTHOR_GUIDE §6). The
  risk is recorded as unverified (PROVENANCE.md, inference 2): live calls were not run.
- **Provider or generic.** Generic. Comma-separated lists in one query value are common. RFC
  3986 lets a component give sub-delimiters a meaning, so `%2C` and `,` need not be equivalent
  to every server. If this provider ever refused `%2C`, the only exit today is native:
  `prepare()?.into_parts()` and `url_mut().set_query(..)`. That route loses the generated
  `send()`, the send policy and Concord's redaction of the request.
- **Smallest change.** A placement option that keeps RFC 3986 sub-delimiters
  (`, ; : @ ! $ ' ( ) *`) unescaped in a query value. It could be a keyword on the placement
  (`query("response_fields") keep_subdelims fields?: Fields`) or a
  `QueryPairsWriter::pair_with(key, value, Encoding::QueryComponent)`. The SDK's `Display` still
  decides the text; Concord still encodes `&`, `=`, `#`, `+` and controls.

## 2. A credential newtype cannot be the declared header value

- **Evidence.** `ToHeaderValue` is sealed (`crates/concord-core/src/header.rs`). Declaring
  `#[sensitive] header(AUTHORIZATION) key: ApiKey` does not compile, so the generated
  `RestCountriesConfig` exposes `authorization: HeaderValue`. Callers write
  `RestCountriesConfig { authorization: bearer(&key)? }` (`tests/support/mod.rs`, `client`).
- **By hand.** `bearer()` (a documented one-line wrapper over `concord_auth::bearer`). The
  client and field docs explain where the value comes from. Nothing stops a caller from passing
  any other `HeaderValue`, such as the provider's bare-key form, which the docs also accept.
- **Provider or generic.** Generic: every SDK whose key needs validation at construction.
- **Smallest change.** Implement `ToHeaderValue` for SDK types through an unsealed,
  value-returning trait (`fn to_header_value(&self) -> Option<HeaderValue>`, the current
  signature), or accept `T: AsRef<HeaderValue>`. Then `RestCountriesConfig { key: ApiKey }` would
  carry the validation in the type.

## 3. Two different `Error`s render identically in the generated docs

- **Evidence.** `cargo doc` output for `RestCountriesCountries`:
  `prepare(self) -> Result<Prepared<CountryPage>, Error>` (that `Error` is `concord::Error`), and
  `send(self) -> impl Future<Output = Result<CountryPage, Error>>` (the SDK's `Error`). They look
  the same, are not interchangeable, and only the link targets differ. `with_base_url` is the
  same. `examples/frankfurter-sdk` hit this too: its client doc carries a paragraph about it.
- **By hand.** The client's doc comment and `Error`'s doc say which call returns which error.
  `From<concord::Error> for Error` makes `?` work across both.
- **Provider or generic.** Generic: every client with `send Policy -> Result<_, E>` whose error is
  named `Error`.
- **Smallest change.** When a client declares a send policy, the generated docs of `prepare()`
  and `with_base_url` should name the type in prose: "returns [`concord::Error`], not the send
  policy's error".

## 4. The non-2xx error pair is the same boilerplate in every SDK

- **Evidence.** `src/error.rs`: `ErrorBodyError` (split by `ErrorKind` into Metadata / Read /
  Decode) and `ProviderError { meta, document }`, with a `Debug` that hides the document text.
  That is about 90 lines with docs. It mirrors `examples/frankfurter-sdk/src/error.rs` almost line
  for line; only the document type differs.
- **By hand.** Those two types and their `Display`, `Debug`, `Error` and `From` impls.
- **Provider or generic.** Generic. Only `ErrorDocument` (the `errors[].message` shape) and
  `ProviderClass` (the status table) are provider semantics.
- **Smallest change.** A `concord::ProviderError<D>` holding `ResponseMeta` and
  `Result<D, concord::Error>`, with a redacting `Debug`. `Received::document` would then have a
  sibling that returns it directly. SDKs would keep their document type and status
  classification.

## 5. The documented out-of-range answer needs a special case before `next_offset`

- **Evidence.** The provider documents an offset past the end as an empty `200` page
  (`derived-out-of-range-offset.json`: `offset` 300, `total` 249). The shared
  `concord_pagination::next_offset(300, 25, 0, Some(249))` reports `OffsetError::BeyondCount`.
  `CountryPage::next_offset` therefore treats an empty page at or past `total` as the end before
  calling it. `workflow_2_an_offset_past_the_end_ends_the_walk` covers this.
- **By hand.** Three lines, plus the `meta.count` and `meta.more` cross-checks.
- **Provider or generic.** Provider semantics: another provider could rightly call that page
  inconsistent. No Concord change is proposed. The shared function stays strict and the rule
  stays in the SDK.

## Shared capabilities considered

| Crate | Decision | Why |
|---|---|---|
| `concord-auth` | **used**: `bearer()` | The key is static, so no token cache. Validation (no empty, whitespace, control or non-ASCII bytes) and the sensitive flag replace hand-written checks. |
| `concord-pagination` | **used**: `next_offset`, `Pages` | Checked offset arithmetic, plus the bounded, explicit walk of `pages()`. `send()` itself never paginates, so a no-match is one request. |
| `concord-retry` | not used | The slice requires a no-match or an unavailable field (`404`/`403`) to stay one ordinary outcome. `send()` makes exactly one request. Whether to repeat a `429` or `5xx` read is the caller's policy, and the provider documents no `Retry-After` (headers stay readable on `ProviderError::meta`). |
| `concord-ratelimit` | not used | The provider's `429` limiter (20 requests per 10 seconds) is per account, across processes. A process-local window inside the SDK would claim a guarantee it cannot give. A caller can still compose admission around `send()`. |
| `concord-codecs` features | `json` only | Every response is JSON. No stream, form or multipart. |

## Provider costs (not Concord), for completeness

- The no-match shape is undocumented, so the SDK infers it (PROVENANCE.md, inference 1).
- Premium values are replaced in place by a notice of another shape, which needed a custom
  `Deserialize` for `Premium`.
- Every country property can be absent under an allowlist, so every model field is an `Option`.
- The `_demo` block, `leaders` entries and several sub-objects are not documented field by
  field, so they are kept as JSON.

## Examples consulted

`examples/frankfurter-sdk` (`Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/model.rs`,
`tests/common/mod.rs`, the first 200 lines of `tests/protocol.rs`). Taken from it: the
send-policy shape, the `ErrorBodyError` split, the redacting `ProviderError` `Debug`, the offline
test client and the shared wire assertion. Details are in PROVENANCE.md.
