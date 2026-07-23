# Manifest-dependent macro fixtures

These packages remain standalone because their behavior depends on Cargo metadata, dependency naming, feature isolation, or a separate resolved feature graph. Ordinary compile-pass and compile-fail behavior belongs in integration tests and Trybuild instead.

| Fixture | What it proves |
|---|---|
| `renamed_facade` | Generated code resolves the caller-visible alias for the one `concord` dependency. |
| `missing_core` | Omitting the runtime crate produces the focused macro diagnostic. |
| `contract_mismatch` | A deliberately incompatible core contract fails during compilation. |
| `json_disabled` | JSON body/response types do not silently work without the façade `json` feature. |
| `oauth_disabled` | The extras-owned `concord::auth::OAuth2ClientCredentials` type is absent when the façade feature is disabled. |
| `multipart_disabled` | Multipart syntax requires the core `multipart` feature. |
| `custom_retry_disabled` | Custom retry syntax requires `dangerous-custom-retry`. |
| `custom_retry_enabled` | The same syntax compiles when its feature is deliberately enabled. |
| `development_tools_enabled` | Generated clients remain compatible with a core build that exposes downstream development tools. |
| `raw_response_disabled` | Generated endpoints omit the raw-response terminal when the core dangerous feature is disabled. |
| `extension_certification` | Public-only extension implementations compile and execute across their owned feature combinations. |
| `no_governor_rate_limits` | Generated and prepared rate limits fail closed without the governor unless a custom or deliberate no-op limiter is supplied. |

A fixture should be deleted when its assertion no longer depends on a distinct Cargo graph. Empty placeholder fixtures are not accepted.
