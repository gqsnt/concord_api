# Maintenance change — field rename, unknown field, reserved-character offset

## Ticket and starting point

Ticket: "Change field and use reserved characters in offset". Provider-brief perturbation: change
a table field name and include an unknown field while an offset contains reserved characters;
verify caller and encoding behavior.

Starting point: branch `sdk20/w4-sdk14-airtable` at `173e4f01` (author commit `e0425cc8`,
consumer commit `173e4f01`), Concord frozen at `0983a349`, clean worktree. Requested runner:
Claude Opus subagent (task capsule). Observed model and effort are not recorded here: they must
come from harness metadata, not from this session's account of itself. I did not write this SDK.

## What I read, in order

1. The task capsule.
2. SDK: `src/lib.rs`, `README.md`, `PROVENANCE.md`, `Cargo.toml`, `.gitattributes`, every
   fixture under `fixtures/` and `fixtures/first_use/`, `tests/protocol.rs`, `tests/workflows.rs`.
   From `tests/first_use.rs` I read only the helpers and the field-selection test, to find which
   literals pin the contract.
3. Harness: `examples/testserver/src/lib.rs` (`Recorded::target`, `query`, `query_all`, `decode`).
4. Concord: `docs/AUTHOR_GUIDE.md` §4.2 (query placement order), §6 (`Display` wire formatting)
   and §7 (sensitive values); `docs/CALLER_GUIDE.md` §8 (what is redacted); the `Debug` impls of
   `Resume`, `PagesError` and `Incomplete` (`crates/concord-pagination/src/types.rs`, `pages.rs`)
   and of `ResponseMeta` (`crates/concord-core/src/response.rs`), to check whether an offset can
   be printed.
