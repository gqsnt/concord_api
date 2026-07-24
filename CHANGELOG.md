# Changelog

## [Unreleased]

### Added

- A single `concord` application façade with generated clients, explicit
  endpoint values, runtime endpoints, and one managed executor.
- Named generated client builders, inherent request terminals, safe endpoint
  descriptions, and sanitized request planning.
- Finite public limits for endpoint identities, authentication placement and
  material, encoded authentication output, multipart structure and metadata,
  runtime endpoint shape, rate-limit structures, provider traffic, response
  acquisition, and pagination.
- First-party JSON, record codecs, pagination policies, rate limiting, and
  OAuth2 client credentials as downstream implementations of public semantic
  contracts.

### Changed

- Built-in credential constructors are fallible and bounded before storage;
  generated static construction and replacement validate before slot mutation,
  and protected-string clones share immutable storage.
- Custom `CredentialMaterial` no longer requires `Clone`; Concord shares ready
  material through private slot storage.
- `concord::api!` is the sole application declaration entry, including for a
  renamed façade dependency.
- Generated declarations, runtime endpoints, and core authentication plans
  enforce one shared 64-placement hard limit.
- `EndpointId` validates borrowed input before allocation.
- Bearer, Basic, Base64, and authentication-query construction use checked
  arithmetic and reject excess output before native request mutation.
- Multipart part and recipe construction is fallible. Factory output is
  revalidated at the private transport boundary.
- Dangerous raw-response methods are emitted directly by the declaration
  compiler only when the calling façade passes the feature capability.
- Generated direct methods, explicit endpoint values, runtime endpoints, and
  every pagination page use the same lifecycle and completion protocol.
- External dependencies use compatible semver requirements; Concord sibling
  packages retain exact version requirements.

### Removed

- Direct application paths through implementation crates, generated request
  extension traits, generated preludes, positional secret inputs, alternate
  executors, native Reqwest escapes, and runtime pagination.
- The façade `advanced` namespace, the core prelude, and the public benchmark
  bridge and partial-lifecycle conversions.
- Unbounded or partially managed response acquisition.

### Safety guarantees

- Construction and planning remain inert.
- Authentication and native transport values remain private and redacted.
- Authentication recovery permits at most one additional attempt.
- Pagination is sequential, finite, cycle-detecting, first-excess
  withholding, and all-or-error.
- Completion remains exactly once, including cancellation-sensitive paths.

All packages remain `publish = false`. Publication requires separate approval
of legal metadata, repository identity, package-name ownership, and the exact
publish operation.
