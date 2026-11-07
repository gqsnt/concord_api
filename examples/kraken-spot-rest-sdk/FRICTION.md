# Concord friction and capability decisions

Baseline is frozen; **no shared crate was edited**. These notes distinguish
provider rules from generic mechanics, with evidence in the owned source/tests.
The costs below include intended extension seams, not only missing features.

F1 — Post-encoding signing and the unsigned preparation boundary.

Evidence: `src/policy.rs::Policy::prepare_signed` must take `Prepared::into_parts`,
read the already encoded form bytes, add the policy-owned nonce, insert two
sensitive headers and retain the decoder. `tests/protocol.rs` compares exact wire
bytes/signatures to an independent Python oracle and proves generated `prepare()`
has no signature or nonce. `tests/workflows.rs` uses `Signed` for controlled
dispatch. HMAC formula, path selection and nonce placement are provider rules;
the three-part holder and its safe Debug are generic mechanics written here.
Smallest possible Concord improvement: a fallible native request-decoration
method returning a redacted prepared holder without manually carrying the client
and decoder. It must preserve the explicit distinction from ordinary unsigned
`prepare()`. Existing `SendWith` already supplies the correct execution seam.

F2 — Clone-safe nonce lifetime remains SDK-owned.

Evidence: generated endpoint clones serialize the same options, while 64 concurrent
`send()` calls allocate 64 different nonces. The reversed-arrival fixture reports
InvalidNonce on the lower arrival. `src/auth.rs::Nonces` uses checked atomic
allocation; `src/policy.rs` places it at signed preparation. A default expression
on an endpoint would run at endpoint construction and be retained by clones
(AUTHOR_GUIDE §2), so it cannot meet this contract. The shared counter is generic;
per-key persistence, seed choice and arrival rejection are provider/application
semantics. Smallest shared addition: an optional checked shared sequence utility
in concord-auth, without promising arrival order, persistence or uniqueness
across separately created sources. No core language change is needed.

F3 — Wire envelope to SDK result projection.

Evidence: declarations return `Envelope<Pairs/Balances/…>`, while the implemented
`SendWith<Envelope<T>>` returns `Result<Response<T>, Error>`. The policy manually
checks ordered error strings, requires a result and projects the payload. Generated
`send()` is documented through its associated policy output because the ordinary
`Result<Response<_>, Error>` template would retain the wire envelope. Call tables
and public policy docs spell out the projected result. Typed Kraken failures are
provider logic; repeating a safe response holder and result documentation is
generic work. Smallest improvement: a documented output-projection recipe/result
alias for `SendWith<Envelope<T>>`, or generated rustdoc that highlights the
associated output's implementation. Do not put Kraken envelopes into core.

F4 — Two independent failure layers and metadata retention.

Evidence: successful HTTP bodies can contain provider errors; a 503 can instead
contain malformed/truncated/wrong-media/oversized JSON. `Signed::send`, `Error`,
`ApiFailure` and `HttpFailure` handle these separately. Protocol tests preserve
status on every non-2xx document failure and check value-free error output.
`Received::document` and `decode_with_meta` remove most generic body/metadata work;
the remaining safe wrappers and no-source-chain errors are handwritten. Kraken
classification and ordered raw errors stay provider-specific. Smallest shared
improvement: an opt-in documented safe `Response<T>` wrapper/example and bounded
HTTP-error container, rather than each SDK recreating their redacted Debug.

F5 — Transport restrictions are outside the declaration.

Evidence: `src/lib.rs::transport` configures no redirects/native retries/proxies,
and a loopback cross-port redirect test observes no destination request. Automatic
replays could reuse a nonce; redirect forwarding could disclose custom headers.
The generated client correctly accepts reqwest transport unchanged, requiring a
handwritten helper and explicit caller contract. Smallest shared improvement: an
opt-in transport-builder recipe/helper for one dispatch with no redirects; do not
change Concord's default transport behavior. TLS remains an application feature
choice; this artifact verifies only local HTTP.

F6 — Shared auth/cache does not implement payload signatures.

Read `concord-auth/README.md` before deciding. It offers sensitive scheme/token
headers and a scoped refresh cache, and explicitly leaves signing to the SDK.
Kraken uses an opaque key header plus a payload/path-dependent HMAC and a counter;
Basic/Bearer or token refresh would change the contract. Handwritten credentials
therefore validate header bytes, decode Base64 and flag API-Key/API-Sign sensitive.
Evidence: fixed official vector and both independent wire vectors pass; Debug and
Display redaction tests cover all credential-bearing handles. Smallest possible
generic addition: a validated sensitive opaque-header constructor; keep provider
cryptography and nonce behavior outside the shared cache. No cache was added.

F7 — Pagination, retry and admission were considered without inventing contracts.

Read every relevant crate README first. `concord-codecs` JSON/Form are adopted
through the facade. `concord-retry::http::retry_after` is adopted without its
attempt loop; protocol tests forward all field lines and exercise missing,
repeated, invalid and date forms. No automatic retry is promised, so auth/nonce
failures and HTTP 429/5xx each dispatch once. The local fixed-window semantics of
concord-ratelimit have no calibrated Kraken quota contract in the three permitted
research pages; no arbitrary quota or FIFO wire ordering is claimed. The optional
OpenOrders cursor API is outside this bounded slice: omitted `with_cursor` and
`limit` use the documented complete-result mode, subject to the 1 MiB byte bound.
No continuation exists to feed concord-pagination, and a too-large result is an
explicit failure. No shared change is needed for these scope decisions. A future
cursor extension should use the shared pagination engine with provider-owned
cursor and limit validation, rather than a handwritten traversal.

Examples consulted (example-assisted result)

| Directly read example file | Taken |
|---|---|
| `examples/testserver/Cargo.toml` | Exact package/dev-dependency identity and runtime features |
| `examples/testserver/src/lib.rs` | Fixture server and lifecycle API, recorded wire inspection, body failure and gate facilities |

No existing `examples/*-sdk` source file was directly read. Assistance from required
documentation excerpts is recorded separately so it is not mistaken for an
independent implementation:

| Guide excerpt's originating example | Taken |
|---|---|
| `examples/quickstart/src/lib.rs` in README/QUICKSTART | Declaration, config, `with_base_url`, generated send and receive idioms |
| `examples/quickstart/src/main.rs` in QUICKSTART | Offline local fixture workflow shape; the referenced local_server file was not opened |
| `examples/fred-sdk/src/lib.rs` in AUTHOR_GUIDE §3 | Disable native retries, redirects and referer in a transport helper |
| `examples/frankfurter-sdk/src/lib.rs` in AUTHOR_GUIDE §5.11 | Bounded error document plus authoritative status and retained head |
| `examples/native/src/{errors,metadata,native_edit,policies,repr}.rs` as discussed in RECIPES | Native signing seam, sensitive native headers, safe Debug and explicit replay ownership; no source files opened |
| Other guide excerpts: OpenAI/Drive multipart, app/sdk traversal, compat cases | Read as required guide content; no implementation taken for this slice |

Source-read and fixture details, dependency licenses, dates and commands appear in
`PROVENANCE.md`. No private reasoning transcript is recorded.
