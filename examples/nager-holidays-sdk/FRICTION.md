# SDK01 author friction and evidence

Baseline: frozen candidate `94d13527`; Rust 1.99.1, edition 2024. Only this SDK,
workspace membership and its lock entries were edited. Entries below record actions,
observable behavior and remaining adapter work. None required a library patch or a
private API. References are relative to this crate unless prefixed with `../../`.

To reproduce a protocol entry, run the named test with the required environment:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-nager-holidays --test protocol TEST_NAME
```

1. **Ordinary declarations covered the complete wire operation.**
   **Cause/layer: SDK declaration using Concord as intended; positive result.**
   `src/lib.rs:15` declares the hosted base, JSON format and one country-then-year
   operation. No query adapter, encoder, context, authentication wrapper, pager or
   calendar abstraction was needed. Concord supplied JSON Accept, one-segment percent
   encoding, header validation and an empty GET body. Reproduce with
   `tests/protocol.rs:13` (`v4_wire_and_documented_array`),
   `tests/protocol.rs:50` (`country_is_one_encoded_segment_and_synthetic_400_keeps_exact_body`)
   and `tests/protocol.rs:346` (`dot_segments_fail_building_without_sending`).
   Observed targets include `/api/v4/Holidays/AT/2026` and
   `/api/v4/Holidays/A%2FT%20%3F%23%25%C3%A9/0`; `..` fails before any request.

2. **SDK metadata/errors still need an additional caller method beside generated send.**
   **Cause/layer: SDK policy at the Concord response boundary; remaining author/caller work.**
   `src/lib.rs:47` adds concrete `.fetch()` because generated `.send()` has the fixed
   `Result<Vec<Holiday>, concord::Error>` output. The SDK path needs `HolidayResponse`
   and a classified SDK `Error`. The implementation is one `prepare`, one `receive`,
   one metadata capture and one `decode`; it does not rebuild execution. This follows
   `../../docs/AUTHOR_GUIDE.md:1196` and the existing Stripe sender pattern.
   `README.md:93` and `README.md:127` document the two caller paths. Reproduce the
   public SDK path with `tests/protocol.rs:13` (`v4_wire_and_documented_array`) and
   `tests/workflows.rs:27` (`workflow_b_retrieves_different_country_year_and_filters_locally`,
   using `--test workflows`). Result: typed errors and headers are available, but an
   SDK author still writes and explains a second terminal method for an ordinary GET.

3. **Status and headers must be captured before finite decoding to survive failures.**
   **Cause/layer: Concord ownership/error contract, adapted explicitly by the SDK.**
   `src/lib.rs:58` receives the unread response and `src/error.rs:20` clones its head
   before decoding. `src/error.rs:81` returns that actual status rather than treating
   `concord::Error::status()` as an all-stage response status. The latter is Status-only;
   on a decode failure the body is gone, so capturing afterwards cannot recover its
   head. Reproduce with `tests/protocol.rs:175`
   (`invalid_dates_missing_fields_v3_wrong_root_and_truncation_are_decode_failures`)
   and `tests/protocol.rs:303`
   (`dropped_body_keeps_head_and_never_returns_a_partial_array`). Both retain HTTP 200
   and the scripted request id despite returning an error. The pre-head disconnect
   test at `tests/protocol.rs:284` retains no head/status. No fallback status or repeated
   request is used. Remaining work: SDK-owned metadata and error forwarding/accessors
   in `src/error.rs:51`, including a value-withholding Debug implementation at line 29.

4. **Summary documentation still directs metadata readers toward the older native path.**
   **Cause/layer: docs discoverability/inconsistent summary; no runtime defect observed.**
   Inspection found `../../docs/CHEATSHEET.md:112` listing Prepared exits without
   `receive()`, and the recipe summary row at `../../docs/RECIPES.md:401` recommending
   `into_parts()` plus handwritten execute/decode for reading response headers. In
   contrast, `../../docs/RECIPES.md:67`, `../../docs/AUTHOR_GUIDE.md:1196` and generated
   `Received` rustdoc show the shorter `receive/head/decode` path. Reproduce by reading
   those locations and comparing `src/lib.rs:58`. Result: used the current public
   boundary; no native execute or manual operation name was written. Library/docs
   files remain frozen. The concern is that the quick summary can send another SDK
   author toward unnecessary reconstruction.

5. **No documented failure schema was available on the two readable official pages.**
   **Cause/layer: provider documentation plus deliberate SDK classification policy.**
   `PROVENANCE.md:9` records both official sources, their sections and access date;
   the linked Scalar page yielded no extracted text and the direct HTML fetch returned
   HTTP 403. The readable pages document successful arrays, not error-body bytes.
   Consequently `src/error.rs:42` classifies HTTP 400 as `InvalidRequest`, other non-2xx
   statuses as `Http`, and keeps Concord's response unread. Reproduce with
   `tests/protocol.rs:50` and `tests/protocol.rs:250`
   (`other_status_keeps_headers_and_unread_non_json_body`). Exact scripted text/HTML
   bytes remain recoverable through `into_concord().into_response()`. No JSON problem
   parser was added. The caller needs that explicit response escape if it wants body
   diagnostics; these synthetic bodies do not establish a provider failure contract.

6. **Model compatibility stayed ordinary SDK/Serde work.**
   **Cause/layer: provider model and SDK compatibility policy; no Concord workaround.**
   `src/models.rs:10` uses `Option<Vec<String>>` for nullable subdivision lists and
   normal Serde unknown-field tolerance. `src/models.rs:46` explicitly maps the six
   type strings while retaining future values; this small domain mapping is not
   HTTP/codec reconstruction. Dates use Chrono instead of handwritten validation.
   Reproduce with `tests/protocol.rs:86`, `tests/protocol.rs:104` and
   `tests/protocol.rs:135`. Result: perturbation preserves all known fields; null,
   empty, populated and omitted subdivision lists decode; a future classification
   retains its original spelling. Required-field failures and legacy-shaped data
   remain errors. Local filters are ordinary iterators (`tests/workflows.rs:27`).

## Verification trace

- Initial `cargo doc -p concord --no-deps --features csv,jsonl` with the required
  environment prefix passed; public Received methods were inspected in generated rustdoc.
- First prefixed `cargo test --locked -p concord-sdk-nager-holidays` exited 101 because
  adding the package/Chrono required updating Cargo.lock. Cargo also reported a read-only
  registry index-cache write warning. No permission change was requested. The authorized
  prefixed `cargo test --offline -p concord-sdk-nager-holidays` used existing cached
  packages, added Chrono 0.4.45 and this SDK to the lock and passed. Its missing-crate-doc
  warnings in the two test files were fixed with crate documentation.
- Final required prefixed `cargo test --locked -p concord-sdk-nager-holidays` passed:
  **13 protocol tests, 3 workflow tests, 4 compiled README doctests**, zero failures.
- Required prefixed `cargo clippy --locked -p concord-sdk-nager-holidays --all-targets
  -- -D warnings` passed without warnings. Required prefixed `cargo fmt -p
  concord-sdk-nager-holidays` completed successfully.
- `git check-attr text` reports `unset` for fixture bytes. No provider calls,
  credentials, library edits, `tests/first_use.rs` or commits were made.

There was no functional Concord blocker in this negative-control slice. The most
substantial remaining work is the extra SDK terminal method, explicit metadata/error
ownership, and finding the current response boundary through mixed documentation.
