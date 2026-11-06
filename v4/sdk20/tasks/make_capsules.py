#!/usr/bin/env python3
"""Writes the author, first-use consumer and maintenance task capsules of one Phase B wave.

Usage: python3 -I make_capsules.py HANDOFF_ROOT WAVE FROZEN_SHA OUT_DIR

Inputs are the handoff's cohort (providers.json for the assignment, COHORT.md for the provider
brief). Capsules state outcomes and contracts; they never contain an author's solution.
"""

import json
import pathlib
import re
import sys

FAMILY = {
    "sol": "Codex gpt-6.1-sol (xhigh)",
    "terra": "Codex gpt-6.1-terra (xhigh)",
    "astra": "Codex gpt-6-astra (xhigh)",
    "opus": "Claude Opus subagent",
}

COMMON = """\
## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree of the frozen candidate commit `{sha}`. Prefix every cargo command
  with `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR={target}` (a shared build cache; if cargo says it is
  waiting for a lock, wait).
- **Frozen library:** do not edit `crates/`, `scripts/`, `docs/`, `checks/` or other examples.
  If Concord forces a workaround (private reach-through, rebuilt execution, handwritten operation
  names, a native escape you think should not be needed), keep the working code and record it as
  a friction entry with a reproduction; do not patch the library.
- Use Concord's public API (`concord`, `concord-pagination`, `concord-extras` through the facade)
  and its docs: `README.md`, `docs/*.md`, crate rustdoc (`cargo doc -p concord --no-deps
  --features csv,jsonl`), `crates/*/README.md`. The five existing provider SDKs under `examples/`
  are normal reading material (say so if one supplied your solution).
- No live provider calls, no credentials, no secrets: all auth values are synthetic and must not
  look like real credentials. Tests run offline against the loopback server in
  `examples/testserver` (a dev-dependency by path).
- Fixture bytes are hand-authored from the official documentation unless you actually obtained a
  documented example; record provenance honestly. Put byte-sensitive fixtures under
  `examples/{slug}-sdk/fixtures/` and keep `examples/{slug}-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.
"""

AUTHOR = """\
# {id} {provider} — SDK author ({size} slice)

Requested runner: {author}. You implement this SDK from scratch.

{common}
## The locked provider brief

{brief}

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
{sources}

Machine-readable scope: {scope}. Excluded: {exclusions}.
Workflows: {workflows}.

## Deliverables (all under `examples/{slug}-sdk/`)

1. A crate named `concord-sdk-{slug}` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/{slug}-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
   root edit allowed; `Cargo.lock` may change accordingly).
2. A normal public developer entry point: a client type with documented construction
   (auth/context configuration), typed domain operations for the whole declared scope, typed
   provider errors that keep useful response metadata, and the required pagination / streaming /
   body / completion behaviour. Built with `concord::api!` and Concord's public types. A mature
   third-party crate is appropriate for generic protocols (signing, XML parsing) — record why you
   chose it. Never delegate HTTP execution to a vendor SDK.
3. `README.md`: what the SDK covers and excludes, construction, both workflows as code, errors,
   completion/partial-outcome semantics, limitations, and that fixtures are not vendor
   conformance.
4. Tests: `tests/protocol.rs` (wire-level request assertions: method, target, headers, exact body
   bytes; documented success and failure shapes; meaningful negative cases from the brief) and
   `tests/workflows.rs` (workflow A and B end to end through the public API against fixtures).
   Do **not** create `tests/first_use.rs` (reserved for the independent consumer).
5. `PROVENANCE.md`: fixture file → official page/section + version + access date →
   transformations; which cases are synthetic.
6. `FRICTION.md`: ordered factual entries — what Concord made easy, what you had to work around,
   each with a file:line reproduction and whether you think it is an SDK, provider, application,
   docs or Concord cause. Include anything in the declaration, the adapter code, the caller path
   or error handling that felt like reconstructing library work.

Commands: `cargo test --locked -p concord-sdk-{slug}`, `cargo clippy --locked -p
concord-sdk-{slug} --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-{slug}` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-{slug}` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
"""

