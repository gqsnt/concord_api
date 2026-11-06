# Independent consumer fixture provenance

These new `first-use-*` fixtures were hand-authored for the independent consumer from
https://distribution.github.io/distribution/spec/api/ and its linked schema-2 manifest page,
https://distribution.github.io/distribution/spec/manifest-v2-2/ (consulted 2026-11-06).
They are not copied vendor examples, captured traffic or Distribution conformance evidence.
No existing SDK fixture was read or reused.

The manifest field layout and media types follow the schema-2 documentation. The configuration
is synthetic JSON. The layer is deliberately opaque synthetic binary data containing NUL,
CRLF and a non-UTF-8 byte, not a real gzip/tar layer or runnable image. This fake checks registry
transport and content addressing, not image runtime validity. SHA-256 values and descriptor
lengths were computed independently using Python hashlib over the exact file bytes, including
final newlines. The error document is invented using the documented error envelope;
FIRST_USE_EXTENSION is an invented unknown-code compatibility case.

Token JSON, catalog/tag JSON and upload/error responses in `tests/first_use.rs` are independently
hand-authored from the API and token specifications. All tokens are visibly synthetic; no real
credentials, registries or token services were used. The existing `.gitattributes` rule
`fixtures/** -text` preserves these fixture bytes across checkouts.

| File | Bytes | SHA-256 |
|---|---:|---|
| `first-use-config.json` | 79 | `sha256:7be3c44c11217c7bf23199c77eb56f0dac90cc4dd638479176b2c88096f2d08e` |
| `first-use-layer.bin` | 47 | `sha256:fd8b9c2d6d9fa601e7a8fa87abb297987bc32433dec51c0f59f09b4df5109f0c` |
| `first-use-manifest.json` | 497 | `sha256:6a3c36094f788db61a012c3f5e41146404f0f74567c0089c192bd8182a839a0b` |
| `first-use-missing-manifest.json` | 197 | `sha256:164e9f552ec2f6cf50970544c484f74a41063ae25adbc92aef54ed651b03bbb4` |
