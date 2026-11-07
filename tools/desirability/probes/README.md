# I3 reader probes: docs-assisted prediction (R1, R2, R3)

The instrument asks fresh model sessions, which have no tools and no repository, to predict what a
generated Concord call does. They work from packed materials: the cheat sheet, the SDK's
`concord::api!` declaration and the Rust items it names, a call snippet, and (for R2/R3) the SDK's
generated rustdoc as text. It measures **docs-assisted prediction**, not first-sight reading. The
truth comes from the implementation, so a low score means the call is hard to predict, not that the
implementation is wrong.

| Metric | Question (item kind) | Session | Truth |
|---|---|---|---|
| R1 | `wire`: the request the call makes | `wire` (no rustdoc) | the `prepare()` dump, or the request a local receiver recorded (see boundaries) |
| R2 | `result`: what `send().await` returns, its error type, and for a client whose base has a path, which `with_base_url` string keeps the prefix | `rc` (with rustdoc) | the declaration, checked by the compiler; `with_base_url` candidates checked by preparing the call |
| R3 | `call`: write the call for a stated intent, as one expression whose value is the `send().await` result | `rc` | bound to the declared send() result type, the answer must compile (in a sandbox) and, run in a second sandbox, send the same requests (method, path, query, headers, body) as a reference call, with an `Ok` value with the item's reply |

## Observation boundaries

Every item carries `boundary`:

- `prepared`: the client has no send policy. The request `.prepare()` builds is the request
  `send()` sends (cheat sheet: one `execute`, no extra request). The wire question asks about
  `call.prepare()`, and the truth is the `prepare()` dump.
- `operation`: the client has a `send` policy, which may sign the request, make sub-requests or
  repeat it. Here the wire question asks what `call.send().await` puts on the wire. The truth is
  the request that `concord_example_testserver` records when the call goes through the public
  `send()`. The client is built as in the snippet, then `with_base_url(<receiver> + <base path>)`
  points it at the receiver, which answers `200 {}`. If no request arrives, or more than one does,
  the item is dropped from R1 as "operation truth not available". The `prepare()` dump is never
  used for these items. It is kept in the truth file as `prepared`, next to
  `policy_changes_request`.

## Files

| Path | What |
|---|---|
| `common.py` | SDK loading (files, clients, Rust item index, module index of real item paths, aliases, `use concord::..` imports, crates used), a small Rust type parser, type identities (R2), cargo and scratch-copy helpers, `bank_hash` |
| `generate.py` | question generator → `<out>/index.json`, `<out>/<sdk>.json` |
| `truth.py` | ground truth → `<bank>/truth/<sdk>.json`, `<bank>/truth/drops.json`; generated harness modules (main bank: `checks/probe-truth/main/*.rs`) |
| `pack.py` | materials → `<bank>/packed/<sdk>.{wire,rc}.md`, `<sdk>.meta.json` (question → item map, sizes, sources, leak scan) |
| `run.py` | readers (Claude, Codex) → `<out>/<provider>/<sdk>.<session>/` |
| `score.py` | scorer, R3 compile and run sandboxes (bubblewrap) and `--selftest` (seeded controls with expected rates, sandbox effect checks) → `score.json`, `score.md` |
| `form-check/` | the R3 answer-contract parser: a small Rust binary on `syn` 3.0.9 (`full`, `visit`, `parsing`), not a workspace member (its own `[workspace]` table and `Cargo.lock`); `score.py` builds it on first use with `cargo build --offline --release` into `$CARGO_TARGET_DIR/release/concord-probe-form-check` |
| `bank/` | main bank, seed 1 (`packed/` is regenerable and git-ignored) |
| `SEALED.sha256` | hash of the validation bank (seed 9001), which lives outside the repository in `work/sealed/bank/` |
| `FINAL.sha256` | hash of the final confirmation bank (seed 4242), which lives in `work/sealed/final/` and is used once at the end |
| `../decl.py` | the declaration parser (shared with I1) |

## Regenerating

```sh
export CARGO_TARGET_DIR=/home/canarit/projects/Concord_Desirability_Loop/work/targets/probe CARGO_BUILD_JOBS=6
python3 -I tools/desirability/probes/generate.py --seed 1 --out tools/desirability/probes/bank
python3 -I tools/desirability/probes/truth.py --bank tools/desirability/probes/bank
python3 -I tools/desirability/probes/pack.py --bank tools/desirability/probes/bank   # needs the nightly toolchain
python3 -I tools/desirability/probes/score.py --bank tools/desirability/probes/bank --selftest --out DIR
# score.py builds its R3 form checker itself; by hand (and its unit tests):
cargo build --offline --release --manifest-path tools/desirability/probes/form-check/Cargo.toml
cargo test --offline --release --manifest-path tools/desirability/probes/form-check/Cargo.toml
python3 -I tools/desirability/probes/run.py --bank tools/desirability/probes/bank --provider claude --model claude-opus-5-5 --sdk S.. --out RUNS
python3 -I tools/desirability/probes/run.py --bank tools/desirability/probes/bank --provider codex --sdk S.. --out RUNS
python3 -I tools/desirability/probes/score.py --bank tools/desirability/probes/bank --runs RUNS
# held-out banks (outside the repository; only their hashes are committed)
python3 -I tools/desirability/probes/generate.py --seed 9001 --role validation --holdout-of 1 \
    --out $WS/work/sealed/bank --quiet --hash-file tools/desirability/probes/SEALED.sha256
python3 -I tools/desirability/probes/generate.py --seed 4242 --role final --holdout-of 1 9001 \
    --out $WS/work/sealed/final --quiet --hash-file tools/desirability/probes/FINAL.sha256
```

