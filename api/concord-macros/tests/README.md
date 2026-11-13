# Parser evidence

`unit/mod.rs` is included inside the proc-macro crate. This keeps the production
parser private and tests it using `proc_macro2`, without invoking a compiler.
`ui.rs` runs nine compiler snapshots through the facade and two compile-fail
checks of the typed catalogue API. They use author lines that differ from the
page examples, and are recorded and verified using stable Rust, with Cargo offline.

- `corpus/`: all 41 complete roots and eight schematic candidates copied from
  `/home/canarit/projects/concord_v5_probes/challenge-parser/corpus/`.
- `attacks/`: all 180 files copied from that probe. Lexical errors, syntax errors
  and returned trees all count as completed cases; every case is caught for
  panics. No semantic outcome from the probe is assumed.
- `paper/`: 57 complete roots freshly extracted from today's tour, language,
  response, behaviour and all eleven API pages, using the probe's independent
  lexical extractor (none of its parser or checker code). Tests verify that every
  extracted source is still present in its page, as well as parsing it.
- Eleven current and `again/` API pages are additionally read at test time, their
  complete `concord::api!` blocks extracted as token groups, and parsed. All
  eleven parse. README's older API descriptions still use legal expression
  bodies; their historical design and type-checking issues are outside step 1.

## Fixture migrations

`fixture-migrations.json` lists every changed fixture and the number of input
annotations updated. In the old probe an `Option<T>` input implicitly meant
optional; these intended optional inputs now use `name?: T`. An explicit
`Option<T>` after an arrow and inside a Rust expression is unchanged. The
qualified `::std::option::Option<String>` input of `url_optional.rs` likewise
becomes `x?: String`. No body expression needs replacing: those remain legal.
New entry bodies, optional marks, `idle_timeout`, and part bases are covered by
fresh paper roots and tree tests. The original corpus page/line metadata and
source hashes remain historical provenance; sources in its two manifests have
been updated to match their migrated fixtures.

## Intentional non-descriptions

All complete descriptions parse. The eight copied `schematic_*.rs` files fail
as intended, without weakening the grammar:

| File | Why it is not a description |
|---|---|
| `schematic_001.rs` | Metasyntactic `part name(inputs)`, without an input type |
| `schematic_002.rs` | Explicit Unicode ellipsis |
| `schematic_003.rs` | Explicit Unicode ellipsis |
| `schematic_004.rs` | Explicit Unicode ellipsis |
| `schematic_005.rs` | Explicit Unicode ellipsis |
| `schematic_006.rs` | Explicit Unicode ellipsis |
| `schematic_007.rs` | Explicit Unicode ellipsis |
| `schematic_008.rs` | Explicit Unicode ellipsis |

`paper/omitted.json` records corresponding schematic roots in today's pages,
including their source and reason; they are not counted as complete roots.
`paper-refusals/generic_scope.rs` is the explicit generic-scope wish in
`docs/apis/again/kubernetes.md:405`. It is rejected because scope type parameters
are outside the grammar.

## Parsing choices and phase boundaries

The decisions in macro.md §2 are followed: whole-expression entry parsing,
lookahead for edit keywords and function-entry bodies, Rust-decoded templates,
strict hole spelling, and mandatory commas between arms, including block arms.
A body function is parsed as a Rust expression path, including qualified paths.
Statements after a part use the accepted second form of message 16, naming the
part in its label and help. Typed constructors require every varying argument;
there is no numeric lookup, binding dictionary or production example fallback.
Help for step 1 uses the captured author tokens, preferring Span::source_text.
Replacements are formatted once and second-place texts name their actual role.
The page comparison supplies its canonical examples only inside a test module.

Checks assigned to later steps are deferred: attributes, API name spelling,
input types, input/part lookup, status-key ranges, response field validity and
uniqueness, paths, headers and inheritance. Their written tokens remain in the
AST. Forms explicitly assigned a later diagnostic are also retained: client/part
spreads and part children (37), edits before whole statements (24), and body-entry
edits (22/23). This resolves the tension between the exact EBNF and the numbered
step assignments without implementing a later check in step 1. A returned tree
does not assert semantic validity. Steps 2–4 now consume that tree, as described below. Other syntax
errors, including children in endpoints and stray commas, use parser errors.

## Accepted review regressions

`unit/review.rs` labels each Astra finding A1–A7 and each Opus finding O1–O10.
O6 is the compile-fail harness in `ui.rs`. Parse failures mark the owning node,
entry block or arm block; sound children keep independent marks. Recovery uses
only the failed parse's remaining cursor, consumes the trailing semicolon, and
commits bookkeeping only for retained statements. DSL nodes are parsed on a work queue and the tree is assembled bottom-up. More
than 64 scopes is refused. A token safety pass bounds syn recursion; then an AST
visitor enforces the exact 64-level expression limit, preserving valid method
chains, closures, and the combination of 64 scopes with 64 expression levels.
The initial iterative token pass also protects construction of syn's token buffer.