CONSUMER = """\
# {id} {provider} — independent first-use consumer

Requested runner: {consumer}. You did not write this SDK and have not seen its author's notes.

{common}
## Your situation

You are an application developer adopting `concord-sdk-{slug}` (in `examples/{slug}-sdk/`). Use
its `README.md` and rustdoc (`cargo doc -p concord-sdk-{slug} --no-deps`) and Concord's public
docs. Reading the SDK's `src/` or its tests counts as an intervention: record when and why. Do not
read `FRICTION.md`. Do not modify the SDK's `src/`, `README.md` or existing tests.

## Business outcomes

{outcomes}

Provider facts you may rely on (from the official documentation): see the sources below and the
SDK's README. Sources: {sources}

## What to build

`examples/{slug}-sdk/tests/first_use.rs`, starting from an empty file: an application-style test
suite that reaches both outcomes through the SDK's public API against a local fake of the provider
that **you** write from the documentation (use `concord_example_testserver`; you may reuse the
SDK's documented fixture files only if the README points to them). Check the wire requests you
expect, the results, and the meaningful failure/completion cases (a failed page, a provider error
document, a stream that ends early, …) — whatever the outcome makes relevant.

Before writing each outcome, write your prediction (request, return value, how completion and
failure are signalled) into `examples/{slug}-sdk/FIRST_USE.md`; then keep a short ordered trace
there: first entry point, docs used, distinct compiler diagnostics, interventions (source reads,
native escapes), the working path, and concrete friction. No numeric scores.

Command: `cargo test --locked -p concord-sdk-{slug} --test first_use`.
Final message: test count passing, the first intervention (if any), and the top friction points.
"""

MAINTAINER = """\
# {id} {provider} — maintenance change

Requested runner: {maintainer}. You did not write this SDK.

{common}
## The change ticket (chosen before you saw the SDK)

{perturbation}

Do it in `examples/{slug}-sdk/` (source, README, tests) preserving the existing contract: the
existing workflows and first-use tests must keep passing unless the real provider contract
requires caller changes (then show and justify them). Add tests that exercise the change on the
wire. Do not read `FRICTION.md` or `FIRST_USE.md` before finishing a first attempt.

Write `examples/{slug}-sdk/MAINTENANCE.md`: the edits by kind (declaration, adapter, model,
tests, docs), how many independent places encode the same fact and had to change, what you had
to read to find them, any generated-name or compile surprise, and whether the change felt local.

Command: `cargo test --locked -p concord-sdk-{slug}` (all tests).
Final message: files changed, tests passing, and whether one semantic change touched several
independently maintained descriptions of the same fact.
"""


def brief(cohort: str, provider: str) -> str:
    """The `### N. Provider ...` section of COHORT.md."""
    sections = re.split(r"\n(?=### \d+\. )", cohort)
    for section in sections:
        head = section.splitlines()[0]
        if head.startswith("### ") and provider.split()[0].lower() in head.lower():
            return section.split("\n## ")[0].strip()
    raise SystemExit(f"no brief for {provider}")


def main() -> int:
    root, wave, sha, out = pathlib.Path(sys.argv[1]), int(sys.argv[2]), sys.argv[3], pathlib.Path(sys.argv[4])
    cohort = (root / "cohort" / "COHORT.md").read_text()
    providers = json.loads((root / "cohort" / "providers.json").read_text())["providers"]
    out.mkdir(parents=True, exist_ok=True)
    for i, p in enumerate(q for q in providers if q["wave"] == wave):
        target = f"/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-{'ab'[i % 2]}"
        text = brief(cohort, p["provider"])
        fields = dict(
            id=p["id"], provider=p["provider"], slug=p["slug"], size=p["size"], sha=sha,
            author=FAMILY[p["author_family"]], consumer=FAMILY[p["consumer_family"]],
            maintainer=FAMILY[p["maintainer_family"]], brief=text,
            sources="\n".join(f"- {u}" for u in p["official_sources"]),
            scope="; ".join(p["scope"]), exclusions="; ".join(p["exclusions"]),
            workflows="; ".join(f"{w['id']}: {w['goal']}" for w in p["workflows"]),
            outcomes="\n".join(f"- **{w['id']}.** {w['goal']}." for w in p["workflows"]),
            perturbation="\n".join(f"- {m}" for m in p["maintenance"])
            + "\n\nThe provider brief's perturbation paragraph, verbatim:\n\n"
            + next((l for l in text.splitlines() if l.startswith("- Perturbation:")), ""),
            target=target,
        )
        fields["common"] = COMMON.format(**fields)
        for role, template in (("author", AUTHOR), ("consumer", CONSUMER), ("maintainer", MAINTAINER)):
            (out / f"{p['id']}-{p['slug']}-{role}.md").write_text(template.format(**fields))
    return 0


if __name__ == "__main__":
    sys.exit(main())
