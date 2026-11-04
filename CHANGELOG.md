# Changelog

## 4.2.0

A focused maintenance release: compiler correctness and exact advice text, fair benchmarks and
trustworthy maintained checks, and corrected caller documentation. The statement DSL, the four
crates, the codec contracts and metadata policies, request types, `IntoValue` borrowing and the
Rust 1.99.1 minimum and pin are unchanged; no edit is needed except where an expression fragment
forwarded by your own `macro_rules!` into a type or initializer now means what Rust says
([migration notes](docs/MIGRATION.md#from-41-to-42)). Not published on crates.io; the four crates
share one version and require each other exactly (`=4.2.0`).

### Fixes

- **Conditional `Clone` keeps every bound.** Types that two crates' macros write with the same
  tokens (`$crate::Payload`) are different types; the bounds of an endpoint request's conditional
  `Clone` were deduplicated by their printed text, so the bound of a non-`Clone` one could be
  dropped and the declaration failed with E0277 without any `.clone()`. Each value and body keeps
  its own bound: such declarations compile, and cloning a request that holds a non-`Clone` value is
  still rejected.
- **Type macros are output types.** `-> payload!()`, `-> m::payload![]` and `-> ::c::payload! {}` are
  ordinary payload types; they were read as the removed in-place format. The removed spellings
  around a type macro still report their replacement, and a braced type macro that takes the
  endpoint's block gets a direct hint.
- **Exact advice and generated documentation.** Types in diagnostics and generated rustdoc are
  rendered token by token: literal contents are kept (`Payload<{ count("a :: b") }>` was suggested
  as `"a::b"`, changing a six-byte length to four), a raw name stays raw (`body r#type?: String`,
  not `body type?`), punctuation written apart stays apart and a macro's input is shown token for
  token, so applied advice selects the same macro arm. A payload holding a `macro_rules!` fragment
  is named `T` instead of spelled.
- **Forwarded `macro_rules!` fragments keep their grouping.** Value, body and output types and
  value initializers that contain an expression fragment (`Payload<{ $e * 2 }>`, `= $e * 2`) mean
  what Rust says (6 for `$e = 1 + 2`, where 4.1 computed 5, or named different types for the body's
  format and the signature); a forwarded `dyn A + B` behind `&` is accepted.

### Maintenance

(filled in at integration: benchmark equivalence, examples, workflow guard, mutation helper,
metadata-stage test, documentation)


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
a file, and every gate step runs `python`, `cargo` and `tar` behind a wrapper that measures each
command line as Windows would quote it (UTF-16 units) and refuses one over the limit on all
platforms. CI now runs the same complete inventory (packages and benchmarks included) on Linux and
Windows; doing so exposed two more Windows-only script defects, also fixed: GNU tar read the
`D:\...` archive path as a remote `host:path`, and a carriage return from Windows Python ended a
path read with `read`.

### Tests and checks added in this release

- `checks/consumers/facade-gzip`: an application with reqwest's gzip support, against compressed
  fixtures, a withheld body and a decompression-disabled control.
- `crates/concord/tests/accept.rs::no_suggestion_keeps_the_metadata_checks`.
- `scripts/argv-guard.sh` and `scripts/argv-length.py` (with a self-test gate step), installed by
  `scripts/verify.sh`; `scripts/check-ci.py` refuses platform-selected gates and `tar -f NAME`.