5. Airtable documentation, fetched 2026-11-06 (no version or update label on the developer
   pages): [List records](https://airtable.com/developers/web/api/list-records),
   [Errors](https://airtable.com/developers/web/api/errors),
   [Field model](https://airtable.com/developers/web/api/field-model) and the
   [Getting started](https://support.airtable.com/articles/6292134965-getting-started-with-airtable-s-web-api)
   article ("Last updated 2 months ago"). Findings: `fields[]` takes "names or IDs"; formulas
   may use field names or IDs; `returnFieldsByFieldId` defaults to false, so response keys are
   field names; `commentCount` appears only with `recordMetadata`. None of these pages describes
   field renames, or what Airtable returns for an unknown field name in `fields[]` or a formula.
6. Only after the first attempt passed: `FRICTION.md` and `FIRST_USE.md`. Neither changed the
   work. FRICTION item 6 already notes that the author's workflow test covers a renamed field, an
   unknown nested value and a missing old name across two independent fixtures. The new tests
   add a rename in the middle of one traversal, a selection that still uses the stale name, and
   resuming a saved offset across a rename.

## Finding before editing

The SDK encodes no field name. `Record::fields` is a `BTreeMap<String, serde_json::Value>`,
`ListOptions::fields` and `filter_by_formula` are sent verbatim, and nothing caches a schema. The
offset is a `#[sensitive] query offset?: String` taken from the decoded JSON and passed back
unchanged. `Resume`'s `Debug` prints `At(..)` and `ResponseMeta`'s `Debug` prints header names
only, so neither prints the offset. So the perturbation needed no code change. I made none and
verified the behavior on the wire instead.

## Edits by kind

| Kind | Edits |
| --- | --- |
| Declaration (`concord::api!`) | none |
| Adapter (`Policy`, `Client::list_page`, `Client::list_pages`) | none |
| Model (`Record`, `RecordPage`, `ListOptions`, `Error`) | no code; one rustdoc line on `ListOptions::fields` ("names or field IDs ... Response keys are field names either way") |
| Tests | new `tests/maintenance.rs`, 3 tests; new `fixtures/maintenance/` with 5 hand-authored files (covered by the existing `fixtures/** -text`) |
| Docs | one README paragraph (renames, unknown fields, stale names, restart rule, offset encoding); two `PROVENANCE.md` rows; this file |
| Existing tests | none edited; all still pass |

The new tests, all against the loopback fake:

- `rename_between_pages_passes_new_and_unknown_fields_and_returns_offset_exactly`: page 1 has
  `Status` and an offset whose JSON spells `/`, `é`, `"` and `\` as escapes. Page 2 has the
  renamed `Stage [v2] / #1 & 50%+`, an unknown collaborator-shaped object field and an unmodelled
  `commentCount`. The test asserts the old key is absent, the new and unknown values are kept
  exactly, the traversal ends `Exhausted`, and the second request target is exactly
  `offset=itr%2Frec%3A%40%21%24%26%27%28%29*%2B%2C%3B%3D%3F%23%5B%5D%25+%252F%7E%C3%A9%22%5C`
  (it decodes to the JSON-decoded offset).
- `stale_name_in_selection_fails_typed_with_exact_offset_and_restart_uses_new_name`: the
  selection and formula still use `Status`, and page 2 is a 422. The result is `Incomplete` with
  page 1's record and `PagesError::Provider { failed: Resume::At(offset) }`, where `offset` is
  exact; the error is `Error::Http` with status, header and provider type kept. The
  `Debug`/`Display` of the error and of `Incomplete` contain neither the raw offset, nor its
  encoded form, nor the provider message. The caller then restarts from `Resume::Start` with the
  new name in `fields[]` and the formula, and the new name's encoding is pinned.
- `id_selection_survives_rename_and_resumes_saved_offset_but_keys_follow_names`: the selection
  and formula use synthetic field IDs. The page cap is reached before the rename, and the saved
  offset is resumed after it with unchanged options. The test asserts that both requests carry
  the same `fields[]`, that no `returnFieldsByFieldId` is sent, that the offset comes back
  exactly, and that response keys are the new name.

My predicted raw encodings for the offset and the renamed field name matched on the first run.
Mutation check (reverted): making the adapter pre-encode `%` in the offset
(`o.replace('%', "%25")`) fails all three new tests.

## How many independent places encode the same fact

- **A field's name.** SDK source: zero places. In a caller, one rename has to change three
  independently written places together: the `fields` selection, the `filter_by_formula` text,
  and every `record.fields["..."]` read. The SDK cannot keep them consistent and cannot detect a
  rename: the old key's absence looks the same as an omitted empty value. In the new tests the
  renamed name appears in three fixture files and two constants (raw and wire-encoded).
- **"Response keys are field names."** Four prose descriptions, none checked by the compiler: the
  `Record::fields` rustdoc, README paragraph 1, the new README paragraph and the amended
  `ListOptions::fields` rustdoc. Only the third test pins it behaviorally, by asserting that
  `returnFieldsByFieldId` is not sent. So in docs, yes: one semantic fact has several
  independently maintained descriptions. This change had to touch two of them (one amended, one
  added) and left README paragraph 1 alone because it is still true.
- **"The offset is passed back exactly."** Five code places, none of which changed: the
  declaration (`#[sensitive] query offset?`), the `present_offset` deserializer, the
  `EmptyOffset` check in `Policy`, the `InvalidInput` check in `list_page`, and the `Pages`
  projection `next: response.value.offset`.

## Generated-name or compile surprise

No generated-name surprise: no generated setter (`set_fields`, `maybe_offset`, ...) was touched.
No compile diagnostics in the new code; rustfmt reflowed two statements in the new test.

There is one compile hazard on the obvious follow-up, observed with a reverted probe. The rename
problem's documented remedy is `returnFieldsByFieldId=true`. Adding it as a public
`ListOptions` field makes two existing tests stop compiling with `E0063 missing field`:
`tests/protocol.rs:23` and the consumer's `tests/first_use.rs:417` both build `ListOptions` with
every field listed and no `..Default::default()`. `ListOptions` is a plain public struct without
`#[non_exhaustive]`, so every new list option is a breaking change for callers who write it
exhaustively. That is an SDK design property, not a Concord one.

## Proposal, not done

Expose `returnFieldsByFieldId` so that record keys stay stable across a rename. The trade-off:
as a `ListOptions` field it breaks the two literals above (and any caller written the same way);
as a client-level mode (`Client::with_field_ids()`) it stays compatible but places a per-request
parameter on the client. Choosing between them is the owner's decision and is beyond the ticket,
which asks for verification. No caller change was needed for the ticket itself.

## Did the change feel local?

In the SDK, yes. One semantic change (a field rename plus a reserved-character offset) needed
no declaration, adapter or model code, only tests, fixtures and prose. What is not local is on
the caller's side. A rename must be applied consistently to the selection, the formula and the
reads, and an offset saved under the old options must be discarded. The SDK's contract resumes
only with the same options, and the string offset cannot enforce that (FRICTION item 3 makes the
same point).

Observed vs inferred: the wire behavior above is observed against the loopback fake. That a
renamed field arrives under its new name on later pages, and that a stale name in `fields[]` or a
formula is rejected with some 422, is inferred and modelled; the pages read do not document it.

## Commands and results

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-airtable
```

Results: unit 0, `first_use` 14, `maintenance` 3, `protocol` 3, `workflows` 3, doctest 1, all
passing. `cargo clippy --locked -p concord-sdk-airtable --all-targets -- -D warnings` and
`cargo fmt -p concord-sdk-airtable --check` are clean. No frozen path (`crates/`, `scripts/`,
`docs/`, `checks/`, other examples) was edited.
