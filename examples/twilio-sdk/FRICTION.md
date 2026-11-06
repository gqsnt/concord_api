# SDK03 author evidence

Frozen Concord baseline: `94d13527`. All references below are relative to this
directory unless prefixed with the repository path. Entries record observed
implementation and checks, not a claim of independent consumer validation.
`tests/first_use.rs` is intentionally reserved for that separate role.

1. **Bounded typed HTTP errors still require leaving the response boundary.**
   Cause: **Concord** mechanism gap plus **SDK** body-limit/error policy.
   `src/error.rs:195` calls `receive()` once and preserves the generated operation;
   at `src/error.rs:201` it must take `into_response()` because declared
   `Received::decode()` rejects non-2xx before a provider decoder can consume it.
   `src/error.rs:213` then repeats the Content-Type/context/body/EOF sequence,
   performs a 64-KiB chunk limit and classifies failures. This is the native escape
   and reconstructed generic response-consumption work in this slice; no HTTP
   execution is rebuilt. Reproduction: `tests/protocol.rs:308` (invalid error
   shapes), `:334` (exact byte limit), `:376` (complete JSON but missing transport
   EOF). All passed. The selected error policy retains metadata and a typed reason,
   but discards raw error bodies and underlying JSON/read diagnostics.

2. **The ordinary public call needs a wrapper to return SDK errors and metadata.**
   Cause: **SDK** public-interface policy interacting with **Concord**'s fixed
   generated result. The two operations are declared at `src/lib.rs:33` and `:39`,
   then exposed again at `src/lib.rs:86` and `:91` to return
   `Result<Response<Message>, Error>`. `src/lib.rs:99` also exposes preparation for
   callers who deliberately need native inspection; sending that value directly
   follows Concord's error contract instead. The wrapper prevents ordinary callers
   from remembering a separate generic `send` helper. Reproduction:
   `tests/workflows.rs:24` and `:47` use only `Twilio` methods and retain an earlier
   submission when the fetch fails. Both passed. Maintenance must keep declaration,
   preparation and wrapper methods aligned when adding an operation.

3. **Basic auth is an SDK header adapter, not a declaration operation.**
   Cause: **SDK** / generic protocol placement; not a demonstrated Concord defect.
   `src/auth.rs:20` validates credential structure, encodes `username:password`
   with mature `base64`, and marks the resulting native `HeaderValue` sensitive.
   `src/lib.rs:24` injects it using a normal native header binding. This is manual
   auth setup, but no per-call native request editing or HTTP execution is needed.
   Reproduction: `tests/protocol.rs:14` asserts the exact Basic header, `:129`
   proves the path account is independent, and `:150` checks rejection/redaction.
   All passed. Synthetic strings are used throughout; there was no credential
   acquisition or live authentication.

4. **Flat form encoding and optional-field maintenance fit the existing API.**
   Cause: **Concord** positive result with **SDK** serde field mapping.
   `src/lib.rs:35` selects the public `Form`; `src/models.rs:12` supplies PascalCase
   field names, and `src/models.rs:24` omits only `None`. No custom encoder or native
   body replacement was needed. Reproduction: `tests/protocol.rs:78` adds Unicode,
   ampersand and plus in `MessageIntent`, compares literal fixture bytes, then
   asserts that the baseline POST and GET are unchanged. `:115` distinguishes an
   empty optional value. Both passed. **Provider** caveat: the Unicode intent is an
   encoding stress case; its vocabulary acceptance was not tested.

5. **Metadata and operation identity survive without handwritten endpoint names.**
   Cause: **Concord** positive result. `src/error.rs:26` captures status, version,
   headers and operation before body consumption; `src/error.rs:204` delegates the
   successful body to the declared decoder. `src/error.rs:157` gets the operation
   from either captured metadata or the original transport error. Only assertions
   contain endpoint-name strings. Reproduction: `tests/protocol.rs:270`, `:291`
   and `:403` cover decode, head and no-response failures. All passed. No private
   reach-through, fabricated status, manual execute/finish pair or library patch.

6. **Completion is HTTP decoding; resource failure is still successful data.**
   Cause: **provider** semantics and **application** retry policy.
   `src/models.rs:117` exposes status and processing errors without turning an
   `undelivered` resource into an HTTP error. Reproduction:
   `tests/workflows.rs:24` asserts HTTP 200 with code 30007, and
   `tests/protocol.rs:376` / `:403` prove incomplete and lost POST responses do not
   become success or trigger SDK resubmission. Transport retries/redirects are
   explicitly disabled in the tests' supplied reqwest client. Successful finite
   JSON uses Concord's ordinary buffered decode with no SDK success-body cap
   (`src/error.rs:204`); this limitation is documented in README.md.

7. **A recipe sentence disagrees with the current public API.**
   Cause: **docs**. Repository `docs/RECIPES.md:70` says the final URL is something
   `Received` deliberately does not expose; public rustdoc at
   `crates/concord-core/src/response.rs:182` exposes `Received::url`. Reading the
   public rustdoc resolved this discrepancy. The SDK did not need the URL and
   uses `head()` metadata only (`src/error.rs:26`). No docs edit was made to the
   frozen tree.

8. **Initial lock/dependency setup required one local correction.**
   Cause: **SDK** dependency selection / **application** build environment.
   The first manifest selected base64 0.22; the first locked test attempted an
   index/cache update, printed a read-only Cargo registry-cache warning, then
   rejected the required lockfile change. Inspection found base64 0.23.1 already
   locked. `Cargo.toml:13` now selects that version; one unlocked
   `cargo test --offline -p concord-sdk-twilio` added only this SDK's lock entry.
   The subsequent requested locked test passed. No registry permissions were
   changed. The managed-environment status tool and network-policy file were not
   available in this executor; no credential files were read.

## Verification observed

Every Cargo invocation used `CARGO_BUILD_JOBS=5` and the required shared
`CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a`.

- `cargo doc -p concord --no-deps --features csv,jsonl`: passed.
- First `cargo test --locked -p concord-sdk-twilio`: expected lock-update rejection.
- `cargo test --offline -p concord-sdk-twilio`: lock updated; initial library and
  2 README doctests passed before integration tests had been added.
- `cargo fmt -p concord-sdk-twilio`: passed.
- `cargo test --locked -p concord-sdk-twilio`: **18 protocol + 4 workflow + 2
  doctests passed**, 0 failures. Unit-test binary contains 0 tests.
- `cargo clippy --locked -p concord-sdk-twilio --all-targets -- -D warnings`:
  passed without warnings.
- `cargo doc --locked -p concord-sdk-twilio --no-deps`: passed without warnings.
- Final scope check: only this SDK directory, its workspace member and package
  lock entry differ from the baseline; all 16 fixtures are listed in provenance
  and Git reports their `text` attribute unset. `git diff --check` passed.

Provider documentation was read online; all SDK execution tests used loopback.
