# P0 capsule: correct stale *current* documentation (preserve historical records)

You edit documentation in the Concord Rust workspace (your working directory, a git worktree).
Read `AGENTS.md`/`CLAUDE.md` if present. You cannot commit; leave the change in the working tree.
Change prose and doc comments only (rustdoc in `crates/*/src/*.rs` is allowed, code is not).

An independent inventory found these current guides/references misdescribing the present API. For
each, read the cited place and the current source it should describe, then correct the guide so it
states the current truth precisely (no new promises). Where a correction concerns a historical
record (`FRICTION.md`, `FIRST_USE.md`, `MAINTENANCE.md`, `v4/**` records), do **not** rewrite the
observation: add a short dated "Current status (2026-11-06): …" pointer instead.

| Location | Problem → current truth |
|---|---|
| `README.md:134`; `crates/concord/src/lib.rs:165`; `docs/CHEATSHEET.md:112`; `docs/RECIPES.md:313–314`, `:408` | Unqualified "one `send()` is one execute/request, no retries". True of `Prepared::send` and the default generated `send()`; a client `send policy;` runs `SendWith<T>::send`, which may sign, make sub-requests or repeat. |
| `docs/CALLER_GUIDE.md:252` | Generated `send` universally `prepare()?.send()`. Only without a send policy; a typed stream output opens its reader without consuming the body. |
| `docs/CALLER_GUIDE.md:300` | Every call returns `Result<_, concord::Error>`. A policy's `SendWith<T>::Output` is the SDK's; `prepare()` keeps Concord's result. |
| `docs/AUTHOR_GUIDE.md:1038` | Incremental consumption directed to `send_raw` + native reads. The routes are `-> stream T` (`Incremental<T>`) or `-> stream` (`Incoming`), §5.9. |
| `docs/RECIPES.md:107` | "Any typed output buffers the whole body." Finite `-> T` does; `-> stream T` opens the reader unread. |
| `docs/CALLER_GUIDE.md:378–379` | Untrusted error bodies via a hand-written chunk-limit loop. `Received::bytes_limited` (any status, bounded, names the operation) and `decode_limited`; a native loop stays a valid escape. |
| `crates/concord/src/lib.rs:104–107`, `:120–130` | Overview omits `Received`, `Incoming`, `Endpoint`, `SendWith`, `Incremental`, `-> stream`, client `send`. Add them briefly where the overview lists the runtime/syntax. |
| `docs/MAINTAINERS.md:10–16`, `:37–39` | Runtime map omits `response.rs` (response state/identity), `prepared.rs` (`Endpoint`/`SendWith`), `codec.rs` (`EncodedBody`, `Incremental`); multipart described only via reqwest (there is the declared/shared `Multipart` format too). |
| `examples/native/README.md:23` | Says `decoder.finish(Err(e))`; current `metadata.rs` uses `decoder.transport_error(e)`. |
| `examples/google-drive-sdk/README.md:262–263` | Says the related-multipart serializer is SDK code; `upload.rs` uses the shared `MultipartBody::related`/`Part`. |
| `examples/open-meteo-sdk/README.md:126–127` | Says an SDK guard rejects unmatched-quote EOF; the guard was removed, shared CSV framing rejects it. |
| `examples/clickhouse-sdk/README.md:158` | `Error::Concord` described as pre-head/upload only; `Insert::send` also returns it for a post-head receipt-body transport failure (then `meta()` is `None`). State it. |
| `examples/distribution-registry-sdk/README.md:233–234` | "Every unexpected 2xx becomes UnexpectedStatus": catalog/tag/token paths only check `is_success`. State the per-operation truth. |
| `examples/ntfy-sdk/README.md:167–170` | NDJSON end-of-body: a complete final JSON value without LF at EOF is accepted; an incomplete one fails. |
| `examples/slack-sdk/README.md:118–119` | Checkpoints "bound to the originating listing": `Cursor(String)` carries no context; the caller must reuse it with the same listing. |
| `v4/sdk20/READOUT.md:117` | "eight SDKs use `decode_limited`": current source has ten (Slack, Linear, Dropbox, Drive, Airtable, Snowflake, World Bank, Notion, Graph, registry). Add a dated correction note; keep the historical text. |
| `v4/sdk20/READOUT.md:446` | Open-Meteo listed among stream readers: its `-> stream` is runtime selection; the forecast result is buffered. Dated note. |

Also check `docs/CUSTOM_FORMATS.md:248–252` and `docs/RECIPES.md:84–87`: add a one-line pointer to
`Received::head`/`Received::meta`/`concord::ResponseMeta` where a reader could think metadata
requires changing the output type.

Verify every claim against the source before writing it (cite symbols in your report). Run
`python3 -I scripts/check-records.py` (default files) and, if you touched rustdoc,
`RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps` (`CARGO_BUILD_JOBS=4`), plus the
quickstart documentation-excerpt test if you touched a file it checks
(`cargo test --locked -p concord-example-quickstart`). Report each change (file:line, before → after
in one line), anything you found to be already correct, and the commands with exit codes.
