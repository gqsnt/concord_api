#!/usr/bin/env python3
"""Writes findings.json, the machine-readable index of the 4.4 review findings (NEXT_REVIEW_4_4.md
is the narrative). Run from the repository root:

    python3 -I v4/extensions-real-apis/findings_index.py

Each finding keeps the reviewer's classification, severity and evidence strength; `disposition` is
the coordinator's resolution; `resolution` names the commit/test for fixed findings or the
recommendation it feeds. Full findings with reproductions: review/<strand>/FINDINGS.md.
"""

import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))

# id, title, classification, severity, evidence, recurrence, disposition, resolution, recommendation
F = [
    # Terra — DSL and operation authoring
    ("TERRA-1", "Downstream operations cannot reuse generated scope composition as a typed operation",
     "developer friction", "medium", "reproduced (GitHub extension), source-inspected (Kubernetes)",
     "GitHub SDK, GitHub maintenance exercise, Kubernetes maintenance exercise",
     "proposed for next release (bounded opt-in prototype)", None, "R5"),
    ("TERRA-2", "Response-head observation repeats transport and decode plumbing",
     "developer friction", "medium", "source-inspected; SDK tests reproduced",
     "GitHub, Stripe, Salesforce, Shopify SDKs (independently written)",
     "proposed for next release", None, "R1, R2"),
    ("TERRA-3", "Shared endpoint parameter sets are redeclared", "developer friction", "medium",
     "reproduced (Kubernetes declarations); maintenance diff", "Kubernetes (copied within one SDK), GitHub per_page",
     "proposed for next release (bounded prototype)", None, "R5"),
    ("TERRA-4", "Per-request inherited header overrides repeat at every endpoint", "developer friction",
     "medium", "reproduced (Stripe wire tests)", "Stripe only (19 endpoints)",
     "proposed for next release (with TERRA-3)", None, "R5"),
    ("TERRA-5", "Generic adapters require handwritten traits over generated requests", "developer friction",
     "medium", "source-inspected; adapters' tests reproduced", "Kubernetes, Stripe (independent); GitHub uses fn pointers",
     "proposed for next release (narrow experiment)", None, "R5"),
    ("TERRA-6", "Stored filter records duplicate declarations and evolve by Rust struct rules",
     "compatibility risk", "medium", "source-inspected; maintenance observations", "GitHub, Stripe, Salesforce maintenance",
     "fixed now (guidance)", "docs/AUTHOR_GUIDE.md §11 (commit 916d541)", "R6"),
    ("TERRA-7", "Built-in flat Form cannot represent Stripe's nested body", "developer friction", "medium",
     "reproduced", "Stripe only", "not adopted now: needs a second verified consumer", None, "R7"),
    # Sol — defects and compatibility
    ("SOL-1", "CSV writer validates one serialization and writes another", "correctness defect", "medium",
     "reproduced", "one shared encoder", "fixed now", "commits cff5e35, a6a33a4 (tests/csv_write_integrity.rs)", None),
    ("SOL-2", "Empty chunks bypass cooperative work accounting", "performance-resource limit", "medium",
     "reproduced", "shared driver (all readers)", "fixed now",
     "commits 86f1472, 175e5c0 (tests/stream_budget.rs)", None),
    ("SOL-3", "Chunks Debug exposes response content", "correctness defect", "low", "reproduced",
     "one helper", "fixed now", "commits 86f1472, 175e5c0 (tests/stream_foundation.rs)", None),
    ("SOL-4", "Generated Clone binder collides with a payload lifetime named '__concord", "correctness defect",
     "low", "reproduced", "one macro rule", "fixed now",
     "commit 7262007 (checks/clone-hygiene a_payload_lifetime_named_like_the_binder_keeps_clone)", None),
    ("SOL-5", "Yield docs misclassify a malformed final line", "documentation gap", "low", "reproduced",
     "one policy", "fixed now (docs)", "commits 86f1472, 175e5c0", None),
    ("SOL-6", "Discovery-failure outcome guarantee needs a caller-stop qualification", "documentation gap",
     "low", "reproduced", "one traversal", "fixed now (docs + test)",
     "commit 916d541 (finish_after_a_discovery_failure_reports_stopped_and_keeps_the_failure)", None),
    ("SOL-7", "FromIter unnecessarily requires Unpin", "developer friction", "low", "reproduced",
     "one adapter", "fixed now", "commits 86f1472, 175e5c0 (tests/stream_foundation.rs)", None),
    ("SOL-8", "Repeated native send plumbing encodes an impossible success branch", "developer friction",
     "medium", "source-inspected across independent SDKs", "GitHub, Stripe, Salesforce, Shopify, GitHub maintenance",
     "proposed for next release", None, "R1, R2"),
    ("SOL-9", "'Add an optional value: unchanged' omits loss of generated Clone", "compatibility risk", "medium",
     "reproduced", "one macro rule", "fixed now (guidance)", "docs/AUTHOR_GUIDE.md §11 (commit 916d541)", "R6"),
    # Astra — core/native boundaries and streaming
    ("ASTRA-1", "CSV escape dialects lose literal escape bytes on round-trip", "correctness defect", "high",
     "reproduced", "one shared writer", "fixed now",
     "commit cff5e35 (tests/csv_write_integrity.rs escape round trips; DECISIONS CSV-16)", None),
    ("ASTRA-2", "CSV writer limits checked after allocating the oversized record", "performance-resource limit",
     "medium", "reproduced", "one shared writer", "fixed now",
     "commits cff5e35, a6a33a4; header names after the Astra re-check: e27a0958, 843416d9 (tests/csv_write_memory.rs, csv_write_integrity.rs; DECISIONS CSV-17/18)", None),
    ("ASTRA-3", "Empty chunks can monopolize the executor", "performance-resource limit", "medium",
     "reproduced", "same as SOL-2", "fixed now", "commits 86f1472, 175e5c0", None),
    ("ASTRA-4", "JSON framing applies the work budget too late", "performance-resource limit", "medium",
     "reproduced", "JSONL and JSON values readers", "fixed now",
     "commits 86f1472, 175e5c0 (tests/stream_budget.rs)", None),
    ("ASTRA-5", "Preserving metadata requires repeated native send/error-conversion scaffolding",
     "developer friction", "medium", "reproduced; cross-checked", "same cause as SOL-8/TERRA-2",
     "proposed for next release", None, "R1, R2"),
    # Opus — pagination and caller workflows
    ("OPUS-1", "GitHub LinkPages::restore re-binds any API URL", "correctness defect", "medium", "reproduced",
     "GitHub only", "fixed now",
     "commit d4578fcf; numeric links kept their repository only after Sol's re-check: b528bc79 (stored form with binding; tests/paging.rs stored_continuation_keeps_its_repository)", None),
    ("OPUS-2", "Non-advancing continuations accepted by three adapters", "correctness defect", "low",
     "reproduced (GitHub), source-inspected (Kubernetes, Salesforce)", "3 of 5 SDK adapters",
     "fixed now", "commits d4578fcf (GitHub), 36b0adc4 (Kubernetes, Salesforce; tests/continuations.rs, ku_a_inventory.rs); edge cases from Sol's re-check: SOL-R1, SOL-R2", None),
    ("OPUS-3", "Items hides the page: metadata dropped, replayed prefix unverifiable", "developer friction",
     "medium", "reproduced + source", "GitHub F9, Stripe F6 (independent)",
     "doc sentence fixed now; accessor proposed for next release", "commit 7ac6ca6a (sequential.rs resume_items docs)", "R3"),
    ("OPUS-4", "Restart composes two traversal types; consumers disagree on budget semantics",
     "developer friction", "medium", "source-inspected in two consumers", "SF-B and DX-03 (independent caller)",
     "proposed for next release", None, "R3"),
    ("OPUS-5", "manifest_jobs cannot express a strictly serial provider", "speculative opportunity", "low",
     "reproduced (timing)", "GitHub, Stripe, Shopify hand-roll nesting", "blocked by missing evidence", None, None),
    ("OPUS-6", "SH-A nested failure: misleading parent summary, unprocessed siblings", "correctness defect",
     "low", "reproduced", "Shopify only", "fixed now",
     "commit 585f7fea (sh_a_nested_failure_mid_page_resumes_at_the_interrupted_parent_page); page replay on resume documented after Sol's re-check: 66fde2ac", None),
    ("OPUS-7", "Same drain/error-stash/outcome loop written ~13 times", "developer friction", "low",
     "source-inspected", "five apps (Shopify F5, Kubernetes F6, Salesforce F10)", "proposed for next release (low)",
     None, "R3"),
    ("OPUS-8", "Much of the pagination public surface has no real consumer", "compatibility risk",
     "medium (if published)", "source-inspected", "n/a", "proposed for next release (mark provisional)", None, "R3"),
    ("OPUS-9", "PAG-08 JSON-record full-job consumer absent", "requirement-evidence gap", "low",
     "reproduced", "n/a", "fixed now (test)", "commit 77455a7b (checks/stream-stress/tests/jsonl_jobs.rs)", None),
    ("OPUS-10", "concord-pagination not discoverable from README/RECIPES/CHANGELOG", "documentation gap", "low",
     "source-inspected", "n/a", "fixed now (docs)", "commit 7ac6ca6a (README, docs/RECIPES.md, CHANGELOG.md)", None),
    ("OPUS-11", "Reports name the start key of a failed multi-page job", "documentation gap", "low",
     "reproduced", "n/a", "fixed now (docs)", "commit 7ac6ca6a (independent.rs, jobs.rs docs)", None),
    # Sol resolution re-check of the OPUS fixes and the ASTRA-2 follow-up (commit d2681065)
    ("SOL-R1", "A next link differing only by fragment bypasses GitHub's progress check", "correctness defect",
     "low", "reproduced", "GitHub only", "fixed now",
     "commit b528bc79 (tests/paging.rs next_link_differing_only_by_fragment_does_not_advance)", None),
    ("SOL-R2", "Kubernetes first fetch sends a template's continue token but checks against none",
     "correctness defect", "low", "reproduced", "Kubernetes only", "fixed now",
     "commit fc166290 (ku_a_inventory.rs ku_a_a_continue_token_on_the_template_is_not_sent)", None),
    ("SOL-R3", "A named link after a repository rename cannot be restored through the original factory",
     "compatibility risk", "low", "reproduced (conditional loopback scenario)", "GitHub only", "fixed now",
     "commit b528bc79 (stored form keeps the binding; stored_continuation_keeps_its_repository)", None),
]


