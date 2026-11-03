# Final record — native header names, representation statements, Riot

Branch `concord-v4-final`, worktree `/home/canarit/projects/concord_v4_final`. Started from
`625ac49` (`concord-v4-native-repr`, code `1d9ffe7`). Not pushed, not merged, nothing published.
Preserved unchanged: V3 (`concord-v3`), `concord-v4` (`8f7adc2`), `concord-v4-refine`
(`a915c8d`), `concord-v4-native-repr` (`625ac49`; `6bc5032` on the remote), the native-repr side
branches, and the untracked `.claude/` and `concord_v4_handoff/` in the V3 worktree (the
finalization handoff was extracted into the work directory). The migration side branch `concord-v4-final-migrate` was merged here (`70f24a8`)
and left in place. The work directory with snapshots, proposals, trials, reviews and raw
measurements is `/home/canarit/projects/concord_v4_finish_work/`.

**Final candidate:** `81836ec` (code). Later commits change only `v4/` records.

| | State |
|---|---|
| Implemented | Native header names (F06–F09, with the amendments and the per-occurrence identity of X-R1). Representation statements (F01–F05). Sealed `ToHeaderValue` (F08). Riot example (F11). Regex migration route withdrawn (F10). Review-pack reconciliation (F13: error accessors, decode hints, `#[must_use]`, cheat sheet, compatibility example, narrow late-error expansion) |
| Tested | `scripts/verify.sh --msrv --nightly --package` on `81836ec`, clean tree, Linux x86_64, rustc 1.97.1 / 1.85.1 / nightly: 21 of 22 steps pass, including 601 workspace tests. The `records` step failed only because this file did not exist yet; it passes from the records commit on. Receipt: `/home/canarit/projects/concord_v4_finish_work/evidence/verify_81836ec.txt` |
| Independently reviewed | Executable: Sol 6.1 XHigh. Semantic: Claude Opus. Both reviewed `ab3cb85`, re-reviewed `0f559d8` and confirmed `5195cd1`. Fresh use: Terra 6.1 XHigh and Sonnet (ten tasks), Terra (three tasks on the changed journeys). See `MODEL_RUNS.md`, `FINDINGS.md` |
| Accepted | See "Acceptance" below |
| Windows / macOS | NOT_RUN for this candidate: CI needs a push, which is not authorized. The baseline passed Windows CI (run `41459582303`, `6bc5032`) |

## 1. The authoring surface

One mixed-format declaration: the doctested `docs/CHEATSHEET.md`. Its text:

```rust,ignore
pub const X_TENANT: HeaderName = HeaderName::from_static("x-tenant");

concord::api! {
    /// A documents API.
    pub client Docs(
        #[sensitive] header(AUTHORIZATION) token: String, // required: a field of `DocsConfig`
        header(X_TENANT) tenant?: String,                 // optional: absent until set
    ) {
        base "https://api.docs.example/v1";
        headers { ACCEPT = "application/json"; "x-client" = "concord"; }
        format json;

        scope folder(id: u32) {
            path "folders/{id}";
            GET list(query page?: u32, query("per-page") per_page: u32 = 50) -> Vec<Doc> { path "docs"; }
            POST create(body doc: NewDoc) -> Doc { path "docs"; }
            PATCH rename(doc: u64, body change: Rename) -> Doc {
                path "docs/{doc}";
                request json "application/merge-patch+json"; // format and media type together
            }
            PUT write(doc: u64, body markdown: String) { path "docs/{doc}/content"; request text "text/markdown"; }
            GET raw(doc: u64) -> Vec<u8> { path "docs/{doc}/raw"; response bytes; }
            DELETE remove(doc: u64) { path "docs/{doc}"; } // no `->`: any 2xx, body not decoded
            GET public(doc: u64) -> Doc {
                path "public/{doc}";
                headers { remove AUTHORIZATION; override "x-client" = "concord-public"; }
            }
        }
    }
}
```

Calling it:

```rust,ignore
let docs = Docs::new(reqwest::Client::new(), DocsConfig { token: "Bearer t".into() }).set_tenant("acme");
let folder = docs.folder(7);
let page: Vec<Doc> = folder.list().set_page(2).send().await?;        // GET …/folders/7/docs?page=2&per-page=50
let renamed: Doc = folder.rename(3, Rename { title: "Notes".into() }).send().await?;
let prepared = folder.public(3).prepare()?;                          // a native reqwest::Request, no Authorization
match folder.remove(9).send().await {
    Err(e) if e.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) => {
        let wait = e.response().and_then(|r| r.headers().get(reqwest::header::RETRY_AFTER)); // borrowed
        let body = e.into_response()?.bytes().await?;                                       // then taken
    }
    other => other?,
}
```

The real Riot declaration is `examples/riot/src/api.rs`. Its shape:
- a sensitive native key `#[sensitive] header(X_RIOT_TOKEN) key`, with `X_RIOT_TOKEN` a
  `HeaderName` constant;
- `ACCEPT = "application/json"` and `format json;` on the client;
- one scope per published route list, each with `subdomain "{route}"`:
  - `account` (regional): includes RSO `me`, which does `remove X_RIOT_TOKEN;` and sets
    `AUTHORIZATION = "Bearer {access_token}"`;
  - `platform`;
  - `matches` (regional): seven query parameters, six of them optional;
  - `tournament_stub`: three body operations;
- a separate credential-free `DataDragon` client (`-> Bytes` with `response bytes;`).

The calling code is `examples/riot/src/lib.rs`:
- `player_summary` makes 4 + n bounded reads and stops on a `429` with `Retry-After`, read through
  `Error::response()`;
- `tournament_codes` sends a body.

Generated rustdoc of a request type: `RiotTournamentStubCreateCodes`, rendered as text.

```text
Creates tournament codes (at most 1000 per call).

Request: POST https://{route}.api.riotgames.com/lol/tournament-stub/v5/codes. Body (parameters):
TournamentCodeParameters, encoded with the format json (the request default of the client Riot).
Content-Type: application/json is sent with the body. A header of this chain whose native name is
content-type replaces it (and is sent with or without the body). Output: Vec<TournamentCode>,
decoded from a 2xx response with the format json (the response default of the client Riot). Query
(when present): tournamentId, count. Headers, in chain order (sent when present; native names are
compared when the request is built): X_RIOT_TOKEN (sensitive), ACCEPT.

pub fn prepare(self) -> Result<Prepared<Vec<TournamentCode>>, Error>
    Builds the native request without sending it.
    # Errors
    ErrorKind::Build when a value cannot be written (a path segment, host label, header value or
    body) or two native header names collide; nothing is sent.
```

A representative diagnostic: `ui/header_constant_as_value`.

```text
error: `JSON` is not a value of `Api`; the right of `=` and `{..}` holes name values declared in a parameter list (a Rust constant is not one: declare a value initialized from it, `json: T = ..`)
  --> tests/ui/header_constant_as_value.rs:10:28
   |
10 |         headers { ACCEPT = JSON; }
   |                            ^^^^
```

## 2. Before and after

From `concord-v4-native-repr` (`625ac49`). This compiles there, verified in a scratch crate
against that tree:

```rust,ignore
concord::api! {
    pub client Store {
        base "https://store.example.com";
        request json;
        response json;
        PATCH patch(id: u64, json("application/merge-patch+json") patch: Patch) -> Item { path "items/{id}"; }
        GET readme() -> text String { path "readme"; }
        PUT upload(bytes data: Bytes) { path "upload"; }
    }
}
```

On this candidate it is rejected with the exact replacements:

```text
error: removed in-place body format: write `body patch: Patch` and `request json "application/merge-patch+json";` on this endpoint
error: removed in-place output format: write `-> String` and `response text;` on this endpoint
error: removed in-place body format: write `body data: Bytes` and `request bytes;` on this endpoint
```

After the migration (a doctest in `docs/MIGRATION.md`): `format json;`, then
`PATCH patch(id: u64, body patch: Patch) -> Item { path "items/{id}"; request json "application/merge-patch+json"; }`,
`GET readme() -> String { path "readme"; response text; }` and
`PUT upload(body data: Bytes) { path "upload"; request bytes; }`.

Header names: `header("authorization") token` stays valid. The new forms are
`header(AUTHORIZATION) token`, `X_RIOT_TOKEN = v` and `remove AUTH;`, where `AUTH` is any alias.
From V3, see `docs/MIGRATION.md` (table and a full example).

