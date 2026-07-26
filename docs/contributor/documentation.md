# Documentation maintenance

Concord has one documentation site. Its mdBook configuration is `book.toml`,
its source is `docs/`, and its generated HTML is
`target/concord-docs/`. Rustdoc remains the exact authority for Rust items;
the book owns learning paths, feature operation, compositions, reference
navigation, architecture, runtime contracts, and maintenance.

## Build the site

Install the exact maintained generator:

```text
cargo install mdbook --version 0.5.4 --locked
```

Validate and build:

```text
just site
```

Validate once and start the local watch server:

```text
just site-serve
```

`just site` runs the documentation validator before `mdbook build`.
`just site-serve` validates before `mdbook serve`. Both render to the configured
`target/concord-docs/` output and neither rewrites tracked reference fragments.

`just docs` composes the site with all-feature workspace doctests and
warning-denied Rustdoc for the maintained no-default, selected-feature, and
all-feature product surfaces. It is a deliberate sample of important feature
boundaries rather than every possible Cargo feature combination.

## Site architecture

The navigation has one audience transition:

```text
Using Concord
  Start
  Core Guide
  Features
  Recipes
  Extending Concord
  Reference

Maintaining Concord
  Architecture
  Compiler
  Runtime contracts
  Development
```

The landing page starts a user journey; it is not a flattened sitemap. Start
pages produce the first declaration, built client, and executed request.
The Core Guide explains the ordinary model. Feature pages teach one capability.
Recipes compose capabilities for a concrete goal. Extension pages teach
downstream implementations. Reference pages own exhaustive exact facts.

Maintainer pages explain flow, ownership, reasons, invariants, failure modes,
extension boundaries, hazards, and verification. They are not source-file
inventories. Source and test paths are code-formatted text because links from
book Markdown to files outside `docs/` are intentionally rejected.

Every Markdown source page appears exactly once in `docs/SUMMARY.md`. A section
heading may group chapters but does not reserve an empty page. Do not add an
unlisted page or list one page under several sections.

## Canonical ownership

Before writing a fact, choose one canonical owner:

| Fact | Canonical owner |
|---|---|
| exact Rust signature and item behavior | Rustdoc |
| declaration grammar and exhaustive forms | `docs/reference/dsl.md` |
| generated application shape | `docs/reference/generated-api.md` |
| feature inventory | façade Cargo metadata plus generated feature fragment |
| numeric defaults and ceilings | linked core/extras constants plus generated limits fragment |
| error categories and focused kinds | core Rustdoc plus `docs/reference/errors.md` |
| compatibility and MSRV | `docs/reference/compatibility.md` |
| user workflow | one Start, Core Guide, or feature page |
| concrete composition | one recipe and its canonical example |
| runtime ordering | managed-executor architecture and owning core tests |
| package boundaries | crate-boundaries architecture and workspace manifests |
| commands | `justfile` and owning xtask |
| CI execution | `.github/workflows/ci.yml` |
| release state | product manifests, package xtask, changelog, release page |

Other pages summarize the fact and link to that owner. Do not duplicate the
DSL table, numeric limits, error taxonomy, lifecycle sequence, or extension
trust contract across several chapters.

Generated-control prose must treat absence and a declaration default as
independent capabilities. A defaulted `Option<T>` or `Option<Vec<T>>` exposes
`set_*`, `clear_*`, and `reset_*`; `set_*` accepts the inner value. A guide that
mentions one control should make explicit whether absence and default
restoration also apply, then link to the generated API reference.

## Canonical example includes

Complete book examples live under `crates/concord-examples/src` and are
compiled by that crate and its tests. Book pages include named source regions:

```text
// ANCHOR: example_name
// complete canonical excerpt
// ANCHOR_END: example_name
```

<pre><code>&#123;&#123;#include ../../crates/concord-examples/src/path.rs:example_name&#125;&#125;</code></pre>

Use a stable descriptive anchor name, one start and one end marker, and the
shortest complete region that demonstrates the page task. Do not copy the
same complete program into Markdown. Keep loopback execution in the example
owner when the narrative depends on real request behavior.

The xtask validator resolves every include against the repository root,
rejects escapes and temporary execution aids, and verifies that each named
anchor exists once and is ordered correctly. The example crate remains the
compiler owner; the whole book is not delegated to `mdbook test`.