## Names, resolution and checks (P3.2)

`unit/pipeline.rs` checks every row of case tables 1, 2 and 9, the two entry
orders, closed arrow spellings, lexical owner environments, and damaged facts.
All 180 parser attacks also run through the semantic stages without panics.
`semantic/coverage.tsv` maps each compiler fixture to its intended catalogue
number; a separate unit test verifies that the actual pipeline reaches it.
`ui.rs` checks all their `.stderr` files on stable Rust. The generator creates
source fixtures only and never updates expected compiler output.

`flat/` contains eleven full API descriptions copied from `paper/` and all five
tour clients copied from their leading DSL comments. Each `.flat` is
a checked-in plain-text list. Inputs retain the original declaration and expanded
owner. Every value retains its writer, placement node, spans, and lexical bindings.
Surviving query/header values print in evaluation order; `wire=N` retains the
key's first position, including across removal and later plain insertion.
`cross_base` marks the step-5 adapter gate, without attempting to infer a Secret
from a Rust type. Path pieces carry the same gate individually. Middlewares are
client construction facts, outside the endpoint lists. Exact arms precede class
arms; disjoint keys sort numerically for deterministic output.

The leading Shapes description and all 32 implemented endpoints agree with
`flat/tour_shapes.flat`: request pieces and expression order, readers, status
arms, field aliases, success wrappers and problem readers. This includes `held`
and the eleven endpoints added for the refusal and paging suites. No list/client
behavior difference remains.

The test copy of Files also includes the separate description comment from
`tour/src/grant.rs`, appended to the leading Files description before its closing
brace. Its golden therefore includes `files.grant`; the provenance test assembles
the same two comments afresh and checks that copy.

The architect authorized changing the legacy parser seam test to message 43 for
`client C { GET g(); }`. It is now named
`c1_18_an_endpoint_without_a_base_reports_message_43`; the valid-description test
still checks the explicit step-5 stop.

`unit/tour.rs` parses the hand-written Rust of all five clients plus grant. It
compares every reader's expression and visible input/owner bindings, every arm's
bindings and field aliases, and arm asking order with the flat origins. It also
checks that the retained method determines each call's conditional preparation
capability (`__prepare_if_none_match`). An independent part-reader/arm test checks
that part-local bindings survive separately from the containing node's bindings.
The model already retained all of this information; no production schema change
or step-5 generation was needed. The read value's generated name `__body` leaves
the authored `body: answer` alias beside an input named `body`, both retained by
the flat arm.

## P3.2 author readings (A39–A41)

`real/` adds all 97 Opus mistakes from complete `docs/apis/` descriptions and
three damage snapshots. `real/cases.json` records each exact mutation and its
API source. `make_real_fixtures.py --check` verifies those sources and the
compiled `unit/real_cases.rs` table; generation never changes `.stderr` files.
The unit test reconstructs every compiler fixture from its API copy, verifies
coverage of all 46 messages, and checks the repaired description through steps
1–4. Executable help text is extracted from the actual diagnostic and applied;
worded repairs use the page author's intended names, placements and values.
The plain-keyword Rust expression in `m04_plain` is refused by the parser; its
raw-identifier counterpart exercises message 4 and its repair.

`unit/readings.rs` also protects all seven Astra findings, including a separate
semantic assertion for each surviving mutation R12, R18, C12, N08 and N09. Its
16,000-key workload checks replacement evaluation order and original wire
positions after removal and reinsertion. Live entries are indexed by key.

A39 authorizes the changed expectations for conventional Rust token spacing,
truthful repair text and whole spans. A40 removes false absence diagnostics
under damage. The canonical catalogue examples remain compared with the page;
production supplies context-specific helps where those examples are unsuitable.
The repair tests also exposed query pairs conflicting with part entries, stale
source text on synthesized repairs, and body-removal advice that would leave
inputs unused; these now retain the intended data or use worded instructions.

P4.2 refreshes all five tour description copies and their flat lists from the
merged hand-written clients' leading comments. Riot follows A38 (no client arm,
queue and start_time inputs); Files follows A38/A41 (text/cursor body entries,
upload host, authorization override and idempotence, download clock); Shapes
now has 32 endpoints, including its refusal and ten paging endpoints. Tasks
changes statement source positions and Feed changes the pager's local name.
The 49-endpoint reader/arm comparison retains all original checks. The Tasks
expansion was regenerated from the macro and remains byte-for-byte unchanged.
`tour-gen` also checks that all six Tour and seventeen Conformance integration
files are included by path, including both C8 suites. No original test file or
expected response was changed. The supplemental body-composition test keeps
its original expected bytes with a dedicated description, since Files::search
now has the guide's simpler shape. Candidate flat lists can still be written
separately under `api/target/p3-2-flat-review` with
`CONCORD_REVIEW_GOLDENS=1`; that mode never overwrites an existing expected list.