Truth depends on the implementation, so regenerate it for every measured commit (`truth.py` writes
the `HEAD` it saw into each truth file, and `truth_format`; the scorer refuses R3 against truth
files older than format 3, fix round 3: their reference holds `let` statements, which the
single-expression contract refuses, and their `-> stream T` items are unchecked). Cargo runs only inside a throwaway copy,
`target/tmp/probe-truth-copy` (or `probe-score-copy`), which every run recreates. The committed tree
is never modified. `truth.py` compiles trusted generated code on the host; `score.py` compiles
reader answers inside bubblewrap (see "R3 execution" below). Reader runs made with an earlier
call question (an `async fn` body ending before `Ok(())`, or the fix-round-2 contract that allowed
`use` lines and `let` statements) are not comparable with the current answer contract; re-run
`pack.py` before new reader runs (the call questions' contract text changed in fix round 3). At a holdout checkpoint, run `truth.py` and `pack.py` on the held-out bank, then
`run.py` and `score.py`; the harness goes to `<bank>/harness/`. To check a held-out bank's
integrity, run `common.bank_hash` on it and compare the result with the committed hash.

## Generation

Per SDK (`examples/*-sdk`, parsed by `decl.py`), up to 6 endpoints. The selection is a greedy
stratified cover: each pick adds the most strata not yet covered, ties are broken in seeded order,
and once nothing new is covered the generator prefers a new (client, first scope). The strata
recorded per pick are:

`scope_depth2` (two or more scopes), `query`, `header` (a header value or block at scope/endpoint level, or a
client header set by the pick), `body`, `override_remove` (in a scope/endpoint block),
`optional_set`, `optional_unset`, `initialized` (a default in effect), `base_path` (base URL path
or client `path`), `scope_path`, `format_exception` (`request`/`response` on the endpoint).

The choices for each pick are seeded per (seed, SDK, endpoint). With two or more readable optional
values, at least one is set and one is left unset (at most 3 are set). With probability 0.4, one
sent string value needs percent-encoding (`a b/c`, `x&y=z`, `50% off`). Sensitive values get
`CANARY<SDK><n>Z` strings.

### Eligibility

- **Arguments**: a value is readable from a literal when its type (after SDK `type` aliases) is
  `String`/`&str`, an integer, `bool`, `f32`/`f64`, `NonZero*` (`std::num::NonZeroU32::new(3).unwrap()`)
  or `Vec<String>` with `each`. An endpoint is eligible only when all its required values, and those
  of its scopes, are readable. Unreadable optional values are left unset.
- **Bodies**: `String`, `serde_json::Value` (if the SDK depends on serde_json) or a struct/enum
  with `Default` (derived or `impl Default`). The snippet writes `T::default()`, not
  `Default::default()`. Generated arguments take `impl IntoValue<T>`, so `Default::default()`
  there is rustc E0283 (caller guide §1).
- **Clients**: built with `Name::new(reqwest::Client::new(), NameConfig { .. })`, or `Name::new(..)`
  alone when the client has no required values. A config field is filled as follows:
  - sensitive `String` or `HeaderValue`: a canary (`HeaderValue::from_static(..)`);
  - a readable type: a literal;
  - otherwise: `Default::default()` when the type has `Default`.
  A client is not eligible when a required value has none of these forms. As an extension, client
  values (config fields and client setters) may also be `HeaderValue::from_static("..")`.
  `generate.py --sdk-constructors` (opt-in, off for the committed banks) also accepts a public
  associated const of the value's SDK type, or a public no-argument associated fn returning it
  (`ApiVersion::PINNED`, `Policy::public()`). On seed 1 it makes Kraken (5 picks) and Salesforce
  (2 picks) eligible: 34 of 55 SDKs, 104 endpoints, 97 picks; the 32 other SDK files are unchanged.
  Kraken's 3 private endpoints then refuse locally (`Policy::public()` has no credentials), so they
  keep only their R2 items. Strava, Xero, PayPal, QuickBooks and S3 stay ineligible: their policy
  values need constructors with non-literal arguments.
- Main bank, seed 1: **32 of 55 SDKs are eligible, with 97 of 316 endpoints and 90 picks**. The
  main reasons are:
  - a required scope or endpoint value is an SDK newtype (GitHub `Owner`/`RepoName` ×22, Shopify
    `ShopDomain` ×11, Discord `ChannelId`, Ntfy `Topic`, Mailgun `Domain`, ...);
  - a required body type has no `Default` (Adyen, Anthropic, Linear, Elasticsearch, ...);
  - a client value with no literal form or `Default` (S3 `Signer`, Kraken/Xero `Policy`,
    PayPal/Strava `Session`, QuickBooks `Connection`, Salesforce `ApiVersion`).
  `index.json` lists every ineligible SDK and endpoint with its reason.

### Items and sessions

Each pick yields three items: `wire` (session `wire`), and `result` and `call` (session `rc`). The
`with_base_url` sub-question is asked once per client: the client's base must have a path or a
client `path` statement, with no template and no `subdomain` on the chain.

- The `wire` session gets the cheat sheet, the declaration and named items, and one snippet per
  item (`let api = ..; let call = ..;`). By default it gets no rustdoc, as `prompts/probe_reader.md`
  and `DESIRABILITY.md` R1 describe. `pack.py --wire-docs` adds the rustdoc.
- The `rc` session gets the same plus the generated rustdoc. Result questions name the endpoint by
  its place in the declaration and show no call snippet, so nothing from them can be copied into
  the call answer. The client constructor is shown once.
- Call questions state the R3 answer contract (`generate.CALL_CONTRACT`): `code` is one Rust
  expression evaluating to the `send().await` result, ending with `.send().await`, with no `?` on
  that outermost send and no trailing `;`; client setters are chained on `api` and values written
  inline (no `;`, no `let` or `use` statements, no other `.await` or `?`, no `return`, `break`,
  `continue`, loops, `async` blocks or `unsafe`); it is evaluated as
  `let result: <the send() result type> = { code };` inside an `async fn`. `pack.py` words every
  call question this way from the item's stored `intent` (`generate.call_question_text`), so
  generator-v1 banks (main, validation, final; their stored `question` text predates the contract)
  show the contract the scorer applies. The bank files and their hashes are unchanged. Generator
  v2 stores the same wording (fix round 3 changed the wording; no committed bank stores it). The
  scorer's parser (fix round 4) accepts a superset of the stated form: a `;` inside `vec![x; n]`,
  and statements, `return` and `?` inside a closure body; an answer that follows the wording is
  never refused by it.

### Materials (`pack.py`)

- **Declaration**: every `api!` block of the SDK, verbatim.
- **Named items** (from the SDK's `src/`, never its tests; `#[cfg(test)] mod` bodies are skipped):
  - the structs, enums, type aliases, traits, consts, statics and free functions the declaration
    names;
  - the methods it calls in initializers (`Type::method()`);
  - for bodies built with `T::default()`, the transitive field types and `impl Default` blocks.
  A free function over 60 lines is truncated, and the named items are capped at 60,000 characters
  per SDK.
- **Rustdoc**: the source is rustdoc JSON from `cargo +nightly doc --no-deps --lib` with
  `--document-private-items`, because generated clients are often `pub(crate)`. It is rendered to
  text: the items whose span lies inside an `api!` block, with their fields, methods, docs and trait
  impls, then the trait impls of the SDK types the declaration names (for example
  `impl SendWith<Paged<R>> for SendPolicy { type Output = .. }`). The nightly version and the JSON
  hash are recorded in `<sdk>.meta.json`.
- **No worked examples**: fenced code blocks inside SDK doc comments and in the rendered docs are
  replaced by "(code example omitted from the probe materials)". Then every call chain on the SDK's
  generated client, scopes and endpoints is removed from comment lines (declaration, named items)
  and from the rendered docs: a code span holding one becomes "(call example omitted from the probe
  materials)", and a line holding one outside a code span is replaced whole. A call chain is a run
  of `.name(args)` segments (arguments balanced; `.await`, `?` and line breaks allowed between) that
  spells an endpoint's full navigation (two or more segments), or names the endpoint and then
  `.send()`, `.prepare()` or one of its setters (`common.chain_hits`). Only the cheat sheet's
  generic example remains.
- **Leak scan** (`<sdk>.meta.json` → `leak_scan`; the same scan of the materials built without the
  call-chain filter is `leak_scan_unfiltered`):
  - a wire item's full truth URL appearing in the materials;
  - a call item's navigation spelled by a call chain (the rule above, so root endpoints and chains
    with arguments count), with the chain text (`call_chain_text`), and every endpoint call chain
    found at all (`endpoint_chains`);
  - the cheat sheet's generic example scanned the same way (`cheatsheet_chain`, informational): its
    `api.list().set_page(2).prepare()?` coincides with the 16 call items whose endpoint is `list`;
  - the exact R2 truth type. For docs-assisted `rc` sessions this one is expected, because rustdoc
    states each generated `send()` signature. In the main bank it hits 82 of the 86 scorable R2
    items, so R2 under rustdoc is largely a lookup in the docs.

  Main bank: unfiltered, the scan finds PokeAPI's `.pokemon("clefairy").send()` and
  `.list().set_limit(20).set_offset(0).send()` (the `KeepHead` docs; items `pokeapi/c1`, `c2`);
  filtered, no endpoint call chain is left in any SDK's materials.
