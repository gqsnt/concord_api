# Fixtures, provenance and evidence tiers (DATA-01)

No fixture in this delivery is a captured provider response. Every provider fixture is classed in
its manifest as `official_example` (a licensed documentation example, edits recorded),
`schema_derived_synthetic` (built from the pinned documentation/schema) or `fault_injection`. Each
manifest entry records the workflow and operation, SDK source path, provider version, schema
digest or documentation URL with retrieval date, sha256 of the bytes, wire assertions, expected
outcome and the test command; each app has a test that recomputes the hashes and checks that every
file is listed.

| Provider | Manifest | Entries | Classes | Pinned version / schema | Hash check |
|---|---|---|---|---|---|
| GitHub | [`examples/github-app/tests/fixtures/MANIFEST.json`](../../examples/github-app/tests/fixtures/MANIFEST.json) | 43 (+5 generators) | 18 official example (MIT, from the pinned description), 23 schema-derived, 2 fault | REST `2026-03-10`; `github/rest-api-description` `api.github.com.2026-03-10.json` | `examples/github-app/tests/manifest.rs` |
| Stripe | [`examples/stripe-app/tests/fixtures/MANIFEST.json`](../../examples/stripe-app/tests/fixtures/MANIFEST.json) | 13 | 11 schema-derived, 2 fault | `Stripe-Version: 2026-10-28.endive` (reference pages; no OpenAPI digest claimed) | in the ST-A/ST-B tests |
| Shopify | [`examples/shopify-app/tests/fixtures/MANIFEST.json`](../../examples/shopify-app/tests/fixtures/MANIFEST.json) | 29 (+ deterministic generator) | 22 schema-derived, 7 fault | GraphQL Admin `2026-10` (versioned reference pages; no machine-readable schema digest) | `examples/shopify-app/tests/manifest.rs` |
| Salesforce | [`examples/salesforce-app/tests/fixtures/MANIFEST.json`](../../examples/salesforce-app/tests/fixtures/MANIFEST.json) | 22 (+ generated in tests) | 18 schema-derived, 2 official example, 2 fault | REST/Bulk API 2.0 `68.0` (documentation retrieved through a browser; the archive pages are not frozen per version) | `examples/salesforce-app/tests/manifest.rs` |
| Kubernetes | [`examples/kubernetes-app/tests/fixtures/MANIFEST.json`](../../examples/kubernetes-app/tests/fixtures/MANIFEST.json) | 37 (+2 generated datasets) | 32 schema-derived, 5 fault | v1.37.2 OpenAPI v3 (`api__v1` sha256 `d09ab224…`, `apps__v1` `e1326db2…`, `batch__v1` `39e41a47…`) | `examples/kubernetes-app/tests/fixtures_manifest.rs` |

Codec seeds (synthetic handoff seeds, each with `PROVENANCE.md` and `.gitattributes` keeping CRLF
bytes exact): `crates/concord-extras/tests/data/{csv,jsonl,json_values}/`. Large data is generated
on demand, never stored: `checks/stream-stress` (deterministic `rec(i)`), the GitHub/Shopify/
Salesforce/Kubernetes generators named in their manifests.

Provider sources, supported and deliberately unsupported operations, and documented discrepancies
with their resolution are in each SDK's `PROVIDER.md`.

## Evidence tiers per workflow

| Workflow | OFFLINE_VERIFIED | REAL_SERVER_VERIFIED | LIVE_PROVIDER_VERIFIED |
|---|---|---|---|
| GH-A, GH-B | yes (loopback fixtures; `cargo test -p concord-example-github-app`) | not applicable | NOT_RUN (no credentials; binaries print `NOT_RUN`) |
| ST-A, ST-B | yes (`cargo test -p concord-example-stripe-app`) | not applicable | NOT_RUN (no key; ST-B needs a sandbox mutation opt-in) |
| SH-A, SH-B | yes (`cargo test -p concord-example-shopify-app`) | not applicable | NOT_RUN (no shop; SH-B job creation needs a development-shop opt-in) |
| SF-A, SF-B | yes (`cargo test -p concord-example-salesforce-app`) | not applicable | NOT_RUN (no org; SF-B needs sandbox job/mutation opt-ins) |
| KU-A, KU-B | yes (`cargo test -p concord-example-kubernetes-app`) | NOT_RUN (no kind/k3d/kubectl on the machine; no existing cluster used) | NOT_RUN |

Offline fixtures are checked against primary documentation by their authors, not against a live
service: agreement with a fixture is not live interoperability. Assumptions the documentation did
not settle (examples: Shopify's bulk result Content-Type, the HTTP status of an expired signed
URL, whether Salesforce accepts chunked uploads, Stripe's status for a reused idempotency key with
different parameters) are listed in the SDKs' `PROVIDER.md` files.
