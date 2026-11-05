#!/usr/bin/env python3
"""Writes requirements.json: the handoff's requirement list (requirements.handoff.json, unchanged)
with implementation evidence per ID. Run from the repository root:

    python3 -I v4/extensions-real-apis/requirements_evidence.py

Statuses: DONE (implemented and checked by the cited tests/commands), DONE_OFFLINE (implemented and
verified offline; the live tier is NOT_RUN), PENDING (waiting on a step recorded in STATUS.md).
The final statuses are set when the delivery is recorded; this script is the single source.
"""

import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))

E = {
    "INT-01": ("DONE", ["v4/dx-4.3/CI_FAILURE_RESOLUTION.md", "v4/dx-4.3/STATUS.md"],
               ["gh pr view 2 --repo gqsnt/concord_api"]),
    "INT-02": ("DONE", ["v4/dx-4.3/CI_FAILURE_RESOLUTION.md", "scripts/socket-snapshot.py",
                        "examples/riot/tests/fixture_lifecycle.rs"],
               ["cargo test -p concord-example-riot --test fixture_lifecycle",
                "python3 -I -B -m unittest discover -s scripts/tests -p 'test_socket*.py'"]),
    "INT-03": ("DONE", ["v4/dx-4.3/VERIFICATION.md"], ["CI runs 41931697578, 41931702050 (ae5618a)"]),
    "INT-04": ("DONE", ["v4/dx-4.3/STATUS.md#integration"], ["CI run 41953231307 (fd4771a)"]),
    "INT-05": ("DONE", ["v4/dx-4.3/STATUS.md", "v4/extensions-real-apis/STATUS.md"],
               ["CI run 41956577954 (receipt d180254): Linux 37/37, native Windows 37/37, nightly 16/0"]),
    "BASE-01": ("DONE", ["v4/extensions-real-apis/STATUS.md", "v4/extensions-real-apis/HISTORY_REWRITE.md"], []),
    "BASE-02": ("DONE", ["v4/extensions-real-apis/STATUS.md", "v4/extensions-real-apis/FIXTURES.md",
                         "examples/github-sdk/PROVIDER.md", "examples/stripe-sdk/PROVIDER.md",
                         "examples/shopify-sdk/PROVIDER.md", "examples/salesforce-sdk/PROVIDER.md",
                         "examples/kubernetes-sdk/PROVIDER.md"], []),
    "CSV-01": ("DONE", ["crates/concord-extras/src/csv/", "examples/extension/tests/dsl.rs"],
               ["cargo test -p concord-extras --features csv --test csv_dialect",
                "cargo test -p concord-example-extension --test dsl"]),
    "CSV-02": ("DONE", ["crates/concord-extras/src/csv/", "checks/dx03-csv-runtime/"],
               ["cargo test -p concord-extras --features csv --test csv_dialect",
                "cargo test -p concord-check-dx03-csv-runtime"]),
    "CSV-03": ("DONE", ["crates/concord-extras/tests/csv_mapping.rs"],
               ["cargo test -p concord-extras --features csv --test csv_mapping"]),
    "CSV-04": ("DONE", ["crates/concord-extras/tests/csv_dialect.rs", "crates/concord-extras/tests/csv_fixtures.rs"],
               ["cargo test -p concord-extras --features csv"]),
    "CSV-05": ("DONE", ["crates/concord-extras/tests/csv_dialect.rs", "crates/concord-extras/tests/csv_write.rs"],
               ["cargo test -p concord-extras --features csv"]),
    "CSV-06": ("DONE", ["crates/concord-extras/tests/csv_write.rs"],
               ["cargo test -p concord-extras --features csv --test csv_write"]),
    "CSV-07": ("DONE", ["crates/concord-extras/tests/csv_limits.rs", "crates/concord-extras/tests/csv_http.rs",
                        "checks/stream-stress/"],
               ["cargo test -p concord-extras --features csv", "cargo test -p concord-check-stream-stress"]),
    "CSV-08": ("DONE", ["examples/extension/"], ["cargo test -p concord-example-extension"]),
    "JSON-01": ("DONE", ["crates/concord-extras/src/jsonl/", "crates/concord-extras/tests/jsonl_framing.rs",
                         "crates/concord-extras/tests/jsonl_http.rs"],
                ["cargo test -p concord-extras --features jsonl"]),
    "JSON-02": ("DONE", ["crates/concord-extras/src/jsonl/values.rs",
                         "crates/concord-extras/tests/json_values_framing.rs"],
                ["cargo test -p concord-extras --features jsonl --test json_values_framing"]),
    "STREAM-01": ("DONE", ["crates/concord-extras/src/stream/error.rs", "crates/concord-extras/tests/csv_http.rs",
                           "crates/concord-extras/tests/jsonl_http.rs"],
                  ["cargo test -p concord-extras --all-features"]),
    "STREAM-02": ("DONE", ["v4/extensions-real-apis/STRESS.md", "crates/concord-extras/tests/jsonl_incremental.rs",
                           "crates/concord-extras/tests/csv_limits.rs"],
                  ["cargo run --release -p concord-check-stream-stress -- --mib 256"]),
    "STREAM-03": ("DONE", ["crates/concord-extras/src/stream/upload.rs", "crates/concord-extras/tests/jsonl_upload.rs",
                           "checks/stream-stress/tests/smoke.rs"],
                  ["cargo test -p concord-extras --all-features", "cargo test -p concord-check-stream-stress"]),
    "STREAM-04": ("DONE", ["checks/stream-stress/tests/smoke.rs", "crates/concord-extras/tests/jsonl_http.rs",
                           "crates/concord-extras/tests/csv_http.rs"],
                  ["cargo test -p concord-check-stream-stress"]),
    "STREAM-05": ("DONE", ["crates/concord-extras/src/stream/error.rs"],
                  ["cargo test -p concord-extras --all-features"]),
    "PAG-01": ("DONE", ["crates/concord-pagination/tests/sequential.rs", "examples/github-sdk/src/paging.rs",
                        "examples/stripe-sdk/src/pages.rs"],
               ["cargo test -p concord-pagination --test sequential"]),
    "PAG-02": ("DONE", ["crates/concord-pagination/tests/independent.rs", "crates/concord-pagination/tests/first_page.rs"],
               ["cargo test -p concord-pagination"]),
    "PAG-03": ("DONE", ["crates/concord-pagination/tests/jobs.rs", "examples/salesforce-app/tests/sf_b_bulk_csv_round_trip.rs"],
               ["cargo test -p concord-pagination --test jobs", "cargo test -p concord-example-salesforce-app"]),
    "PAG-04": ("DONE", ["crates/concord-pagination/tests/sequential.rs", "examples/kubernetes-sdk/src/list.rs",
                        "examples/shopify-sdk/"], ["cargo test -p concord-pagination"]),
    "PAG-05": ("DONE", ["crates/concord-pagination/tests/independent.rs", "crates/concord-pagination/tests/jobs.rs"],
               ["cargo test -p concord-pagination"]),
    "PAG-06": ("DONE", ["crates/concord-pagination/tests/items.rs", "crates/concord-pagination/tests/independent.rs"],
               ["cargo test -p concord-pagination"]),
    "PAG-07": ("DONE", ["crates/concord-pagination/tests/independent.rs"], ["cargo test -p concord-pagination"]),
    "PAG-08": ("DONE", ["crates/concord-pagination/tests/jobs.rs", "examples/salesforce-app/tests/sf_b_bulk_csv_round_trip.rs",
                        "checks/dx03-fanout-sink/", "checks/stream-stress/tests/jsonl_jobs.rs"],
               ["cargo test -p concord-example-salesforce-app", "cargo test -p concord-check-dx03-fanout-sink",
                "cargo test -p concord-check-stream-stress --test jsonl_jobs"]),
    "PAG-09": ("DONE", ["crates/concord-pagination/", "scripts/check-graph.py", "scripts/package-check.sh",
                        "scripts/verify.sh"], ["scripts/verify.sh --package --bench"]),
    "SDK-BASE": ("DONE", ["v4/extensions-real-apis/SDK_BRIEF.md", "examples/*-sdk/", "examples/*-app/"],
                 ["scripts/verify.sh (test-examples-isolated)"]),
    "GH-A": ("DONE_OFFLINE", ["examples/github-app/tests/gh_a_activity_audit.rs"],
             ["cargo test -p concord-example-github-app --test gh_a_activity_audit"]),
    "GH-B": ("DONE_OFFLINE", ["examples/github-app/tests/gh_b_artifact_retrieval.rs"],
             ["cargo test -p concord-example-github-app --test gh_b_artifact_retrieval"]),
    "ST-A": ("DONE_OFFLINE", ["examples/stripe-app/tests/st_a_billing_reconciliation.rs"],
             ["cargo test -p concord-example-stripe-app --test st_a_billing_reconciliation"]),
    "ST-B": ("DONE_OFFLINE", ["examples/stripe-app/tests/st_b_form_idempotency.rs"],
             ["cargo test -p concord-example-stripe-app --test st_b_form_idempotency"]),
    "SH-A": ("DONE_OFFLINE", ["examples/shopify-app/tests/sh_a_catalog_inventory.rs"],
             ["cargo test -p concord-example-shopify-app --test sh_a_catalog_inventory"]),
    "SH-B": ("DONE_OFFLINE", ["examples/shopify-app/tests/sh_b_bulk_export.rs"],
             ["cargo test -p concord-example-shopify-app --test sh_b_bulk_export"]),
    "SF-A": ("DONE_OFFLINE", ["examples/salesforce-app/tests/sf_a_crm_extraction.rs"],
             ["cargo test -p concord-example-salesforce-app --test sf_a_crm_extraction"]),
    "SF-B": ("DONE_OFFLINE", ["examples/salesforce-app/tests/sf_b_bulk_csv_round_trip.rs",
                              "examples/salesforce-app/tests/sf_b_ingest_round_trip.rs"],
             ["cargo test -p concord-example-salesforce-app"]),
    "KU-A": ("DONE_OFFLINE", ["examples/kubernetes-app/tests/ku_a_inventory.rs"],
             ["cargo test -p concord-example-kubernetes-app --test ku_a_inventory"]),
    "KU-B": ("DONE_OFFLINE", ["examples/kubernetes-app/tests/ku_b_list_watch.rs",
                              "examples/kubernetes-app/tests/ku_b_segmentation.rs"],
             ["cargo test -p concord-example-kubernetes-app"]),
    "DATA-01": ("DONE", ["v4/extensions-real-apis/FIXTURES.md"], ["manifest tests in each *-app"]),
    "DATA-02": ("DONE", ["v4/extensions-real-apis/STRESS.md", "v4/extensions-real-apis/evidence/"],
                ["cargo run --release -p concord-check-stream-stress -- --mib 256"]),
    "DATA-03": ("DONE", ["crates/concord-extras/tests/csv_fixtures.rs", "crates/concord-extras/tests/jsonl_framing.rs",
                         "crates/concord-pagination/tests/"], ["cargo test -p concord-extras --all-features",
                                                               "cargo test -p concord-pagination"]),
    "DATA-04": ("DONE", ["v4/extensions-real-apis/STRESS.md"], ["cargo test -p concord-check-stream-stress"]),
    "DATA-05": ("DONE", ["examples/testserver/", "examples/testserver/tests/lifecycle.rs",
                         "examples/riot/tests/fixture_lifecycle.rs"],
                ["cargo test -p concord-example-testserver"]),
    "DX-01": ("DONE", ["v4/extensions-real-apis/USAGE_AND_MAINTENANCE.md", "checks/maintenance-*/"],
              ["cargo test -p concord-check-maintenance-<provider>"]),
    "DX-02": ("DONE", ["v4/extensions-real-apis/DECISIONS.md", "examples/*-sdk/FRICTION.md",
                       "v4/extensions-real-apis/NEXT_REVIEW_4_4.md"],
              ["defects fixed at their layer (macro SOL-4, codecs, SDK adapters); repeated friction ranked in NEXT_REVIEW §4-5"]),
    "DX-03": ("DONE", ["checks/dx03-csv-runtime/", "checks/dx03-fanout-sink/"],
              ["cargo test -p concord-check-dx03-csv-runtime", "cargo test -p concord-check-dx03-fanout-sink"]),
    "VERIFY-01": ("DONE", ["scripts/verify.sh", "scripts/check-graph.py", "scripts/package-check.sh"],
                  ["scripts/verify.sh --package --bench"]),
    "VERIFY-02": ("PENDING", ["v4/extensions-real-apis/VERIFICATION.md"], []),
    "REVIEW-01": ("DONE", ["v4/extensions-real-apis/NEXT_REVIEW_4_4.md", "v4/extensions-real-apis/review/",
                           "v4/extensions-real-apis/findings.json"], []),
    "REVIEW-02": ("DONE", ["v4/extensions-real-apis/NEXT_REVIEW_4_4.md", "v4/extensions-real-apis/findings.json",
                           "v4/extensions-real-apis/review/astra-recheck/",
                           "v4/extensions-real-apis/review/sol-recheck/"], []),
    "REVIEW-03": ("DONE", ["v4/extensions-real-apis/NEXT_REVIEW_4_4.md"], []),
    "DELIVERY-01": ("PENDING", ["v4/extensions-real-apis/STATUS.md"], []),
}


def main():
    handoff = json.load(open(os.path.join(HERE, "requirements.handoff.json")))
    out = dict(handoff)
    reqs = []
    for req in handoff["requirements"]:
        status, paths, commands = E[req["id"]]
        r = dict(req)
        r["implementation_status"] = status
        r["implementation_evidence"] = [{"paths": paths, "commands": commands}]
        reqs.append(r)
    missing = set(E) - {r["id"] for r in handoff["requirements"]}
    assert not missing, missing
    out["requirements"] = reqs
    out["evidence_note"] = "Statuses per v4/extensions-real-apis/STATUS.md; source identities in VERIFICATION.md."
    with open(os.path.join(HERE, "requirements.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, indent=1)
        f.write("\n")
    counts = {}
    for r in reqs:
        counts[r["implementation_status"]] = counts.get(r["implementation_status"], 0) + 1
    print(counts)


if __name__ == "__main__":
    main()