- Concord's source, tests and records are never included.

## Truth (`truth.py`)

For each SDK file holding an `api!` block, a generated unit-test module (`use super::*`) is
injected into a throwaway copy of the repository. The line
`#[cfg(test)] #[path = "<abs>"] mod __concord_probe_truth;` goes at the end of the innermost inline
`mod` around the block, or at the end of the file. Building runs
`cargo test -p .. --lib --no-run --no-fail-fast --message-format=json`. Errors are mapped to
generated functions by line, failing functions are dropped and the rest is rebuilt until clean.
The tests then run with `cargo test -p .. --lib __concord_probe_truth -- --nocapture` and print
`@@PROBE {json}` lines, written by hand with no extra dependency. Proxy variables point at a closed
local port, so nothing reaches a real provider.

- **wire**: the method, the URL, the headers in order, the body (JSON when the Content-Type is
  JSON or `+json`, else text, or null), the Content-Type and the Accept. A header value that
  contains a canary becomes `<secret>`; canaries in the URL and body become `<secret>`. For
  `operation` items the truth is the recorded request instead, with the method, the declared
  origin plus the recorded target, the headers and the body. Headers the HTTP client adds are left
  out of it: host, content-length, transfer-encoding, connection, accept-encoding, and
  `accept: */*` when the prepared request had no Accept.
- **result**: the type comes from the declaration.
  - `send P -> Template` gives the type, with `_` replaced by the output.
  - Without a policy, the type is `Result<T, concord::Error>` (no `->` → `()`; `-> stream` →
    `concord::Incoming`).
  - Plain `send P;` is resolved through the SDK's `impl<..> SendWith<A> for P { type Output = .. }`
    by unifying `A` with the output. Exactly one impl must match; impls generated by a macro are
    "not resolvable".
  - `-> stream T` is not resolvable, because the reader type is the format's.
  The result is checked by compiling `__assert_output::<_, TYPE>(&call.send())`. The error type is
  the second argument of the resulting `Result`, after alias expansion. For the `with_base_url`
  question, `keeps` lists the candidates (`host`, `host/`, `host/<prefix>`, `host/<prefix>/`) whose
  prepared URL is the original path under the staging origin.
- **call**: the reference answer (one expression, `api.<client setters>.<call>.send().await`: the
  R3 contract form) is compiled in the same wrapper the scorer uses, bound to the declared send()
  result type (`binding`: the R2 truth's `send_returns`; success checked with `is_ok` when it is a
  `Result`). For `-> stream T`, whose reader type is the format's, the binding is the declared
  outer type with the reader left to inference (`Result<_, concord::Error>` without a send policy,
  the template such as `Result<_, Error>` with one), tied to the reference call's own
  `send().await` type (`same_as: reference`: the reference is type-checked in a branch that never
  runs, on a clone of the client), and checked with `is_ok`: the stream opened with the reply.
  Without a `Result` outer type the value stays unbound and unchecked. The reference then runs
  against local receivers answering 200 with candidate replies, in order, until the call
  succeeds: the files of the SDK's own `fixtures/` (names sharing a word with the endpoint, its
  scopes or its output first, error documents last), then a JSON document constructed from the
  output type (serde attributes followed, booleans false then true); for a `-> stream` pick then
  the same files again under each stream media type (`text/event-stream`,
  `application/x-ndjson`, `application/json`) and minimal bodies of each format; then `{}`, `[]`,
  an empty body and a text body. The first reply that decodes (or, for a stream, opens) is the
  item's `fixture`; the reference then runs twice more with it.
  The R3 truth is the first of those runs: method, target, headers other than host, content-length,
  transfer-encoding, connection and accept-encoding, and the body (canaries as `<secret>`). Header
  values and bodies that differ between the two runs are `volatile` (a nonce, a signature).
  `result_checked` is true when the binding checks a `Result` and both runs returned `Ok`; otherwise
  the item says why (`result_unchecked_reason`). When nothing is recorded, or the two runs send
  different request lines, `check_available` is false and the item is left out of R3 (the
  compile-only diagnostic remains).

Main bank, seed 1:
- wire: 88/90 items have truth (51 at the `operation` boundary). The two dropped are the Zendesk
  subdomain client: with a subdomain the base must stay a domain name, so the local receiver
  cannot be reached.
- result: 86/90 items are scorable. Four `-> stream T` endpoints are dropped (Kubernetes ×3,
  Mastodon ×1).
- call: 88/90 items have a request check; Zendesk ×2 have none. The result is checked on all 88:
  74 replies come from SDK fixture files, 10 are constructed from the type, 4 are stream replies.
  The 86 non-stream items bind the declared type; the 4 `-> stream T` items (Kubernetes ×3,
  `Result<_, concord::Error>`; Mastodon, `Result<_, Error>`) bind the declared outer type tied to
  the reference's own type. Kubernetes' `JsonStream` refuses `text/event-stream` and
  `application/x-ndjson` and opens with `application/json` (`stream:generic:json`); Mastodon's SSE
  format refuses its documented SSE body served as `text/plain` (the `.txt` extension's type,
  the fixture fix round 2 chose because its result was unchecked) and every JSON document, and
  opens with the same file as `text/event-stream`. Every item binds a type and checks `is_ok`.
  No header or body is volatile in the main bank.