## 3. Consequential decisions

`DECISIONS.md` gives the reasons and the rejected alternatives.
- **Formats as statements (F01–F04).**
  - `format`, `request` and `response` can be written on the client, a scope or an endpoint; the
    nearest one wins, and a format and its media type change together.
  - Payload positions name Rust types only.
  - No clearing form.
  - Format words for the built-ins, paths for custom formats.
- **Media types (F05)** are parsed by `mime`.
- **Native header names (F06).**
  - A literal or a `HeaderName` path, wherever a header key is written.
  - Identity is the resolved name. Every path occurrence is its own key (X-R1): equal tokens are
    not proof of one header.
  - Literal-only chains are composed by the macro. Chains with a native key are composed when the
    request is built, by `HeaderPlan`, on names alone.
- **HTTP validity belongs to `http` (F07).**
- **Sealed `ToHeaderValue`, no `TextHeader` (F08).**
- **Sensitivity by resolved name (F09).** Run-time conditions, pruned, cached per value,
  `cfg`-gated and computed on every chain.
- **The regex migration script is withdrawn (F10).**
- **Riot (F11).**
- **Review-pack reconciliation (F13).**
  - Rejected: empty JSON as `null`, a media-type allowlist, and a literal `content-type` ban.
  - Added: bounded improvements (`RECONCILIATION.md`).

## 4. Model participation

Every requested model ran with the requested settings. The observed values come from the session
`turn_context` (Codex) or the transcript `message.model` (Claude), recorded in `MODEL_RUNS.md`.
- **Claude Opus** (`claude-opus-5-5`): blind designer, candidate implementation (nodes), final
  semantic review, re-review and confirmation.
- **Claude Sonnet** (`claude-sonnet-5-5`): blind designer, migration of tests and examples, fresh
  user (ten tasks).
- **Sol 6.1 XHigh** (`gpt-6.1-sol`/xhigh): blind designer, blind cross-review, final executable
  review, re-review and confirmation.
- **Sol 6.1 Medium** (`gpt-6.1-sol`/medium): pragmatic assessment, corpus and control, the
  statements candidate, the compatibility example.
- **Terra 6.1 XHigh** (`gpt-6.1-terra`/xhigh): blind designer, blind cross-review, fresh user (ten
  tasks, then three).

No reviewer certified its own implementation. The executable reviewer (Sol XHigh) did not write
the candidate. The compatibility example was built by Sol Medium, a different run and effort, and
the semantic reviewer examined it too.

## 5. Verification

Bound to `81836ec`, clean tree (receipt
`/home/canarit/projects/concord_v4_finish_work/evidence/verify_81836ec.txt`):
- fmt, clippy (`-D warnings` lints of the workspace);
- 601 workspace tests: 73 UI fixtures, oracles of 252 sensitivity cases and about 838,000
  header-plan chains, Riot 17, compat 10, `checks/hygiene`;
- the `concord` feature lanes: no default features 384, JSON only 436, reqwest defaults only 386,
  form only 387, form with JSON 5;
- feature isolation and multipart alone (11);
- five rustdoc builds with `-D warnings`;
- MSRV 1.85.1: 442, 130 and 11 tests;
- packaged `.crate` consumers;
- nightly `Clone` checks (16).

`records` passes once this file exists.

Independent execution evidence beyond the repository's own tests:
- Sol XHigh: about 106 tests of its own on 1.97.1 and 1.85.1, 16 negative compile probes,
  mutants (five in the review, then the two blocking fixes, all killed), packaged consumers, 32
  generated native/literal pairs plus 96 fixpoint cases with 4,928 matching observations.
- Opus: probes, a three-way differential fuzz of about 440 declarations against literal-spelling
  ground truth, and `-Ztime-passes` comparisons.

Not run: Windows and macOS (no push), live Riot, editor navigation.

## 6. Performance trade-offs

`PERF.md` has the full record.
- **Allocations.** Identical to the baseline on all 84 shared benchmark IDs.
- **Representation statements** cost nothing measurable at run time or compile time.
- **Literal header names** cost what they did before.
- **Native header names at run time** cost about 150–190 ns per `prepare` on six-header chains
  (a run-time header plan, no allocation).
