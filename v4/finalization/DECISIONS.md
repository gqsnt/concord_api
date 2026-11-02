# Decisions — final V4: native header names, representation statements, Riot

Campaign records: this directory (`STATUS.md`, `MODEL_RUNS.md`, `TEST_MATRIX.md` entries in
`../TEST_MATRIX.md`, `PERF.md`, `FINAL.md`). Earlier decisions (`../native-repr/DECISIONS.md`
N01–N12, `../refinement/DECISIONS.md` D01–D18) stand unless superseded here. Evidence outside the
repository (frozen proposals, review reports, receipts) is under
`/home/canarit/projects/concord_v4_finish_work/` and is cited by file and SHA-256 in `MODEL_RUNS.md`.

Each decision: KEEP / CHANGE / REJECT / DEFER, with the task-driven reason.

## F01 — Body and response formats: uniform statements (CHANGE)

**Decision.** Payload positions name Rust types only (`body x: T`, `body x?: T`, `-> T`). The
format comes from `request F;`, `request F "media/type";`, `response F;` or `format F;` (both),
written by the client, a scope **or an endpoint**; the nearest statement wins per direction. The
in-place forms (`json b: T`, `F("m") b: T`, `body("m") b: T`, `-> F T`) and the owner-only rule
are removed (compile errors with the exact replacement).

**How it was chosen.** Four design families wrote blind first-round proposals from one neutral
brief and corpus (Opus, Sonnet, Sol XHigh, Terra XHigh; Sol Medium wrote a frozen pragmatic
assessment and built the corpus and the control). Three of the four families independently
proposed this model (spelled `body F;`, `format/request/response`, `representations { }`); Sol
XHigh proposed body/response *nodes* inside the endpoint block with atomic recipes, plus a
"contracts" alternative. Two materially different candidates were implemented as macro patches —
uniform statements (by Sol Medium, not an author family) and nodes (by Opus, not the author
family) — and run on the same 38-test corpus with the control: all three behave identically
(36 pass; 2 failures by design, unrelated to bodies). Three blind cross-reviewers (Terra XHigh, Sol
XHigh, Sonnet) compared them under neutral labels: Terra and Sol ranked nodes first *as taste*,
Sonnet ranked statements first with compiler-fact evidence.

**Why statements.** The evidence that separates them is executable:
- with nodes the written endpoint signature no longer equals the generated callable signature
  (`POST create()` takes the body; making a body required changes callers while the declaration
  line stays the same) — named as the strongest counterexample by all three reviewers; the
  owner's brief asks for stable Rust-like signatures;
- nodes accepted an endpoint policy with no node silently, needed a codec import in every module
  (with no import hint in the error), could not state built-in media types in rustdoc, and cost
  +912/−608 macro lines against +162/−113;
- statements reject a statement with nothing to apply to, give compiling replacement messages,
  keep the `body` placement consistent with `query x`/`header(K) x`, and keep words that need no
  import and survive a renamed dependency.
Kept from the node design and the reviews: a statement replaces format **and** media type
together (atomic, as the node recipes); the unsupported-codec error names `Encode<T>`/`Decode<T>`
with Concord's message instead of a `BodyFormat` mismatch; the response-media error explains why;
the endpoint's own type documents the request it builds.

**Rejected.** Nodes (above). Contracts (`Representation<T, Codec>` aliases, no lexical defaults: a
mostly-JSON API restates every payload's contract; its substrate compiled in Sol XHigh's probes).
Profiles (atomic request+response bundles: combinatorial growth for one-off exceptions). Raw kinds
(`-> text` in type position: two mechanisms to teach). Representation as an inherited key with
`override`/`remove` (forces cleanup edits on every move). Keeping in-place forms as aliases (two
spellings of one rule). The previous in-place control (asymmetric: exceptions in place, defaults
only on owners; a media-only form that inherits the format).

## F02 — Endpoint statements need something to apply to (KEEP, refined)

`request` without a body and `response` without `->` on an endpoint are compile errors; `format`
on an endpoint needs at least one of the two (executed friction in the candidate run: `format`
on a body-only endpoint was an error by the first rule). Unused statements on clients and scopes
stay legal (they resolve nothing, so a format behind a disabled feature costs nothing).

## F03 — Clearing a default (REJECT)