## Generated reference fragments

The feature and limit tables have canonical source inputs. Regenerate only
after those inputs change:

```text
cargo run --locked -p concord-xtask -- docs --write --locked
```

Then verify without mutation:

```text
cargo run --locked -p concord-xtask -- docs --locked
```

The feature table comes from typed Cargo metadata. The limit table links core
and extras and reads exported normative constants. Hand-edit neither generated
region: make the source fact correct and run `--write`.

## Validator contract

The docs xtask uses a Markdown parser rather than regular expressions for
navigation and links. It checks:

- generated feature and limit fragments are current;
- every SUMMARY target exists, stays under `docs/`, is Markdown, and appears
  only once;
- SUMMARY is the exact Markdown-page inventory;
- every include target and named anchor is valid and repository-contained;
- every local file and heading target resolves;
- Markdown source links do not leave `docs/`;
- retired user-facing vocabulary does not return.

External URLs are syntax-counted but not fetched. Network link health must be
checked and reported separately; a passing docs xtask does not establish it.
The validator also does not replace Rustdoc, compiler diagnostics, workflow
lint, or visual browser review.

When adding a validator rule, include positive and negative unit tests in
`tools/xtask/src/docs.rs`. Keep the rule about a stable repository invariant,
not a one-time migration artifact.

## Redirects and URL changes

Use mdBook's built-in redirect map in `book.toml` when a previously published
chapter moves. Point each old HTML path directly at its final canonical page;
do not create Markdown redirect stubs or chains.

Before removing an old source:

1. map every heading to a retained, rewritten, or intentionally deleted owner;
2. migrate incoming local links;
3. add the old HTML redirect;
4. validate content parity and the rendered redirect file;
5. remove the obsolete Markdown source;
6. let strict SUMMARY inventory prove there is no orphan.

Changing a heading can break an anchor even when the file path is stable.
Search incoming links and update them in the same change.

## Writing user pages

User pages lead with the task and show a working default early. Explain what
Concord handles automatically at the moment it becomes relevant, state
limitations beside the affected behavior, and link to advanced or exact
material.

Use `Complete example` and `Related` as stable wayfinding labels. Avoid
compiler/runtime-maintainer vocabulary in beginner prose. Do not expose
generated-private plumbing as something an application should call.

Recipes use this exact task structure:

```text
Goal
Complete code
Why these declarations
Adaptations
Production notes
Related
```

A recipe solves a composition; it does not repeat an entire feature guide or
reference grammar.

## Writing maintainer pages

A runtime-contract page should include, where relevant:

```text
Purpose
Public model
Internal flow
Architectural invariants
Bounds
Errors and redaction
Extension boundary
Failure modes
Change hazards
Verification
```

Describe why ordering and boundaries exist. Separate stable invariants from
today's helper names and module layout. Do not turn every implementation detail
into a normative rule.

Use small text diagrams only when they clarify a flow or boundary. Keep tables
narrow enough for mobile rendering; move long explanations below the table.

## Rendered review

Automated HTML generation does not establish usability. For a site-wide
change, inspect:

- landing and every Start page;
- one complete Core Guide path;
- authentication, streams/records, pagination, and rate limiting;
- one recipe and one extension page;
- DSL reference, architecture overview, and managed executor;
- navigation, tables, code blocks, redirects, and search at desktop and
  mobile widths.

Check search using representative tasks, exact type names, and maintainer
terms. Confirm that results lead to the canonical owner rather than several
near-duplicate chapters. Record when an interactive browser or viewport is
unavailable; static HTML inspection is not the same evidence.

## Change checklist

Before committing documentation:

1. run the owning example and test suites;
2. run the docs xtask without `--write`;
3. build mdBook with 0.5.4;
4. inspect changed rendered pages and navigation;
5. verify old redirects affected by the change;
6. run doctests and warning-denied Rustdoc when public contracts changed;
7. run `git diff --check`;
8. inspect status for rendered output, review artifacts, and temporary files.

For a release-facing site change, use the complete matrix in
[Testing strategy](testing.md) and the archive checks in
[Release process](release.md).

## Related

- [Workspace and adding a feature](workspace.md)
- [Testing strategy](testing.md)
- [Generated contract versioning](generated-contract.md)
- [Release process](release.md)