None of the 51 `operation` wire items' policies changed the request (`policy_changes_request`:
false). `truth/drops.json` lists every drop.

## Scoring (`score.py`)

**R1**, per field:

| Field | Rule |
|---|---|
| `method` | upper-case match |
| `destination` | scheme and host in lower case, path with percent-encoding hex in upper case; an empty path is `/` |
| `query` | the `&`-separated pairs in order, hex upper case. Not decoded: `a+b` ≠ `a%20b`. `diag_query_decoded` compares the form-decoded pairs as a separate diagnostic |
| `header_names` | the set of lower-cased names must be equal |
| `header_values` | every non-secret truth header must have exactly the same value. Not applicable when the truth has no non-secret header. Secret headers are compared by name only; canary literals and `%3Csecret%3E` in answers are read as `<secret>` |
| `body` | JSON is compared structurally (a JSON string answer is parsed); text must match exactly; no body requires `null` |
| `content_type`, `accept` | exact match after trimming; a missing key counts as `null` |

Per item, `critical` means method, destination and query are all right, and `exact` means every
field is. A right method never makes up for a wrong destination.

**R2**: both types are resolved in the scope of the declaration's module (the module of the
item's `api!` block, `Sdk.decl_module`) to trees of **tagged definition identities**
(`common.canon_ident`), and the trees must be equal. Every path becomes `("id", identity,
generic arguments)` with one of these identities:
`("sdk-def", <crate>, *module path, Name)` (a definition in the module index),
`("sdk-item", <crate>, Name)` (an SDK type the index does not place, reached by a bare name),
`("ext-crate", <crate>, *path)` (an item of a crate the SDK uses: `concord::Error`,
`reqwest::header::HeaderMap`), `("std-lib", *path)`, `("std-prelude", Name)` (`Result`, `Option`,
`Vec`, `String`, `Box`), `("primitive", name)`, and `("unresolved", "<path as written>")`. An
unresolved path never equals a resolved identity, and no answer text can produce a resolved
identity except by resolving: through fix round 3 the comparison was on printable canonical text,
in which an SDK definition was written `sdk::<module path>::Name`, so an answer writing that text
directly (`Result<HolidayResponse, sdk::error::Error>`) stayed unresolved, kept as written, and
matched (review 5: 52 of 86 items). Tuples, references, slices and arrays compare structurally;
unparsed types (`dyn ..`, `impl ..`, `<A as B>::C`) compare as text. `canon_type` prints the tree
for the record (truth files, aliases) as `{tag path}<..>`, which is not a Rust type. An SDK owner
is resolved only through a path that really exists, so a wrong owner fails (`made_up::Error`,
`reqwest::Error`, `concord::Bytes`, `pokeapi::Error` for crate `concord_sdk_pokeapi`,
`crate::models::Error` where `models` has no `Error`):
- whitespace is normalized and backticks and a trailing `;` are dropped;
- a module index is built per SDK from its source (`common.ModuleIndex`): `src/lib.rs` is `crate`,
  `mod x;` there is `src/x.rs` or `src/x/mod.rs` (`crate::x`), nested likewise (`#[path]`
  followed), an inline `mod x { .. }` is a module, `#[cfg(test)]` modules are left out; per module
  it records the types defined at module level (struct, enum, union, trait, `type` alias, with
  their visibility), the child modules, and the `use` declarations (explicit names, `as` renames,
  globs, `pub` or not);
- a qualified SDK path (`crate::..`, `<crate name>::..`, `self::..`/`super::..` from the
  declaration's module, or a leading module name: a child of the declaration's module, else a
  crate-level module as rustdoc spells paths) resolves only when every segment but the last is a
  real module (or a module or crate imported or re-exported there: `<crate>::concord::X` through
  `pub use concord;`, `pagination::X` through `pub use concord_pagination as pagination;`) and the
  last name is defined there or imported/re-exported there (explicitly, renamed with `as`, or
  through a glob such as `pub use models::*;`); private items and imports count only where the
  declaration's module can see them (in that module or an ancestor). `crate::models::Error`
  (nager-holidays: `models` defines no `Error`) and `crate::models::models::Holiday` stay as
  written and match nothing; until fix round 3 every recognised module name was skipped, which
  credited the wrong-module and repeated-module controls on 40 of 72 items each;
- the definition reached is its `sdk-def` identity (`("sdk-def", "concord_sdk_nager_holidays",
  "error", "Error")`), so two spellings match only when they reach the same definition; a `type`
  alias is expanded in its own module's scope, including generic parameters with defaults; an
  item of another crate becomes its `ext-crate` path (`pub use concord::ResponseMeta` →
  `("ext-crate", "concord", "ResponseMeta")`); a path that does not resolve is `unresolved`;
- a bare name is looked up in the declaration module's scope (its definitions, `use` imports and
  globs), then the primitives, then the std prelude (`Result`, `Option`, `Vec`, `String`, `Box`),
  then SDK-wide (a type the SDK defines, the module closest to the root first, or a name it
  imports anywhere): `use concord::Error` → `concord::Error`, `use
  concord::reqwest::header::HeaderMap` → `reqwest::header::HeaderMap`, `use concord::bytes::Bytes`
  → `bytes::Bytes` (all `ext-crate`); a bare name found nowhere is `unresolved`;
- `concord::reqwest::..` and `concord::bytes::..` become `reqwest::..` and `bytes::..`; other
  `concord::..` paths are `ext-crate` paths as written; a leading `::` is dropped
  (`::concord::Error`); any other qualified path is an `ext-crate` path when its first segment is
  a crate the SDK uses (`Sdk.crates`: Concord, `reqwest`, `bytes`, the `[dependencies]` of its
  `Cargo.toml`, the first segment of its `use` paths), else `unresolved` (`made_up::Error`,
  `sdk::error::Error`, `sdk_def::..`);
- prefixes match only complete, from the start of the path. No macro-internal spelling
  (`concord_core::X`, `$crate::__macro::concord_core::X`, `concord::__macro::concord_core::X`) is
  resolved: the packed materials never print one (0 occurrences of `concord_core` or `__macro` in
  the 97 packed files of the main bank; rustdoc renders `concord::X`). A `concord` or
  `concord_core` segment inside a path is an invented owner (`made_up::concord_core::Error`,
  `x::concord::Error`). Until fix round 2 any path with an inner `concord_core` segment collapsed
  to Concord's export, which credited `made_up::concord_core::Error` on 34 of 86 items;