No corpus or maintenance task needs it: a heterogeneous subtree puts its statements on its uniform
scopes or endpoints (the corpus `Mixed` client has none at the root). Clearing (`codec _` in the
node prototype, `request none;` in a 20-line prototype) adds a third state to every reader.
It does not remove the hazard reviewers found in all three surfaces — moving a `String` output
under a raw-text default compiles and changes the wire meaning — which is addressed by docs (the
effective format and its origin on every endpoint, and the author guide's advice to state a moved
group's formats).

## F04 — Format words and paths (KEEP)

`json`, `text`, `bytes`, `form` stay words (hygienic `$crate` markers: no import, renamed
dependency safe, concrete media types in rustdoc); any other format is a Rust path resolved by
rustc. A bare lowercase non-word is an error suggesting `self::name`.

## F05 — Declared media types are parsed by `mime` (CHANGE)

The macro's own media-type grammar is replaced by the `mime` crate's parser (already in the lock
graph via reqwest) plus `http`'s check that the value can be a static header value. `mime` is
slightly more lenient than the old grammar (it accepts `a/` and a trailing `;`); its acceptance is
the rule.

## F06 — Native header names (CHANGE)

A header key is a literal name or a Rust path to a native `HeaderName` constant or static, in
direct placements (`header(AUTHORIZATION) token: String`) and block entries
(`ACCEPT = ".."; override AUTH = v; remove "authorization";`). No Concord key wrapper; no name
table; the identifier of a path is never read.

**Identity.** The macro knows literal/literal pairs (normalized by `http::HeaderName::from_bytes`)
and identical paths; any pair involving a native constant is unknown (const comparison of
`HeaderName` is impossible on stable: E0015/E0493, reproduced by three designers). Chains with
literal keys only are composed by the macro, as before. A chain with a native constant is composed
when the request is built by `__private::HeaderPlan`, on names and presence flags only — no value
is converted or formatted to find its header. Errors the macro can prove (three-valued analysis:
same/different/unknown) stay compile errors; the others are `Build` errors from `prepare()` naming
the declaration. Strategy chosen over the alternatives the designers prototyped: per-pair inline
residual comparisons (larger generated code, same result) and a cached interpreter of the whole
request (unnecessary). `tests/header_plan.rs` checks the plan against the sequential rule on every
chain of up to four contributions over three names (~838,000 cases).

**Known limit.** Two key paths with identical tokens are treated as the same header even under
different `macro_rules` hygiene (a proc-macro cannot observe hygiene).

## F07 — HTTP validity belongs to `http` (CHANGE; supersedes N03's text rule)

Literal names: `http::HeaderName::from_bytes` at expansion, lowered to `from_static` of the
normalized name (the downstream `http` checks the static again). Literal values and each decoded
literal template fragment: `http::HeaderValue::from_bytes` at expansion (a rejected byte in a
fragment cannot be repaired by a hole); visible-ASCII literals become `from_static` statics, other
accepted literals use the checked runtime constructor (no narrowing to ASCII). Formatted and
`String` values: `HeaderValue::from_str` once. Concord's token table and visible-ASCII rule are
removed: UTF-8 text beyond ASCII is now accepted where `http` accepts it (a deliberate break from
N03). `http` and `mime` are host dependencies of the proc-macro, resolved to the same versions as
the runtime in one lockfile.

## F08 — Header values: a sealed `ToHeaderValue`, no `TextHeader` (CHANGE; supersedes N01/N02)

`ToHeaderValue` is sealed: `HeaderValue`, `str`/`String`, integers, `bool`, and pointers to them.
`TextHeader` and `header_text*` are removed. The only type that can carry its own sensitivity flag
is `HeaderValue`, which has no `Display` and is never text, so the confirmed lead "a natively
flagged value also used as text loses its guard when its header contribution is removed" cannot be
written, and the unsealed allowlist is gone. A domain type's header text goes through a template.

## F09 — Sensitivity by resolved name (CHANGE)

The compile-time fixpoint treats each native constant as a header of its own, off the safety list
(a lower bound). A value it leaves unclassified that is sent under a native constant gets
run-time conditions: the resolved name is on the safety list, or its key equals that of a
sensitive contribution. Generated `Debug` evaluates them when rendering (`__private::Withheld`);
outgoing flags come from the header plan. A conditional value that is also text is withheld
statically (URL redaction is static). One list of sensitive names and one of reserved names exist
in the macro and the runtime, kept equal by a unit test.

**Amended (`0e56a65`, after the compile measurement).** The condition "its key equals that of a
sensitive contribution" was emitted for every other native key that might be sensitive, including
keys that might be sensitive only by their own name. Generated code then grew with the square of
the native keys on a chain: the deep native suite expanded to 34,816 inline name comparisons, and
compiled 11× slower in release. A condition is now added only when it is not implied by the value's
own conditions. When the two keys name one header, the other key's name being on the list means
this key's name is on it too, which is the value's own `Name` condition. The deep suite drops to
1,088 comparisons (expansion 7.7 MB → 2.1 MB). The rule is the documented one, and no value it
withholds is shown. A value whose native key only aliases a *public* header was withheld before
by over-approximation and is now shown, as the generated docs and §7 always said
(`native_keys::transitive_classification_follows_resolved_names`,
`deep_native_chain_withholds_only_the_aliased_level`). The list gained seven vendor secret names
(`api-token`, `x-api-token`, `x-access-token`, `private-token`, `x-goog-api-key`,
`ocp-apim-subscription-key`, `x-amz-security-token`); it remains a safety net.

**Amended again (after re-measuring `d3e26e4`).** The conditions read only constants (the names
key paths hold), but they were still written inline in every endpoint and every `Debug`. In the
shared suite, 200 endpoints repeated them (16,452 comparisons). Each value's conditions are now
one generated function with a `OnceLock<bool>`, evaluated once per process (252 comparisons). Each
condition term is also gated by the `cfg` of its key's accessor. A key written only in compiled-out
owners has no accessor, and a `Debug` that named it failed to compile (`cannot find function
__concord_…_key_N`, a latent defect since F06; `native_keys::native_conditions_follow_cfg`). Such a
contribution does not exist, so its term is `false`.

## F10 — The regex migration script is withdrawn (CHANGE)

The script migrate_via.py corrupted string constants, comments and header literals and merged two
endpoints on one line (reproduced on the campaign corpus). Migration is manual, guided by the
compiler's exact replacement messages (`docs/MIGRATION.md`). A future tool would have to work on
tokens inside `api!` only, show a diff and refuse ambiguity.

## F11 — The Riot example (KEEP as specified)

`examples/riot`: account-v1, summoner-v4, champion-mastery-v4, champion-v3, match-v5,
tournament-stub-v5 (three body-bearing operations), and a separate credential-free Data Dragon
client. One route type per published route list (they differ per operation); no platform→region
mapping. `X_RIOT_TOKEN` is a native constant marked `#[sensitive]`; RSO `/accounts/me` removes it
and sends `AUTHORIZATION = "Bearer {token}"` (the docs' curl example). Request budget and `429`
handling are application code. Live mode is opt-in, read-only, bounded; never in tests.
Unverified and recorded as such in `examples/riot/SOURCES.md`: the stub's routing value, the
key-header name in retrieved official text, stub eligibility.

## F12 — Living records (CHANGE)

`scripts/check-records.py` checks `v4/finalization/*.md` and `v4/TEST_MATRIX.md` by default.
Earlier campaigns' records describe their own revisions and are not re-checked against the current
tree.

## F13 — Review-pack reconciliation (KEEP the selected design; bounded additions)

The owner's independent review pack was reconciled item by item against the current code
(`RECONCILIATION.md`). Kept: uniform statements without `body("m")` (F01); empty and whitespace
responses are not JSON `null` (an empty body is reported with a static hint instead); no
format/media-type allowlist (Kubernetes server-side apply sends JSON as
`application/apply-patch+yaml`); no ban on literal `content-type` headers (8b/8c stand).
Added:
- `Error::response(&self)`, which borrows a status error's unread response;
- a public `BodyError::body_source`;
- `#[must_use]` on generated setters and scope navigation;
- static hints in the JSON decoder;
- an `=` boundary explanation and a did-you-mean in the unknown-value error;
- `# Errors` docs;
- a cheat sheet, a declaration-change table and module wrapping in the author guide;
- the compatibility example (`examples/compat`).
