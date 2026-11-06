# Provenance

Every fixture under `fixtures/` is **hand-authored**. No byte was captured from a running
Distribution server, a public registry or a real token service; no credential was used. The
fixtures follow the shapes the official pages document; where a page shows a complete example,
the table says so and lists every change.

## Sources checked

| Source | URL | Version / date | Accessed |
|---|---|---|---|
| Registry HTTP API V2 | https://distribution.github.io/distribution/spec/api/ | A living document: no version or publication date on the page. Its "Changes" list ends at entry **l** ("Document TOOMANYREQUESTS error code"). | 2026-11-06 |
| Token Authentication Specification | https://distribution.github.io/distribution/spec/auth/token/ | No version or date on the page (its examples carry 2015 dates). | 2026-11-06 |
| Token Scope Documentation (linked from the token page) | https://distribution.github.io/distribution/spec/auth/scope/ | No version or date. Used for: scopes are space-separated in one value; a `GET` token request uses one `scope` parameter per scope. | 2026-11-06 |
| Image Manifest V2, Schema 2 | https://distribution.github.io/distribution/spec/manifest-v2-2/ | "schema version 2"; no date. Used for the Docker media types and descriptor fields. | 2026-11-06 |
| OCI Image Manifest | https://github.com/opencontainers/image-spec/blob/main/manifest.md (branch `main`) | `main` at access time; no release pinned. Used for the OCI manifest, config and layer media types. | 2026-11-06 |
| Distribution releases | https://github.com/distribution/distribution/releases | Latest release listed: v3.1.2 (2026-09-24). Recorded for orientation only: the spec pages are not versioned with releases. | 2026-11-06 |

How they were checked: each page was fetched with a web tool that returns a model-written
summary of the page text (with short quotes), not the raw page. Statements quoted or relied on
below were confirmed in those summaries; exact spellings of the spec's error-table *messages*
were not shown verbatim by the tool (see "Synthetic" below).

## Facts relied on, by page

Registry API page:

- `GET /v2/`: 200 = V2 implemented; 401 = read `WWW-Authenticate`, retry; 404 = not V2;
  `Docker-Distribution-API-Version: registry/2.0` on 200/401.
- Error body `{"errors": [{"code", "message", "detail"}]}`; the code list (`BLOB_UNKNOWN` …
  `TOOMANYREQUESTS`); unknown codes are treated as `UNKNOWN`.
- Repository name components `[a-z0-9]+(?:[._-][a-z0-9]+)*`, joined by `/`, total under 256
  characters. Digest grammar `algorithm ":" hex`, `algorithm := /[A-Za-z0-9_+.-]+/`,
  `hex := /[A-Fa-f0-9]+/`; "compliant implementations should use sha256"; clients must verify
  content against the digest used to fetch it; `Docker-Content-Digest` is not trusted over the
  local digest; a header with another algorithm may be ignored.
- Manifest `GET` with an `Accept` naming the supported types; `Content-Type` names the type;
  `Docker-Content-Digest`. Manifest `PUT` with `Content-Type`; `201 Created` with `Location` and
  `Docker-Content-Digest`; missing layers are `400` with `BLOB_UNKNOWN` entries whose
  `detail.digest` names each.
- Blob `HEAD`/`GET`: `200` with `Content-Length`, `Docker-Content-Digest`; `404` `BLOB_UNKNOWN`.
- Upload: `POST /v2/<name>/blobs/uploads/` → `202` with `Location`, `Range: bytes=0-<offset>`,
  `Docker-Upload-UUID` (`Range: bytes=0-0` for a fresh upload); clients treat the upload URL as
  opaque, never assemble it, and use the most recent `Location`; chunks are `PATCH` with
  `Content-Range: <start>-<end>` (inclusive, a non-standard use; start = last end + 1),
  `Content-Length`, `Content-Type: application/octet-stream`, in order; `202` with new `Location`
  and `Range`; `416` with `Location` and the last valid `Range`, from which the client resumes;
  status `GET` → `204` with `Range` and `Docker-Upload-UUID`; completion
  `PUT <url>?digest=<digest>` (final chunk or empty body) → `201` with `Location` and
  `Docker-Content-Digest`; `400 DIGEST_INVALID` means the upload must be restarted. Byte ranges
  are inclusive "and that will be honored".
- Catalog / tags: `n`, `last`; `Link: <<url>?n=<n>&last=<last>>; rel="next"` only when more
  results exist; no `Link` means all results were returned; use the link to page forward; `n` may
  change on later pages; results are lexically ordered; the catalog's contents are
  registry-specific (full, limited by access, or partial for a mirror).

Token page:

