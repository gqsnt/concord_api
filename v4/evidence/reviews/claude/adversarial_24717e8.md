<!-- Verbatim final report of the Claude adversarial subagent (Opus 5.5 XHigh), extracted from its transcript. -->

# Concord V4 adversarial review: snapshot `24717e8` (concord-v4)

I found ten defects and one informational item. Every one is reproduced, and the source was not edited. Repro crate: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/adversarial/exp`, with tests in `tests/*.rs` and compile-failure repros in `repro/*.rs`. I deleted the target dir afterwards because the shared tmpfs hit "Disk quota exceeded".

## Findings (most severe first)

### ADV-1: Medium. Confidence: high (the scope question is arguable). A `#[sensitive]` host label leaks through `Error::source()` on a TLS name mismatch
- **Where:** `crates/concord/src/error.rs:63` (`without_url()` only strips reqwest's URL field) and `error.rs:188-190` (`source()` returns the whole reqwest chain).
- **Repro:** `tests/tls.rs`. It uses `subdomain "{tenant}"` with `#[sensitive] tenant`, a client with `.resolve(...)` and `.tls_certs_only([ca])`, and `openssl s_server` presenting a cert for `other.example.test`.
- **Observed:**
```
Prepared Debug: ... url: "https://<redacted>.api.example.test:44731/" ...
Display: GET T.tenant.ping: transport error (connect)
source chain: -> error sending request -> client error (Connect) -> invalid peer certificate: certificate not valid for name "t0psecret.api.example.test"; ...
```
- **Expected:** the repo's own `s05_transport_error_does_not_leak` says secrets do not appear in "Display, Debug or the source() chain". D8 makes host labels sensitive, yet this one is redacted in `Prepared` and printed in the error chain. Logging `{:#}` or walking the chain (anyhow, tracing) exposes it.
- **Not affected:** the default getaddrinfo DNS failure does not leak (`tests/dns.rs`).
- **Hypothesis, not reproduced:** reqwest's `hickory-dns` resolver errors would contain the name.
- **Fix:** when `meta.sensitive_labels` is non-empty, make `source()` return `None` and keep `transport()` as an explicit escape hatch documented as unredacted. Alternatively, narrow the stated guarantee and the test comment.

### ADV-2: Medium. Confidence: high. "Effective" sensitivity from the safety list is not applied to path segments, labels or other keys
- **Where:** `model.rs:479` (components use `value.sensitive`), `model.rs:635,668,687` (contributions use `v.sensitive`). The safety-list result only reaches `debug_sensitive`, at `model.rs:305-306`.
- **Repro:** `tests/sens.rs`
```rust
pub client Eff(header("Authorization") token: String) {
    base "http://eff.invalid/v1";
    scope keys { path "keys/{client.token}"; GET get(); }
    GET q(api_key: String) { query { "api_key" = api_key; "echo" = "{api_key}"; } }
}
```
- **Observed** (each value is redacted and printed inside the same output):
```
client debug: Eff { base: "http://eff.invalid/v1", token: <redacted> }
prepared: ... url: "http://eff.invalid/v1/keys/TOPSECRET", headers: [authorization: <redacted>] ...
endpoint debug: EffQ { api_key: <redacted> }
prepared: ... url: "http://eff.invalid/v1?api_key=<redacted>&echo=APIKEYSECRET" ...
```
- **Expected:** a value Concord already withholds as sensitive is withheld everywhere in Concord output.
- **Fix:** after composition, promote every `debug_sensitive` value to `sensitive` and recompute component, contribution and slot sensitivity until nothing changes. Then emit `sensitive_segments`, `sensitive_labels` and `sensitive_query` from the result.

### ADV-3: Medium-Low. Confidence: high. Hygiene: generated locals are emitted under user-type spans
- **Where:**
  - `emit.rs:538` (`__v`)
  - `emit.rs:580-583` (`__p.query…` / `__p.header…`)
  - `emit.rs:635,637` (`__c`)
  - The bindings use call-site spans: `emit.rs:536` (`Some(__v)`), `648` (`let mut __c`), `759` (`let mut __p`).
- **Repro:** `repro/hygiene.rs`. A wrapper macro passes the body through as tokens:
```rust
macro_rules! make_api { ($($body:tt)*) => { concord::api! { pub client Hyg { base "http://h.invalid"; $($body)* } } }; }
make_api!(GET e(query q?: String, query r: String, id: u32) { path "x/{id}"; });
```
- **Observed:**
```
error[E0425]: cannot find value `__c` in this scope
error[E0425]: cannot find value `__p` in this scope
error[E0425]: cannot find value `__v` in this scope
help: an identifier with the same name is defined here, but is not accessible due to macro hygiene
```
- **Expected:** it compiles. Passing a single `$t:ty` happens to work because of the invisible group.
- **Fix:** keep the user span only on the expression (`&#place`) for diagnostics. Build `__p`, `__v` and `__c` with `quote!` or one fixed `Span`.

### ADV-4: Low-Medium. Confidence: high. A renamed dependency breaks every expansion
- **Where:** `emit.rs` hard-codes `::concord::` (32 sites).
- **Repro:** `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/adversarial/rename`, using `concord4 = { package = "concord", ... }` and `concord4::api!{...}`.
- **Observed:** `error[E0433]: cannot find 'concord' in the crate root`.
- **Why it matters:** having V3 and V4 side by side, which a migration needs, forces exactly this rename.
- **Fix:** make `concord::api!` a `macro_rules!` that forwards `$crate` to the proc macro, or add a `crate = path` option. Otherwise document the limitation.

### ADV-5: Low. Confidence: high. A base URL the macro accepts panics at runtime in `new()`
- **Where:** `model.rs:1091-1101` treats only all-digit last labels as numeric. WHATWG/`url` also treats `0x…` hex labels (including bare `0x`) as IPv4 numbers.
- **Repro:** `tests/base.rs`, with `base "http://api.0x10";`.
- **Observed:**
```
panicked at .../__private.rs:72:23: api! accepted an invalid base URL "http://api.0x10": request not built: base URL is not an absolute URL
```
- **Expected:** a compile error. `validate_base`'s doc promises "the generated constructor cannot fail at runtime".
- **Fix:** also reject a last label that matches `0[xX][0-9a-fA-F]*`, or have the macro validate with `url::Url::parse`.

### ADV-6: Low-Medium. Confidence: high. `Decode::DISCARD` is documented but never read, so `()` buffers the whole body
- **Where:**
  - `repr.rs:29-31`: the trait doc says "drained chunk by chunk without being buffered, and `decode` receives an empty buffer".
  - `repr.rs:174-177`: the `()` doc says the same.
  - `prepared.rs:123-131`: always calls `.bytes()`.
- **Repro:** `tests/discard.rs`, a user `Decode` with `DISCARD = true` against a 1 MB response.
- **Observed:**
```
decode() received 1000000 bytes with DISCARD = true
assertion `left == right` failed ... left: 1000000 right: 0
```
- **Consequences:** every `-> ()` endpoint holds the full body in memory, contrary to its doc. Third-party `Decode` impls relying on the contract get non-empty input.
- **Fix:** in `Decoder::decode`, `if R::DISCARD { while response.chunk().await?.is_some() {} ; R::decode(Bytes::new()) }`. Otherwise delete the constant and its docs.

### ADV-7: Low. Confidence: high. An invalid `Encode::CONTENT_TYPE` panics instead of returning `Error::Build`
- **Where:** `__private.rs:300`, `HeaderValue::from_static(content_type)`.
- **Repro:** `tests/ctype.rs`, a user `Encode` with `CONTENT_TYPE = Some("application/x\u{2011}custom")`.
- **Observed:** `panicked at http-1.5.0/src/header/value.rs:67:17: HeaderValue::from_static with invalid bytes`, raised from `prepare()`.
- **Fix:** use `HeaderValue::from_str` and map failure to `Error::build(.., "content-type", ..)`, or document the requirement on the public trait const.

### ADV-8: Low. Confidence: high. A literal header value that can never be valid compiles, then always fails at runtime
- **Where:** `model.rs:671-691` (no check for the header family). The check only exists at runtime, at `__private.rs:215`.
- **Repro:** `repro/header_literal.rs`, with `GET e() { headers { "x-name" = "café"; "x-ctl" = "a\nb"; } }`.
- **Observed:** `Err(Error { kind: Build, ... x-name header value contains a control or non-ASCII character })` on every call.
- **Expected:** a compile error, since the literal is static.
- **Fix:** in the model, require header-template literal pieces to be visible ASCII or tab.

### ADV-9: Low. Confidence: high. The proc macro panics on `{r#self}` (also `r#super`, `r#crate`, `r#Self`, `r#_`)
- **Where:** `template.rs:73-74`, `syn::Ident::new_raw(raw, span)`.
- **Repro:** `repro/rawself.rs`, with `path "{r#self}"`.
- **Observed:** `error: proc macro panicked ... message: 'self' cannot be a raw identifier`.
- **Fix:** reject these names with a spanned `syn::Error` before calling `new_raw`.

### ADV-10: Low (semantics/docs). Confidence: high. `each` over a required `Option<T>` makes `None` mean "absent", including deleting an inherited key
- **Repro:** `tests/opt_each.rs`
```rust
pub client O(query("k") k: String = "inherited") { base "http://o.invalid";
  GET e(filter: Option<String>) { query { override "k" = each filter; "f" = each filter; } } }
```
- **Observed:**
  - `filter=None -> query=None`: the inherited `k=inherited` is gone.
  - `Some("x") -> k=x&f=x`.
- **Why it conflicts:** D3 says a required `x: Option<T>` `None` "is a payload, never absent". D6 says Concord "never infers how None is written". With `each`, `None` is spelled as no occurrences, and under `override` it acts like `remove`.
- **Fix:** document that `each` takes any `IntoIterator`, so `Option` gives 0 or 1 occurrences and `override … = each` with an empty iterator removes the key. Rejecting it would need type-name inspection, which the charter forbids.

### ADV-11: Info
With 8 required client values, clippy reports `too_many_arguments (9/7)` on the generated `new` (`lintlib/`). Users cannot silence it from inside `api!`, because only doc attributes are accepted there. The rustc lints `missing_docs`, `unreachable_pub`, `unused`, `unused_qualifications` and `rust_2018_idioms` stay clean under `deny`.

## What I tried that did not break

- **Composition (D4)** (`tests/comp.rs`). All results matched D4:
  - remove then reintroduce keeps the original slot position (`a=V&b=2`);
  - append after an absent or present override;
  - nested optional overrides across scope and endpoint;
  - a template mixing required and optional holes under `override` (absent falls back);
  - header case folding (`"X-TWO"` overrides `"x-two"`);
  - `each` under override (absent, empty, one item);
  - direct placements ordered before block statements.
- **Naming/hygiene in normal use** (`tests/names.rs`, under `#![deny(warnings)]`):
  - value names `f`, `value`, `url`, `http` (on a scope), `r#type`, `r#match`, `fmt`, `self_`, `clone`;
  - a raw-ident scope and endpoint (`r#type`, `r#loop`);
  - a value type with a lifetime (`Tenant<'static>`);
  - a `pub(crate)` client in a submodule;
  - two `api!` in one module, including a client with no endpoints;
  - an endpoint directly under the client.
  - A value named `Some` hits rustc E0530; I'm counting that as a contrived name.
- **Redaction** (`tests/redact.rs`, `tests/deep.rs`), with no leaks and correct indices:
  - runtime bases `/base`, `/a/b/c/`, none, and `//`;
  - trailing slash;
  - mixed literal and sensitive labels (`x{p}.{s}-z`);
  - a 5-level chain with multiple labels per owner and qualified references;
  - query keys needing encoding (`"a b&c=d"` becomes `a+b%26c%3Dd=<redacted>`).
- **Bodies and positional order** (`tests/traits.rs`):
  - body position between a defaulted value and a required value;
  - an optional body sets no content-type when absent;
  - a declared `Content-Type` wins.
  - `Error` and `Prepared<Text>` are `Send + Sync + 'static`, and the `send()` future is `Send`.
- **Classification** (`tests/status.rs`):
  - 302 is a Status error;
  - HEAD `-> ()` is Ok;
  - bad JSON is a Decode error with no body content in the message;
  - a truncated body is "Transport (reading the response body)";
  - 204 with `Json<Option<_>>` is a Decode error;
  - `()` with a 200 body is Ok.
- **Other:**
  - `HeaderName::from_static` accepts every tchar the macro allows, so it cannot panic.
  - I found no scope creep or V3 remnants: no retry, rate, pagination, auth or executor code in the crates.

## Limitations

- I did not run the repo's own test or trybuild suite, because of the disk quota.
- `latin` in `status.rs` had a content-length typo, so it tells us nothing about `Text` UTF-8 errors.
- **Not tested:**
  - the hickory-dns hypothesis;
  - HTTP/2 error chains;
  - how redirects interact with sensitive custom headers (outside the guarantee per S06).
- **Not a defect under D8, but a gap:** secrets inside a runtime base path (webhook-style URLs) are printed by client and `Prepared` Debug, because the base is not a value.
