# Concord friction — SDK25 FRED

Frozen baseline `56a16ee3`; shared crates were not edited. Observable evidence is
in the named files/tests and EVIDENCE.md. This work is **documentation/example-assisted**;
PROVENANCE.md inventories every directly consulted example and embedded excerpt.

| Item | Observable evidence / handwritten cost | Ownership | Smallest candidate change |
|---|---|---|---|
| F1: no shared XML codec | `src/codecs.rs`: Xml Decode implementations, safe parser options, attribute-to-model bridge; default XML workflows pass | XML parsing/head mechanics generic; root/item names, attributes and numeric fields belong to FRED | Optional XML support in `concord-codecs`, with explicit DTD/node limits and an attribute-aware model route; retain FRED's envelope/model rules. A single provider is evidence for evaluation, not enough to adopt it universally |
| F2: no bounded ZIP capability | `ZippedCsv` must inspect members, cap compressed/aggregate extracted sizes and entries, bound actual reads and check CRC; protocol fixtures exercise every branch family | ZIP mechanics generic; which files form a FRED export and what to return are SDK semantics | External bounded ZIP reader accepting byte/entry budgets and returning verified member bytes; FRED keeps membership selection. No need to add ZIP interpretation to Concord core |
| F3: shared media helper is private | Xml/ProblemDocument/ZippedCsv implement singleton Content-Type and charset checks using mime; Json already supplies its own strict check | Generic head-validation mechanics | Expose a reusable strict media/charset validator from `concord-codecs`; format-specific allowlists remain in the SDK |
| F4: bounded ordinary send needs a policy | `Policy::send` calls prepare/receive/decode_limited and document with two caps; a native prepared send bypasses those caps | Limits generic; authoritative HTTP status and problem schemas SDK-specific | A shared finite-body limit policy/combinator, preserving typed provider-error handling. Current SendWith extension is sufficient and avoids per-endpoint wrappers |
| F5: representation choice duplicates endpoint declarations | XML and JSON search/metadata/observations share routes but differ in query `file_type`, output format and Accept. CSV also changes output type | Provider file_type semantics plus static format selection | A format value tied to a request parameter could remove duplicates, but would introduce runtime configuration/type relationships. Retain explicit methods here; no macro change justified by this slice alone |
| F6: offset envelope validation remains handwritten | `src/pagination.rs` projects two model types and checks count/offset/limit, overflow and nonprogress; `pages` delegates scheduling, caps, failure and resume to shared Pages | Projection and continuation meaning provider-specific; checked offset arithmetic reusable | Optional `concord-pagination` offset helper taking count/offset/limit/length. Keep query identity and snapshot guarantees with the SDK/application |
| F7: query credential crosses the native boundary | `http_client` must disable redirects/referrer and native retries; tests prove no forwarded redirect request and secret-free handles/errors. Native request access remains deliberately raw | Transport/application configuration, not declaration composition | Add a concise query-key transport recipe/helper; no implicit core transport mutation. `#[sensitive]` already solves generated Debug/Display without auth wrapper code |
| F8: provider errors need dual interpretation | Official errors docs label their JSON example text/xml; ProblemDocument checks an XML/JSON media type then selects body parser. Problem needs manual Debug to withhold echoed credentials | FRED/document inconsistency and provider text confidentiality are SDK semantics | No core behavior change: silently sniffing every success body would weaken the head contract. A shared alternative-format combinator might remove boilerplate but must make fallback policy explicit |

Shared capabilities that removed work:

- `api!` owns route/query encoding and inheritance, sensitive key diagnostics,
  generated setters and `send()`; the selected-series scope naturally shares its ID.
- `Decode` keeps head rejection before a stalled body; no native transport escape
  was needed for XML or ZIP. Body-only decoding happens inside the custom formats.
- `SendWith`, `Received::document` and `decode_limited` keep bounded provider
  handling on the ordinary generated path, without a second family of call methods.
- `concord-pagination::Pages` supplies explicit bounded/failure outcomes and
  resumable offsets, keeping already-yielded data with the caller.
- `concord-retry::http::retry_after` replaces an early ad hoc integer projection;
  it checks every field line and retains parse failure classes. No retry engine
  is layered into this slice.
- `concord-codecs::Json` is used through Concord's `json` facade feature.
  Shared `csv` is intentionally not enabled: the HTTP body is a ZIP and this
  slice exports exact CSV bytes rather than parsing a provider-dependent table.

## Example use and initial diagnostics

Direct consultation: `examples/testserver/Cargo.toml` and
`examples/testserver/src/lib.rs` supplied local server lifecycle and request/body
helpers. No existing SDK source file was opened. Required guides' embedded
Frankfurter send-policy excerpt influenced the generic bounded policy; embedded
Quickstart examples informed declaration/sensitive-value use. OpenAI/Drive
multipart and native/compat native-edit excerpts supplied no implementation code.
See PROVENANCE for exact file identities and the exhaustive reading record.

External/setup diagnostics, distinct from Concord friction: the first offline
resolution could not find zip; `zip 2.4.2`'s `deflate-flate2` alone did not activate
its optional `flate2` dependency (E0432/E0433); explicit `flate2` feature/backend
fixed it. One SDK comparison needed a dereferenced MIME parameter name (E0277).
The first redaction test incorrectly required every scope Debug to include a
redaction marker; scope Debug omits inherited values, so the meaningful assertion
is that the secret is absent. Missing test-crate documentation warnings were
fixed before the warning-denying gate. None required shared source edits.