- **Native header names at compile time.** on the final candidate (PERF §5) a deep chain of 32 native keys compiles at 1.47× the literal
time in release (11.07× before the review-driven fixes), a 200-endpoint API with 34 native
contributions per chain at 1.86× release / 2.29× check (3.41× / 4.03× before), and a flat
1,000-endpoint API with every header key native at about 1.2×. The premium is linear in the native
contributions on each endpoint's chain.

## 7. Review dispositions

`FINDINGS.md` lists every finding with its disposition:
- the fresh-use findings (U1–U17);
- measurement and reconciliation (P1–P4, R1–R7);
- the final reviews (X-R1..X-R4, O-R1, O-S1..O-S4, O-P1..O-P7, O-D1, O-D2);
- the re-reviews (X2-1..X2-3, N1..N4, P-a..P-e);
- the confirmations.

The three release-blocking defects all concerned native header names, and each is fixed with a
regression test that fails on the earlier code:
- the merge by tokens (X-R1);
- spelling-dependent classification across chains (O-R1);
- a compile failure under `cfg` (P3).

## 8. Migration

- **From `concord-v4-native-repr`:** rewrite in-place formats as statements, following the
  compiler's replacement messages and `docs/MIGRATION.md`. Generated signatures are unchanged.
- **Literal header names** stay valid. Native names are optional.
- **`TextHeader` is gone:** send a domain type's text through a template, or build a `HeaderValue`.
- **Behaviour changes:**
  - UTF-8 header text beyond ASCII is accepted where `http` accepts it.
  - The same native constant set twice is a `Build` error at `prepare()`; prepare every endpoint
    in a test.
  - The empty-body decode error now carries a hint.
- **From V3:** `docs/MIGRATION.md`, "From Concord V3 to V4".

## 9. Remaining limitations and backlog

- **Identity errors.** Mistakes involving native header names, such as a constant set twice or an
  override with no target, are found by `prepare()`, not the compiler. Backlog item 1: a
  generated, name-only check of every endpoint's header plan per client (O-D2).
- **Classification over-approximates in two recorded cases** (O-P1, O-P7): more redaction, never
  less. Backlog: one exact run-time fixpoint over real names (O-D1).
- **A native chain generates its own header plan per endpoint,** linear in the native
  contributions on the chain: about 1.2× compile time for an API whose every header key is native, up to 2.3× (check) on a
stress suite with 34 native contributions per endpoint.
- **Not fixed: one duplicate error per missing `Display` or `ToHeaderValue`** (D-DIAG-2).
- **Not done:** generated modules, a formatter, `#[deprecated]` on values, an `ErrorKind`
  `Display`.
- **Riot facts not verified** (`examples/riot/SOURCES.md`):
  - the stub's routing value;
  - the key-header spelling in the retrieved text;
  - stub eligibility.

  Fixtures prove only the request Concord sends.
- **NOT_RUN:**
  - Windows and macOS for this candidate;
  - HTTP/2 and TLS-specific behaviour;
  - editor navigation (rust-analyzer go-to-definition and rename);
  - live services.

## Acceptance

The coordinator accepts `81836ec` as the refined V4 against the owner's finalization
handoff and the review-pack addendum:
- The selected body/response design is implemented throughout: DSL, core, generated callers,
  diagnostics, docs, examples and tests. It held under the pack's counterexamples. The three
  rejected proposals have recorded counterexamples.
- Native header names follow resolved identity, `http` decides header validity, and Concord keeps
  no competing validator.
- The Riot example is sourced, typed and tested offline; no live write was made.
- The reqwest boundary is unchanged: no auth orchestration, retries, rate limiting or pagination.
- Criterion, wire tests, oracles, feature isolation, MSRV, packaging and compatibility checks are
  kept and pass. The regex migration route is withdrawn.
- Two independent families, each from a different provider, ran full executable and semantic
  reviews of the frozen candidate, then targeted re-reviews and confirmations. Every finding is
  fixed, documented or deferred with a trigger. No required review is pending.

This is the coordinator's acceptance for the owner's review. It authorizes no push, merge or
publication. Windows and macOS CI for this candidate remain NOT_RUN until the owner authorizes a
push.