- The flow (attempt → `401` + challenge → token request to `realm` → retry with
  `Authorization: Bearer <token>`); the challenge
  `Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:samalba/my-app:pull,push"`;
  token request `GET <realm>?service=..&scope=..` (repeatable `scope`, `client_id`,
  `offline_token`); response `token` or `access_token` (at least one), `expires_in` (default 60),
  `issued_at`, `refresh_token`.

## Fixtures

| File | Source (page → section) | Transformations | Kind |
|---|---|---|---|
| `token-spec-unauthorized.json` | Token page → "How to authenticate", the registry's `401` error body | The documented example body as the fetch tool quoted it (one line); whitespace may differ from the page | documented example |
| `token.json` | Token page → "Token Response Fields", example response | The page's token value replaced by `synthetic-registry-token-pull-0001`; `expires_in` 3600 and `issued_at` `2009-11-10T23:00:00Z` kept | documented example, token synthetic |
| `token-access-token.json` | Token page → the `access_token` alias | Authored: only `access_token` and `expires_in` (300) | synthetic |
| `token-missing.json` | Token page → "at least one of these fields must be specified" | Authored to break that rule | synthetic negative |
| `manifest-docker-v2.json` | Manifest V2 Schema 2 page → "Image Manifest" field list and example | Field set and order of the page's example (pretty-printed; the page's exact indentation was not checked); one layer instead of three; digests and sizes are those of `config.json` / `layer.bin` (computed with `sha256sum` when authoring) | synthetic in documented shape |
| `manifest-oci.json` | OCI image-spec `manifest.md` → properties; media types | Compact JSON, one layer, one annotation `org.example.fixture` | synthetic in documented shape |
| `config.json` | — | Authored minimal image configuration (90 bytes); its content is never parsed by the SDK | synthetic |
| `layer.bin` | — | Authored text (73 bytes); **not** a tar archive: blobs are opaque to the SDK | synthetic |
| `catalog-page-1.json`, `catalog-page-2.json` | API page → "Listing Repositories" body `{"repositories": [...]}` | Authored names, lexically ordered across the two pages | synthetic in documented shape |
| `tags-page-1.json`, `tags-page-2.json` | API page → "Listing Image Tags" body `{"name", "tags"}` | Authored tags | synthetic in documented shape |
| `tags-null.json` | API page tag body | `"tags": null` (an empty repository's answer some registries give) | synthetic edge case |
| `tags-other-name.json` | API page tag body | `name` differs from the requested repository | synthetic negative |
| `error-*.json` | API page → "Errors" (body shape) and the error code table | Codes from the table; messages modelled on the table's message column but hand-written (see below); `detail` values authored (`error-manifest-blobs-unknown.json` follows the manifest-push example: one `BLOB_UNKNOWN` per missing blob with `detail.digest`) | synthetic in documented shape |
| `error-unknown-code.json` | API page → "treat unknown codes as UNKNOWN" | `TOOMANYREQUESTS` plus an invented `VENDOR_SPECIFIC` code | synthetic |
| `proxy-error.html` | — | An HTML page as a proxy in front of a registry might send | synthetic negative |

`.gitattributes` keeps `fixtures/** -text` so the bytes (and therefore the digests the tests
compute from them) survive checkouts on Windows.

## Synthetic, by design

- **Error messages.** The fetch tool paraphrased the error table, so the exact message strings in
  `error-*.json` are hand-written approximations. No test asserts a message except the token
  page's documented `401` body.
- **Tokens and credentials.** All tokens (`synthetic-registry-token-*`, `synthetic-fake-token-*`)
  and the Basic credentials (`fixture-user` / `synthetic-password-not-a-secret`) are readable
  labels, valid nowhere.
- **Upload URLs.** The `Location` values (`/v2/<name>/blobs/uploads/p<id>-g<generation>?_state=…`
  in the fake, `u1?_state=AAA%2Bb%3D%3D&x=1` and similar in scripted tests) are invented to be
  opaque and to change after every chunk; Distribution's real `_state` is a signed blob, not
  reproduced here. The `Docker-Upload-UUID` values deliberately differ from every path.
- **The fake registry and token service** (`tests/support/mod.rs`) implement the documented
  behaviour listed above. Two behaviours are the fake's own choices, not the spec's: a status
  query may use an earlier `Location` of the same session (the answer that moved it was lost),
  and a manifest whose type the request's `Accept` does not list is answered `404
  MANIFEST_UNKNOWN`. It routes on the percent-decoded path (as Go's `net/http` does); this was
  not verified against a running Distribution server.
- **Tag grammar** (`[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}`) is SDK policy taken from the Distribution
  reference grammar as commonly documented; it is not on the pages above and was not re-checked
  this session.
- **Not vendor conformance.** Passing these tests says the SDK follows the pages as read on the
  access date against fixtures and fakes written from them; it says nothing about how a given
  registry deployment behaves.
