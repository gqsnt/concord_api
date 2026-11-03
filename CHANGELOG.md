# Changelog

## 4.1.0

The modular V4: four crates, formats as ordinary Rust types, and response metadata checked before
a body is read. Not published on crates.io; depend on a checkout or a git revision. Rust 1.99.1 is
both the minimum and the pinned toolchain. Upgrade steps are in
[`docs/MIGRATION.md`](docs/MIGRATION.md) (the first two sections).

### Crates

- `concord`: the facade you depend on; `api!`, re-exports, features `json` and
  `reqwest-defaults` (default), `form`, `extras`.
- `concord-core`: the runtime and the open `Encode`/`Decode` contracts.
- `concord-macros`: the declaration macro. It knows no format: it resolves each format as a type
  path, with the same rules as any other path.
- `concord-extras`: the standard formats (`Json`, `Form`, `Text`, `Binary`) and
  `IgnoreContentType<F>`, written against the public traits only.

The four crates share one version and require each other exactly (`=4.1.0`); update them together.

### Changes

- **Formats are Rust paths.** `format Json;`, `response my::Csv;`: no format words, no privileged
  formats. A format of yours is named and resolved exactly like `Json`.
- **Strict `Json`.** A response must declare `application/json` or an `application/*+json` type,
  with no charset other than `utf-8`/`us-ascii`, or the call fails with `ErrorKind::Metadata`.
  For a server that mislabels its responses, `IgnoreContentType<Json>` skips only that check,
  explicitly, per endpoint or scope.
- **`Accept` is a fallback.** A typed endpoint whose format suggests a media type (`Json`, `Form`)
  sends it only when the declared headers leave no `accept` value. An `Accept` you declare always
  wins; a client-wide `Accept` configured on your `reqwest::Client` does not (declare it in
  `headers` instead). When Concord sends none, reqwest's own default goes out. To stop only the
  suggestion, wrap the format so it forwards every decoding step and suggests nothing
  ([format guide, section 8](docs/CUSTOM_FORMATS.md#8-the-accept-suggestion)).
- **Metadata errors keep the response unread.** A refusal by `check_content_type` or `context`
  happens before Concord reads the body; `Error::into_response()` returns the response with its
  body still available.
- **`Client::validate_header_names()`** checks every native header name an endpoint could send,
  without building a request or any value. Its guarantee is limited: a reported endpoint cannot
  build any request, but `prepare()` returns an earlier failure first (body encoding, host, path or
  query formatting), and a passing check says nothing about values.
- **Decompression stays reqwest's.** Enabling reqwest's `gzip` (or other) feature in your
  application works with every format: the format sees the head reqwest exposes and the unpacked
  body. Concord enables no compression feature itself.

### Windows

No library behaviour differs on Windows. The verification scripts did: `scripts/consumers.sh`
passed about 50 KB of `cargo metadata` output as one command-line argument, beyond Windows'
32,767-character limit, so the gate failed there while passing on Linux. It now passes the data as
a file, every gate step runs behind a wrapper that enforces the Windows limit on all platforms, and
CI runs the same complete inventory (packages and benchmarks included) on Linux and Windows.

### Tests and checks added in this release

- `checks/consumers/facade-gzip`: an application with reqwest's gzip support, against compressed
  fixtures, a withheld body and a decompression-disabled control.
- `crates/concord/tests/accept.rs::no_suggestion_keeps_the_metadata_checks`.
- `scripts/argv-guard.sh`, installed by `scripts/verify.sh`.
