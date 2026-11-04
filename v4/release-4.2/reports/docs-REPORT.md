# Lane `docs`: report

> Saved by the coordinator from the lane's returned text: the lane's own write of this file was
> refused by a harness hook for subagents, and the lane correctly did not route around it. Lane:
> Claude Opus subagent (Agent tool, requested `opus`, effort `xhigh`; effective effort not
> observable). Logs, probes and isolated Riot copies are in this directory.

- Worktree `/home/canarit/projects/concord-v42/wt-docs`, branch `v42/docs`, base `07dcdd03c61a1822341e92d3820e66117f1aef14`.
- Final commit: **`0aa5942b15220849e9c77dbf3aa2a3077d1d080c`** (merged into `concord-v4.2` as `d17a675`).
- Toolchain 1.99.1; every compiling command through `tools/heavy.sh`, `--locked`, worktree `target/`.
- `scripts/verify.sh` and replay consumers not run.

## Files changed (12)

`README.md`; `docs/{AUTHOR_GUIDE,CALLER_GUIDE,QUICKSTART,DIAGNOSTICS,MIGRATION,CUSTOM_FORMATS,MAINTAINERS}.md`;
`examples/riot/README.md`; `crates/concord-extras/README.md`; `crates/concord-extras/src/lib.rs`
(crate doc table only); `crates/concord-core/src/value.rs` (doc comment only: text plus a
compile_fail/passing doctest pair). No code outside doc comments; nothing in `concord-macros`,
`benches`, `scripts` or `v4`.

## Corrected statements (before = `07dcdd0`, after = `0aa5942`)

1. **HTAB** (source `http-1.5.0/src/header/value.rs:563`): AUTHOR_GUIDE 493 → 507-512 ("no control
   bytes" → http's rule: visible ASCII, space, HTAB, UTF-8 accepted; other controls rejected; §4.8
   doctest sends and asserts `a\tb`); DIAGNOSTICS 204-205 → 212-214; MIGRATION 398 → 400-402.
2. **`into_response()`** (source `error.rs:190-197`): CALLER_GUIDE 318-319 → 334-338 (Status and
   Metadata; Build/Transport/Decode unchanged as `Err`); QUICKSTART 205-206 → 211-213; MIGRATION
   785 → 788-789.
3. **Runtime base / whole segments** (source `Base::parse` `__private.rs:74-98`, `Parts::segment`
   `:325-330`): AUTHOR_GUIDE 196-198 → 196-199 (IDN hosts, percent escapes kept as written, port 0
   refused); 202-203 → 203-206 (a segment with values that comes out empty, `.` or `..` is a
   `prepare()` build error; `.x` ordinary) with doctest assertions 267-275; CALLER_GUIDE 92 → 97-99;
   DIAGNOSTICS 177 → 186-187. No native URL operation described as a Concord defect.
4. **Text's Content-Type policy** (source `text.rs:30-40`, `media.rs:23-32`): AUTHOR_GUIDE 696 →
   715; MIGRATION 83-84 → 83-85; CUSTOM_FORMATS 416; concord-extras `lib.rs:7`.
5. **Diagnostics**: inherited-format errors at the payload type (DIAGNOSTICS 534 → 543, matches
   `format_inherited_unsupported.stderr`); unresolved-name cascades compiler-dependent (21-22 →
   21-24, probes P2/P3); "reported twice" pinned to rustc 1.99.1 (227 → 236); MIGRATION 172 → 173-174
   (count removed, no snapshot backs it); AUTHOR_GUIDE Depth 1323-1326 → 1342-1349 (1.97.1 numbers
   re-checked on 1.99.1 per C08-B; per-crate `recursion_limit`; `cargo doc` can need it first,
   I24-B); DIAGNOSTICS 765 → 774-777 (E0320/E0275 rough edge).
6. **Compiler vintage**: AUTHOR_GUIDE 1329-1330 → 1352-1354; MAINTAINERS 170 → 170-173.
7. concord-extras README names `IgnoreContentType<F>`.

## Task-adjacent notes

Config only for required client values (CALLER_GUIDE 94-99, QUICKSTART 97-99); formats as type
imports (QUICKSTART 110-111); prepared Accept vs client defaults (CALLER_GUIDE 261-266, checked
against `reqwest-0.13.5/src/async_impl/client.rs:2631-2634`); `.into()`/`.parse()`/`Default` give
E0283 for `IntoValue` arguments (CALLER_GUIDE 85-89, DIAGNOSTICS 153-157, `value.rs` doctest pair);
no `->` vs `-> ()` (CALLER_GUIDE 256-259, QUICKSTART 115-116); scope reuse formats values at each
`prepare()` (CALLER_GUIDE 179-180); copying the Riot example into its own project
(`examples/riot/README.md`, pointer README 28). No synonyms, no new methods, no required value made
optional.

## Maintenance example (compiled)

AUTHOR_GUIDE §11.1 "Moving a default into a scope" (1383-1505), linked from §4.7 and the §11 table:
`per_page=30` and `x-cache: max-age=60` move into `scope repo`; `issues` inherits (pairs reorder),
`pulls` keeps per-call `override … else inherit`, `commits` overrides and removes, `readme` removes
both — all pinned by assertions in one doctest with `before`/`after` modules.

## Checks (logs in `logs/`)

- C1 `cargo test --locked -p concord-example-quickstart`: exit 0 (caller_guide 8, doc_excerpts 2,
  quickstart 3; doctests 33 passed, 10 ignored, incl. §11.1 and the changed §3/§4.8 blocks).
- C2 `cargo test --locked -p concord-core --doc`: exit 0 (IntoValue pair).
- C3–C5 strict `cargo doc` for concord, concord-core, concord-extras (all features): exit 0.
- C6 link/anchor check over changed Markdown: 0 bad.
- Rustdoc inspection: **text extraction only**, not visual.
- Controls: N1 removing readme's `remove "per_page"` → exit 101 at `AuthorGuide (line 1489)`
  (`v=2&per_page=30` vs `v=2`), restored; N2 plain set instead of `override` → exit 101 "already set
  by `Forge.repo`", restored; N3 compile_fail block made compilable → exit 101, restored (stable
  rustdoc does not check the E0283 code annotation).
- Probes (rustc 1.99.1, uncommitted, in `probes/`): E0283 for `.into()`, `.parse()`, `Default`;
  positive control prints `INFER_OK`; broken `api!` used from same/other module → one error only;
  naive default move → two "already set" errors.
- Riot isolated copy (`riot-isolated/`): offline locked test exit 0, 22 passed; `default-features =
  false` → E0432 (Json/Binary); with `json` → check exit 0.

## Not done

verify.sh / workspace tests; feature-lane doc builds (changes not feature-gated); visual rustdoc;
fresh documentation-first trials (coordinator's); query-key/template identifier boundaries; other
platforms.

## Changes requested from other owners (applied by the coordinator in `7a2819a`)

1. `v4/SEMANTICS.md:217` HTAB wording.
2. `examples/quickstart/tests/caller_guide.rs:225` and `docs/CALLER_GUIDE.md:365` excerpt comment.
3. `benches/scale/timing_plan.sh` default toolchain 1.97.1 → 1.99.1.
