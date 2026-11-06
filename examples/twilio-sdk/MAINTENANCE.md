# SDK03 maintenance: optional Unicode and reserved form value

Date: 2026-11-06. Library baseline: `94d13527`; starting worktree HEAD:
`96ff9b2b` (SDK author and independent consumer commits on that baseline).
The worktree was clean before this change. All edits are under `examples/twilio-sdk/`.

The supplied SDK already supported optional `MessageIntent`, with a synthetic
Unicode/reserved-character fixture. This maintenance adds a distinct optional
`StatusCallback` form field, rather than claiming the existing implementation as
new work. Twilio's [Message resource documentation](https://www.twilio.com/docs/messaging/api/message-resource#request-body-parameters),
opened on the date above, documents the optional URL and shows the wire spelling
`StatusCallback` in its request-body example. The SDK sends the original string
through Concord's public `Form` encoder without URL validation or normalization.
Callback reception and signature verification remain outside this slice.

## Edits by kind

| Kind | Edits |
|---|---|
| Declaration | None. `wire::TwilioApi` already declares `body message: CreateMessage` with `request Form;` and JSON responses. |
| Adapter | None. Public create/prepare methods, execution, errors, metadata and authentication keep their existing paths. No custom encoder, private reach-through or additional native execution was introduced. |
| Model | `src/models.rs`: add `status_callback: Option<String>` with omission for `None`, initialize it to `None` in `CreateMessage::new`, and add `with_status_callback`. The existing PascalCase serde rule supplies `StatusCallback`. The response model and redacted `Debug` implementation need no edits. |
| Tests | `tests/protocol.rs`: add one loopback wire test with five POST cases and a subsequent GET; extend the existing diagnostic test with a populated callback. Add three hand-authored `.form` fixtures. Existing first-use and workflow test sources and existing fixture bytes are unchanged. |
| Docs | README scope, compiled example, optional-value behavior and wire evidence; PROVENANCE source check and three fixture entries; this maintenance record. The existing `.gitattributes` rule `fixtures/** -text` already covers all three new files. |

The wire cases cover a Unicode URL with `&` and literal `+`, omission on a later
plain POST, coexistence with the existing Unicode `MessageIntent`, an intent-only
POST, and an explicit empty callback value. Full raw body fixtures establish
encoding independently of the SDK encoder; decoded pairs additionally confirm
that the callback remains one field. The same test checks methods, targets,
authentication, media headers, byte counts and a bodyless GET. Diagnostics keep
the callback destination out of `Debug` output. No callback host is contacted.

## Independent descriptions of the fact

Production has one schema owner, `CreateMessage`, but three code sites needed
edits: the optional field/serialization rule, constructor default and fluent
setter. There is no separately maintained Concord declaration or adapter mapping
for the new field.

Across code, tests and user documentation, six files independently describe the
field contract: the model, protocol test, three expected-byte fixtures and README.
PROVENANCE and this record add two evidence files, for eight changed/added files
in total. Thus one semantic addition does touch several independently maintained
descriptions, even though its production implementation is local to one file.
The Unicode callback value is deliberately repeated in the wire/diagnostic test,
the two populated fixtures and documentation; expectations are not generated
from the model or encoder.

## Reading and discovery

Read the SDK's README, PROVENANCE, declaration/public wrapper, request/response
models, protocol/workflow tests, shared test helpers and relevant first-use
consumer sections to locate the field storage and wire expectations. Also read
the root README, author guide body/form sections, caller guide, form recipe,
core/extras README files and generated public crate/Form rustdoc. No other
provider SDK supplied this maintenance solution; the README's existing Stripe
and GitHub credits describe the original author's work.

`FRICTION.md` and `FIRST_USE.md` remained unread throughout this maintenance task,
including the first implementation attempt. Existing executable first-use tests
were read and run; those filenames are distinct from the withheld reports.

There was no generated-name or compilation surprise: the existing generated
create operation accepts the extended model unchanged, and the new fluent method
is handwritten. The requested `csv,jsonl` rustdoc build succeeded but did not
include `Form`; an attempted read of `concord/struct.Form.html` reported
`FileNotFoundError`. Regenerating with `csv,jsonl,form` exposed the public `Form`
rustdoc. The initial formatting check requested one line wrap in the diagnostic
test; package-scoped `cargo fmt` applied it. Neither required a library workaround
or new friction entry.

## Verification and locality

Every cargo command used this prefix:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a
```

- Baseline `cargo test --locked -p concord-sdk-twilio`: 46 passed.
- First implementation `cargo test --locked -p concord-sdk-twilio`: 47 passed
  (22 first-use, 19 protocol, 4 workflow, 2 README doctests), no failures.
- `cargo doc -p concord --no-deps --features csv,jsonl`: passed; repeated with
  `--features csv,jsonl,form` to read the public form documentation.
- `cargo fmt -p concord-sdk-twilio -- --check`: one wrapping diagnostic initially;
  `cargo fmt -p concord-sdk-twilio` applied the fix; final formatting check passed.
- Final `cargo test --locked -p concord-sdk-twilio`: all 47 passed again after
  formatting and documentation edits. `git diff --check` also passed.
- `git check-attr text` reports `text: unset` for all three new fixtures.

The change felt local in production: one model file, no execution plumbing,
dependency, lockfile, frozen-library or other-example changes. All existing
workflow and first-use calls compile and pass unchanged. These are deterministic
loopback encoding checks with synthetic auth and hand-authored fixtures, not live
provider verification or proof of acceptance of the Unicode/empty callback URLs.