- `std`/`core`/`alloc` prelude paths (`std::result::Result`, `core::option::Option`, ..) are the
  prelude types; other std paths are `std-lib` paths (`core::fmt::Error` = `std::fmt::Error`).

Each truth file lists the aliases it accepts (`aliases`). `base_url` must be one of `keeps`, with
scheme and host in lower case.

**R3 answer contract** (`score.call_form`, decided by the `form-check/` helper's Rust parser,
fix round 4): `code` is one expression whose value is the public `send().await` result. The
helper (`syn` 3.0.9) parses the answer as rustc reads it inside the wrapper's `{ <answer> }`
(`Block::parse_within`; `syn` tokenises strings, raw and byte strings, chars, lifetimes and nested
comments as rustc does) and requires exactly one statement, an expression, with at most one
trailing `;` (dropped from the code to compile). After one optional outer `( .. )` or `{ .. }`
holding exactly one expression and one optional `?`, the expression must be
`<receiver>.send().await` (no arguments, no turbofish). A visitor over the whole receiver refuses:
- outside closure bodies: `return`, `break`, `continue`, `?`, `loop`/`while`/`for`, and a block
  with statements (also an `if`/`match`/`const` block: a block may hold one expression);
- anywhere, closure bodies included: `.await`, `async` blocks and closures, `unsafe`, `yield`,
  `try` blocks, items (`fn`, `impl`, `struct`, `use`, ..), attributes, syntax `syn` does not model,
  and macros other than bare `vec!` and `format!`, whose arguments must parse as expressions
  (`vec![a, b]`, `vec![x; n]`, `format!(..)`) and are checked by the same rules.

Inside a closure body, statements, `return`, `?` and loops only affect the closure, so they are
allowed (`(move || -> Option<_> { let n: usize = "2".parse().ok()?; .. Some(api) })()`). The
refusal reason lists every violation found. `if`, `match`, closures, `if let`, struct literals,
tuples and `vec![x; n]` stay allowed. Refused before compiling: `let r = ..; r`, `.ok()`,
`.unwrap_or_default()`, `send_raw`, a `match` around the call, `<call>.prepare()?.send().await`,
the review-3 early exit `<call>.prepare()?.send_raw().await?; return Ok(()); <call>.send().await`
("more than one statement"), and the review-5 answer that hides the same exit in a tuple between
raw byte strings, `(br#"""#, <call>.prepare()?.send_raw().await?, return Ok(()), br#"""#,
api).4.<call>.send().await` (its `return`, inner `?` and inner `.await`). The fix-round-3 rule was
lexical: its hand-written lexer misread `br#"""#`, so the review-5 answer passed the guard, the
balance check and the call-form rule, and compiled and passed. The same lexer is no longer used
for the balance check either: an answer must lex as balanced Rust token trees (the helper's
`proc_macro2` lexer), so it cannot close the wrapper's braces.

As a compatibility path, an answer ending in `.send().await?` is accepted with its
value bound to the declared result's `Ok` type (its error then leaves through the `?`). The
answer is compiled in this wrapper, with `TYPE` the call item's `binding` (the R2 truth's declared
send() result type; for `-> stream T` the declared `Result<_, E>` tied to the reference's own
type):
```rust
async fn __answer_..(api: Client) -> Result<(), Box<dyn std::error::Error>> {
    // `-> stream T` only: let mut __w = None; if false { let api = api.clone(); __w = Some({ <reference> }); }
    let __r: TYPE = { <answer> };            // `?` form: let __r: TYPE = Ok({ <answer> });
    // `-> stream T` only: __probe_same_type(&__w, &__r);
    __probe_outcome(&__r)                    // Ok(()) for Ok(_), the error's text for Err(e)
}
```
A value of any other type does not compile: a named discard (`let ignored = ..; ()`), a
swallowed error (`.ok()`), a raw response (`.prepare()?.send_raw().await?`). Where no `Result`
type is known (macro-generated `SendWith` impls, a stream behind a plain `send P;`; none in the
main bank) the value is bound without a type and its success is not checked: such items are left
out of R3.pass and their request check is reported as `R3.diag_pass_result_unchecked`.

**R3**: an answer passes when it compiles in that wrapper and, run with `api` pointed at a local
receiver that answers the item's `fixture`:
- it sends the reference's requests: the same sequence of method, path and query pairs; for each
  request the same header names, the same values for every header that is not volatile (secrets
  compared as `<secret>`), and the same body (JSON structurally when the Content-Type is JSON or
  `+json`, form bodies as pairs, anything else exactly; a volatile body only where the two
  reference runs agree);
- its value is `Ok`: the call succeeded with a reply the reference decodes into the declared
  output (for a stream: the stream opened). R3.pass counts only items whose result is checked
  (`result_checked`; all 88 in the main bank); on any other item the request check alone is
  `R3.diag_pass_result_unchecked`.

Compile-only success is reported as `R3.diag_compiles`, the old method/path/query-only rule as
`R3.diag_legacy_pass`, and the share of items whose result is checked as `R3.diag_result_checked`.

**R3 execution** (two bubblewrap sandboxes, `/usr/bin/bwrap`; the scorer refuses to compile or run
answers without it):
- *Compile* (`score.py` `CompileSandbox`): `cargo test --offline --frozen -j N --lib --no-run`
  in the throwaway copy `target/tmp/probe-score-copy`, which also holds the answer modules
  (`.probe-score-harness/`) and the fixture files they embed. Mounts: `--ro-bind / /`; `--tmpfs
  /run` (and `/var/run` when it is not a symlink to `/run`); `--tmpfs $HOME`; inside it a read-only
  bind of the active rustup toolchain (`RUSTUP_TOOLCHAIN` names it), and a writable tmpfs
  `$CARGO_HOME` holding read-only binds of `$CARGO_HOME/bin` and `$CARGO_HOME/registry` (cargo's
  lock files live at the `$CARGO_HOME` root); the copy read-only; the target directory
  writable; `--tmpfs /tmp`; an empty scratch `$HOME`. `--unshare-net --unshare-pid --unshare-ipc
  --die-with-parent --clearenv` with only `PATH`, `CARGO_HOME`, `RUSTUP_HOME`, `RUSTUP_TOOLCHAIN`,
  `CARGO_TARGET_DIR` and `HOME` set; `timeout 3600`. Paths are the host's, so cargo's fingerprints
  match the builds made outside and only the answer packages recompile.