def main():
    out = {
        "label": "4.4 is the requested review label; the package/release version remains a separate decision.",
        "reviewed_commit": "0dd655018892a8cae21479f1cd249d2c875f6c83",
        "strands": {
            "SOL": "Codex gpt-6.1-sol xhigh (observed), defects and compatibility",
            "TERRA": "Codex gpt-6.1-terra xhigh (observed), DSL and operation authoring",
            "ASTRA": "Codex gpt-6-astra xhigh (observed), core/native boundaries and streaming",
            "OPUS": "Claude subagent (coordinator model claude-opus-5-5 inherited; not reported by the harness), pagination and workflows, cross-check",
        },
        "resolution_rechecks": {
            "astra-recheck": "Codex gpt-6-astra xhigh (observed), stream and CSV fixes at ea68333d: ACCEPT_WITH_FIXES (ASTRA-2 header names), fixed in e27a0958/843416d9",
            "sol-recheck": "Codex gpt-6.1-sol xhigh (observed), OPUS fixes and ASTRA-2 follow-up at d2681065: ACCEPT_WITH_FIXES (OPUS-1 numeric links, SOL-R1..R3, OPUS-6 replay), fixed in b528bc79/fc166290/66fde2ac; follow-up check recorded in review/sol-recheck/",
        },
        "findings": [
            {
                "id": i, "title": t, "classification": c, "severity": s, "evidence": e,
                "recurrence": r, "disposition": d, "resolution": res, "recommendation": rec,
                "source": "review/sol-recheck/FINDINGS.md" if i.startswith("SOL-R")
                else f"review/{i.split('-')[0].lower()}/FINDINGS.md",
            }
            for (i, t, c, s, e, r, d, res, rec) in F
        ],
    }
    with open(os.path.join(HERE, "findings.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, indent=1)
        f.write("\n")
    fixed = sum(1 for x in F if x[6].startswith("fixed"))
    print(f"{len(F)} findings, {fixed} fixed now")


if __name__ == "__main__":
    main()