- *Run* (`sandbox_cmd`): each answer's test alone, as its own process: `--ro-bind / /`, `--tmpfs
  /run`, `--tmpfs $HOME` with only the test executable (read-only) and a fresh scratch directory
  (the only writable path) mounted back, `--tmpfs /tmp`, the same namespace flags, `--clearenv`
  plus three variables, `timeout 60`, RLIMIT_FSIZE 64 MiB and no core dumps; its output goes to
  files outside the scratch directory. The run's records carry a nonce the scorer leaves in a
  scratch file; the harness reads and deletes it before the answer runs, and the scorer keeps only
  records with that nonce, so text the answer prints cannot pose as a recording.
- *Host IPC*: `/run` (D-Bus, systemd, the user's runtime directory) is hidden by the tmpfs;
  abstract sockets are isolated by the network namespace, SysV IPC by the IPC namespace. Any
  other pathname socket still visible in a layout is covered with `/dev/null` (`socket_masks`): the
  scorer scans each layout once (`find .. -type s` confirmed with `test -S`) and masks what it
  finds. On the development host that is one connectable socket outside `/run`,
  `/var/snap/hwctl/common/hwctl.varlink`.
- A regex filter stays as an extra layer, on the raw text (not lexed); answers it matches, or
  that do not lex as balanced Rust tokens, fail before compiling: process, file system, network,
  Unix-socket or environment access, compile-time file and environment macros (`include!`,
  `include_str!`, `include_bytes!`, `env!`, `option_env!`), `unsafe`, attributes, or their own
  `reqwest::Client` or `with_base_url`. It is not the only line: the parser-based
  single-expression rule is a further layer (it alone refuses the review-5 answer, which the
  regex filter lets through). The sandbox controls below need statements before the call, so
  they run with that rule off.

**Reporting**: results are reported per item (`readers.<r>.items`), per SDK and per boundary, per
reader (provider plus the observed model in `metas`), and pooled. Several readers answering one
item are not independent participants. The pooled rate therefore averages each item over readers
first, then averages over items, and `n_items` is the number of items.

**Self-test** (`--selftest`): seeded readers with expected rates (`score.py` `SELFTEST_EXPECT`),
discrimination references (`REFERENCE_EXPECT`: `__unbound` and `__form_off` harness variants
scoring never uses) and sandbox and rule effect checks; `score.md` prints each expectation next to
the observed rate and a verdict. `oracle`, `wrong` and `broken` answer every item; each control
answers only the items its perturbation applies to.
- `oracle` gives the truth's answers and the reference code; expected 1.00.
- `wrong` gives perturbed answers (wrong method, path, query, headers, body and media types; wrong
  types and base; R3 code that is not a call, refused by the answer contract); expected 0.
- `broken` gives R3 code in the contract form that does not compile; expected 0.
- `r2_owner_made_up`, `r2_owner_reqwest`, `r2_owner_nested_core`, `r2_owner_nested_concord`: the
  right type with the error's owner replaced (`made_up::Error`, `reqwest::Error`,
  `made_up::concord_core::Error`, `x::concord::Error`); expected 0.
- `r2_owner_wrong_module` and `r2_owner_repeated_module`: the error's owner replaced by a path
  built from the SDK's existing modules (`score.wrong_owners`): `crate::<m>::<Name>` for the first
  module `m` (closest to the root) that neither defines nor imports the name
  (`nager-holidays/r1`: `crate::models::Error`), and `crate::<m>::<m>::<Name>` for the first
  crate-level module (`crate::error::error::Error`); on every item whose SDK has a module;
  expected 0.
- `r2_alias_documented`: the same types in documented qualified spellings: each SDK type as the
  shortest real path from the crate root (`ModuleIndex.spelling`, a public one first:
  `crate::Error`, `crate::model::DeploymentList`), the error with the crate name
  (`<crate>::Error`), Concord's imports as `concord::Error`; expected 1.00.
- Internal canonical spellings written directly (review 5, finding 2), expected 0:
  `r2_spoof_internal_error`, the reviewer's form: the right type with the error written in the
  fix-round-3 canonical text, `sdk::<module path>::Name` (`Result<HolidayResponse,
  sdk::error::Error>` and `sdk::error::Error`), and `r2_spoof_internal_all`, every type in that
  text (`Result<sdk::HolidayResponse, sdk::error::Error>`), on every item whose error is an SDK
  type; `r2_spoof_canonical`, the current record text (`{std-prelude Result}<..>`), and
  `r2_spoof_tag_path`, each identity written as a path headed by its tag
  (`std_prelude::Result<sdk_def::concord_sdk_nager_holidays::HolidayResponse, ..>`), on every
  item.
- `r3_ok_form`: the reference ending in `.send().await?` (the compatibility path); expected 1.00.
- `r3_paren_wrapped` (`(<reference>)`) and `r3_block_wrapped` (`{ <reference> }`): equivalent
  spellings of the contract form, on every item whose result is checked; expected 1.00.
- `r3_wrong_body`: the reference call with a body that serializes differently (a public field set,
  or a builder setter such as `SearchRequest::default().query("wrongbody")`); `r3_wrong_header`:
  a header value changed, or an unset optional header set. Expected R3 0, while the old rule
  (`diag_legacy_pass`) passes them.
- `r3_named_discard` (`let ignored = <call>.send().await; ()`), `r3_swallowed_error`
  (`<call>.send().await.ok()`), `r3_raw_response` (`<call>.prepare()?.send_raw().await?`), on every
  item whose result is checked. They run with the call-form rule off, so only the type binding
  stands in the way: expected R3 0 and `diag_compiles` 0. Discrimination: the same answers bound
  without the declared type (`<control>__unbound`) compile, send the reference's requests and
  pass, expected 1.00; and the call-form rule alone also refuses every one of them (effect check).
- `r3_early_return`: the review's answer `<call>.prepare()?.send_raw().await?; return Ok(());
  <call>.send().await`, on every item whose result is checked; expected R3 0 and `diag_compiles`
  0 (refused by the single-expression rule). Discrimination: `r3_early_return__form_off`, the
  same answers compiled with the call-form rule off on the items without client setters (setters
  take the client by value, so the call cannot be written twice), compile and pass, expected 1.00.
- `r3_review5_reproducer`: review 5's exact answer on `nager-holidays/c1`, `(br#"""#,
  api.holidays("x&y=z",24).prepare()?.send_raw().await?, return Ok(()), br#"""#,
  api).4.holidays("x&y=z",24).send().await`, and `r3_byte_string_early_return`, the same shape
  built from every checked item's call; expected R3 0 and `diag_compiles` 0. Discrimination: with
  the call-form rule off (`__form_off`: the reproducer, and the built answers on the items without
  client setters) they compile and pass, expected 1.00. Effect checks: the regex guard refuses
  none of them and all lex as balanced tokens, so the parser alone refuses them; the reproducer's
  refusal names its `return`, its inner `?` and its inner `.await`.
- `r3_vec_repeat_in_struct` (`(std::ops::Range { start: 0usize, end: vec![0u8; 2].len() },
  api).1.<call>.send().await`) and `r3_closure_statements` (`(move || -> Option<_> { let n: usize
  = "2".parse().ok()?; if n == 0 { return None; } Some(api) })().unwrap().<call>.send().await`):
  valid single call expressions the fix-round-3 rule refused (review 5, finding 3), on every
  checked item, with the call-form rule on; expected R3 1.00 and `diag_compiles` 1.00.
- `r3_stream_wrong_media`: on each `-> stream T` item, the reference call answered with the
  item's reply under `text/plain; charset=utf-8`, which the stream format refuses; expected R3 0
  with `diag_compiles` and `diag_legacy_pass` 1.00 (the same requests: only the result check
  fails it). Discrimination: `r3_stream_wrong_media__unbound`, bound without a result check (the
  fix-round-2 rule for stream items), passes, expected 1.00. Effect check: every `-> stream` call
  item has its result checked, with a reply that is not `text/plain`.
- `r3_sandbox_fs` (2 items): `use std::{fs}; fs::write(<a host-writable directory outside the
  scratch dir>, ..)?;` then the reference call; `r3_sandbox_net` (2 items): a TCP connection
  (`tokio::net::TcpSocket`) to a listener the scorer opens on a non-loopback host address, then the
  reference. Both pass the regex filter, so only the run sandbox stands in the way. Expected R3 0,
  the file absent afterwards, and no connection at the listener (which the host itself is shown to
  reach). All sandbox controls hold statements, so they run with the call-form rule off (it
  refuses each of them too, an effect check).
- Compile-time and IPC controls, run with the regex filter off (it refuses each of them too, an
  effect check), so only the sandboxes stand in the way:
  - `r3_sandbox_include` (2 items): `include_str!("<a real file in the home directory>")` (the
    first of `~/.bash_logout`, `~/.profile`, `~/.bashrc`) before the call. Expected
    `diag_compiles` 0 with "couldn't read"; the same answers, compiled on the host with cargo
    outside bubblewrap in the same copy, compile.
  - `r3_sandbox_env` (2 items): prints `option_env!("HOME")` and `option_env!("CODEX_HOME")`
    before the call (expected R3 1.00: a harmless answer runs normally). Compiled in the sandbox
    they must be the scratch `$HOME` and `None`; compiled outside they must be the host's `HOME`
    and `CODEX_HOME`. When the host has no `CODEX_HOME`, the scorer gives bwrap and the outside
    cargo a canary value, so the control discriminates.
  - `r3_sandbox_ipc` (one item per socket): `std::os::unix::net::UnixStream::connect(<path>)?`
    before the call, for a listener the scorer opens in the user's runtime directory under `/run`
    (it counts connections), the user's D-Bus socket `/run/user/<uid>/bus` when it exists, and the
    first socket outside `/run` the run layout showed before masking. Expected R3 0: inside, the
    connect fails and no request follows; the same compiled tests run directly on the host connect
    and complete the call.
  - after masking, a scan finds no pathname socket in the run or the compile layout.

Main bank result (fix round 4, truth files from `984bd2f6` (format 3; R2 recomputes both sides);
all 85 expectations, 14 discrimination references and 19 effect checks met; `score.py --selftest`,
23 s with a warm target directory and the form checker built; 39 s on the first run, which compiled the new answers):
- oracle 1.00 on every R1 field (88 items, `header_values` 82), on R2 (86, 86, `base_url` 19) and
  on R3 (88 pass, all result-checked, 90 compile; Zendesk ×2 have no request check); `wrong` and
  `broken` 0.00 (`wrong` R3 code is refused by the call-form rule, `broken` fails with E0599).
- R2 wrong-owner controls 0.00: `made_up::Error`, `reqwest::Error`, `made_up::concord_core::Error`,
  `x::concord::Error` on 86 items each; owners built from the SDK's real modules on 72 items each
  (the 14 items of single-file SDKs have no module): `crate::<m>::<Name>` where `m` has no such
  name (`nager-holidays/r1`: `crate::models::Error`) and `crate::<m>::<m>::<Name>`
  (`crate::error::error::Error`). Under the fix-round-2 rules these two were credited on 40 of 72
  items each (error type). Documented spellings 1.00 (86). Internal spellings 0.00: the
  fix-round-3 canonical text of the error (`Result<HolidayResponse, sdk::error::Error>`) and of
  every type, on the 52 items whose error is an SDK type (the fix-round-3 text comparison credited
  the error spelling on all 52, reproduced before the fix), the record text and tag-headed paths
  on 86 items each.
- R3: `r3_ok_form` 1.00 (88); `( .. )`- and `{ .. }`-wrapped references 1.00 (88 each). Wrong
  body (7, now struct-update literals such as `NewFile { name: "wrongbody".to_string(),
  ..NewFile::default() }`) and wrong header (26) 0.00 with `diag_legacy_pass` 1.00. Named discard,
  swallowed error and raw response 0.00 on 88 items each, all failing to compile against the
  declared type (E0308 "mismatched types"; for the raw response "`?` operator has incompatible
  types"); bound without the type, the same 3 × 88 answers compile and pass (1.00), and the
  call-form rule alone refuses all 3 × 88. The review-3 early-return answer is refused before
  compiling on all 88 items ("more than one statement"); with the call-form rule off, the same
  answer compiles and passes on all 74 items without client setters (1.00), and the rule alone
  refuses all 74. Review 5's answer between raw byte strings is refused on `nager-holidays/c1`
  ("a `?` other than an optional final one (outside a closure); an `.await` other than the final
  `.send().await`; `return` outside a closure"), and its shape on all 88 items; the regex guard
  refuses none of these 89 answers and all lex; with the rule off the reproducer compiles and
  passes (1.00, one request, `Ok`), and so do the built answers on the 74 items without setters.
  `vec![x; n]` inside a struct literal and a closure body with statements, `return` and `?`:
  1.00 on 88 items each (compiled with the rule on). Checked against the fix-round-3 `score.py`
  (`05be505a`), its guard, balance check and lexical call-form rule accept all 89 review-5
  answers and refuse all 176 positive-control answers.
- Stream result: the 4 `-> stream T` items pass with the oracle, result checked. Answered with
  their reply as `text/plain`, they compile and send the reference's requests (`diag_legacy_pass`
  1.00) but fail (0.00): Kubernetes "response metadata not accepted: the response Content-Type is
  not application/json or an application/*+json type", Mastodon ".. is not text/event-stream";
  bound without a result check (the fix-round-2 rule) the same answers pass (1.00).
- Run sandbox: file write 0.00 (2; "No such file or directory": the escape directory is under the
  hidden home), TCP connection 0.00 (2; "Network is unreachable"; the host reaches the listener,
  the sandbox made 0 connections).
- Compile sandbox: `include_str!("/home/canarit/.bash_logout")` does not compile in the sandbox
  (2/2, "couldn't read .. No such file or directory") and compiles outside it (2/2).
  `option_env!` compiled in the sandbox gives `HOME` = `/home/canarit/.concord-probe-sandbox-home`
  and `CODEX_HOME` = None (2/2); compiled outside, `/home/canarit` and the host's (canary)
  `CODEX_HOME` (2/2); R3 1.00.
- Host IPC: `UnixStream::connect` fails in the run sandbox for the scorer's listener in
  `/run/user/1000` and for `/run/user/1000/bus` ("No such file or directory"), and for
  `/var/snap/hwctl/common/hwctl.varlink` ("Connection refused": masked); no request follows and the
  listener counted 0 connections. The same compiled tests run on the host connect to all three and
  complete the call (the listener counted 1). After masking, no pathname socket is visible in the
  run or the compile layout. The regex filter alone also refuses all 7 include/env/ipc answers,
  and the call-form rule alone refuses all 11 sandbox-control answers (they hold statements).

## Runners (`run.py`)

There is one fresh session per SDK session (`wire`, `rc`). At most 4 run in parallel, each in a
new empty directory under `target/tmp/probe-run/`.

- **Claude**: `claude -p --output-format stream-json --verbose --model M --tools ""
  --setting-sources "" --strict-mcp-config --safe-mode --no-session-persistence --system-prompt S`.
  `stream-json` is the `json` result plus the transcript, which the tool-call check needs. The
  observed model comes from the assistant events' `message.model` and the result's `modelUsage`.
  Effort is not visible.
- **Codex**: `codex exec -m gpt-6.1-sol -c model_reasoning_effort="xhigh" -c daybreak=false -s
  read-only -C <empty> --skip-git-repo-check --json -o last.txt`. The observed model, effort and
  sandbox come from the rollout's `turn_context`.

A tool call in the event stream (or in the Codex rollout) discards the session, which is kept as
`attempt1/`, and the session runs once more. A second tool call records it as failed. An answer
without an `answers` list is malformed and scores 0 for its items.

## Known limits

- The readers are models, not people; one reader's answers to several items are correlated.
- The truth is the implementation at the measured commit. For `operation` items it is one recorded
  exchange in which the receiver always answers `200 {}`. A policy that reacts to responses
  (retries, follow-up requests) is observed only on that path. R3 uses one reply per item (the
  item's `fixture`).
- R3's result check means "the value has the declared send() result type and is `Ok` with the
  item's reply"; a different endpoint that sends the identical request and has the same result
  type would pass. The call-form rule is syntactic (a parser, not a type checker): `let r = ..;
  r`, which is the same value, is refused, and so is `<call>.prepare()?.send().await` (the
  prepared request's own `send`, which skips a send policy: its inner `?` is not the final one);
  the questions and the reader prompt state the form. Inside the one expression an answer can
  still diverge (a panic through `.unwrap()`, a closure that loops); that run records no complete
  exchange and fails. A closure body can call synchronous code before the call (no `.await`
  there); the sandbox bounds it, and any extra request it made would fail the request check.
  `syn` and rustc can disagree on rare statement splits (`{ api }.list()` at the start); such an
  answer then fails to compile, it is not credited.
- For `-> stream T` items "`Ok`" means the stream opened (the format accepted the reply's media
  type); no event is read. Kubernetes' reply is a minimal JSON value (`{}` as `application/json`):
  the SDK has no watch fixture.
- Both R3 sandboxes hide the home directory and `/run`, but the rest of the file system is
  readable: an answer can read world-readable system files at compile time (`include_str!` of a
  file under `/etc`, also refused by the regex filter) or at run time. It cannot write outside its
  scratch directory, reach the network, other processes or host sockets; its output stays in the
  score files. The compile sandbox shares the probe target directory (writable) with the trusted
  truth builds; answer code cannot run at compile time (build scripts and proc macros come from
  the locked dependencies). A run has no memory or process-count limit beyond its 60 s; processes
  it starts die with the sandbox's PID namespace. An answer that scanned its own process memory
  (`/proc/self/mem`) could find the nonce.
- The eligibility rules leave out 23 SDKs and most endpoints that take SDK newtypes. As a result,
  the bank under-represents validated identifiers, signing policies and typed request bodies.
- Held-out banks: seed 1 already uses 90 of the 97 eligible endpoints. The validation bank (seed
  9001) has 7 fresh endpoints and 64 picks that reuse main-bank endpoints with a different
  instantiation (`reused_endpoint`). The final bank (seed 4242) is entirely reused. Both therefore
  measure generalization over values and optional-value patterns more than over endpoints.
- Subdomain clients (Zendesk) have no operation truth or R3 request check: `with_base_url` cannot
  point them at the local receiver. This could be fixed with a resolving `reqwest::Client`.
- The committed seed-1 question files predate the merged ClickHouse change that marks the
  header-auth username `#[sensitive]`: regenerating seed 1 now gives a different `clickhouse.json`
  (the username becomes a canary). The committed bank is kept; truth is regenerated against it.
- R3 compiles answers in the SDK module's scope (`use super::*`), so crate-private items and the
  module's imports are visible, which is more lenient than an external caller. "The result type,
  where stated" is not checked, because no intent states one.
- R2 with rustdoc is mostly a lookup in the docs (see the leak scan). `-> stream T` items and
  macro-generated `SendWith` impls are not scored.
- The R2 module index reads source text: items generated by macros (the `api!` client types,
  newtype macros) are not in it, so a qualified path to one stays unresolved (scored wrong), and a
  bare name not in the declaration module's scope falls back to an SDK-wide definition of the same
  name. Paths into other crates are not checked against those crates: `reqwest::Error` is an
  `ext-crate` identity of its own (it equals only the same path), not verified to exist. `pub(crate)` and `pub(super)` count as public, and `#[cfg(..)]` other than `test` is not
  evaluated. Module paths are not checked for visibility (a private module's items resolve).
- Parsing (`decl.py`, Rust items, types) is heuristic. Whatever it gets wrong is caught by the
  compile steps and recorded as a drop; it is never silently scored.
