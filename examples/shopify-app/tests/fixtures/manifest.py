"""Writes MANIFEST.json (provenance, sources, sha256, wire assertions, outcome, test command per
fixture). Run from this directory after generate.py: `python3 -I manifest.py .`
"""
import hashlib, json, os, sys
root = sys.argv[1]
R = "2026-11-04"
U = {
 "admin": "https://shopify.dev/docs/api/admin-graphql/2026-10",
 "limits": "https://shopify.dev/docs/apps/build/apis/graphql-admin/rate-limits",
 "pagination": "https://shopify.dev/docs/api/usage/pagination-graphql",
 "bulk": "https://shopify.dev/docs/apps/build/apis/graphql-admin/bulk-operations/queries",
 "bulkop": "https://shopify.dev/docs/api/admin-graphql/2026-10/objects/BulkOperation",
 "product": "https://shopify.dev/docs/api/admin-graphql/2026-10/objects/Product",
 "variant": "https://shopify.dev/docs/api/admin-graphql/2026-10/objects/ProductVariant",
 "collection": "https://shopify.dev/docs/api/admin-graphql/2026-10/objects/Collection",
 "location": "https://shopify.dev/docs/api/admin-graphql/2026-10/objects/Location",
 "level": "https://shopify.dev/docs/api/admin-graphql/2026-10/objects/InventoryLevel",
 "item": "https://shopify.dev/docs/api/admin-graphql/2026-10/objects/InventoryItem",
 "usererr": "https://shopify.dev/docs/api/admin-graphql/2026-10/objects/BulkOperationUserError",
}
SA = "cargo test -p concord-example-shopify-app --test sh_a_catalog_inventory"
SB = "cargo test -p concord-example-shopify-app --test sh_b_bulk_export"
CAT = "examples/shopify-sdk/src/ops/catalog.rs"; INV = "examples/shopify-sdk/src/ops/inventory.rs"
BULK = "examples/shopify-sdk/src/ops/bulk.rs"; EXP = "examples/shopify-sdk/src/bulk_export.rs"
S = "schema_derived_synthetic"; F = "fault_injection"
entries = [
 ("sh_a/one_locations.json","SH-A","Locations",INV,[U["location"]],S,["POST /admin/api/2026-10/graphql.json on shop-one host","x-shopify-access-token = shop-one token","variables.after absent"],"one active location (address.city null); target location chosen",f"{SA} sh_a_reconciles"),
 ("sh_a/one_products_p1.json","SH-A","CatalogProducts",CAT,[U["product"],U["variant"],U["pagination"]],S,["variables.after absent (first page)","variables.variantsFirst present"],"page 1 of 2 (endCursor cHJvZC0y); product 101 has a nested variant page (endCursor dmFyLXAxLTI=); variant 1201 sku null",f"{SA} sh_a_reconciles"),
 ("sh_a/one_products_p2.json","SH-A","CatalogProducts",CAT,[U["product"],U["variant"],U["pagination"]],S,["variables.after = cHJvZC0y (previous endCursor)"],"last page; inventoryQuantity null, untracked item",f"{SA} sh_a_reconciles"),
 ("sh_a/one_variants_p101_a.json","SH-A","ProductVariants",CAT,[U["product"],U["variant"]],S,["variables.id = gid://shopify/Product/101","variables.after = dmFyLXAxLTI= (nested endCursor)"],"nested page 1 of 2",f"{SA} sh_a_reconciles"),
 ("sh_a/one_variants_p101_b.json","SH-A","ProductVariants",CAT,[U["product"],U["variant"]],S,["variables.after = dmFyLXAxLTM="],"nested last page; duplicate SKU, negative quantity",f"{SA} sh_a_reconciles"),
 ("sh_a/one_collections.json","SH-A","CatalogCollections",CAT,[U["collection"]],S,["variables.after absent"],"collection 501 has a nested member page; productsCount null on 502",f"{SA} sh_a_reconciles"),
 ("sh_a/one_collection_products_501.json","SH-A","CollectionProducts",CAT,[U["collection"]],S,["variables.after = Y29sLTUwMS0x"],"nested member page",f"{SA} sh_a_reconciles"),
 ("sh_a/one_inventory_p1.json","SH-A","LocationInventory",INV,[U["level"],U["item"],U["admin"]],S,["variables.id = gid://shopify/Location/1001","variables.names = [available, on_hand]"],"partial data: unitCost null with two per-field ACCESS_DENIED errors at HTTP 200; accepted or rejected per policy",f"{SA} sh_a_reconciles / sh_a_rejected_partial_data"),
 ("sh_a/one_inventory_p2.json","SH-A","LocationInventory",INV,[U["level"],U["item"]],S,["variables.after = aW52LTI="],"last page; a level whose item is in no variant",f"{SA} sh_a_reconciles"),
 ("sh_a/two_locations.json","SH-A","Locations",INV,[U["location"]],S,["shop-two host, shop-two token only"],"second shop, disjoint ids",f"{SA} sh_a_two_shops"),
 ("sh_a/two_products.json","SH-A","CatalogProducts",CAT,[U["product"],U["variant"]],S,["variables.query = JSON string of the search syntax with escaped quotes and non-ASCII"],"one product",f"{SA} sh_a_two_shops"),
 ("sh_a/two_collections.json","SH-A","CatalogCollections",CAT,[U["collection"]],S,["shop-two token"],"one collection",f"{SA} sh_a_two_shops"),
 ("sh_a/two_inventory.json","SH-A","LocationInventory",INV,[U["level"]],S,["variables.id = gid://shopify/Location/2001"],"no discrepancy; status Complete",f"{SA} sh_a_two_shops"),
 ("sh_a/fault_products_throttled.json","SH-A","CatalogProducts",CAT,[U["admin"],U["limits"]],F,["requested once, never retried"],"THROTTLED at HTTP 200 without data; actualQueryCost null (assumption); run stops, resume = cHJvZC0y",f"{SA} sh_a_throttled"),
 ("sh_a/fault_products_access_denied.json","SH-A","CatalogProducts",CAT,[U["admin"]],F,[],"data null with ACCESS_DENIED: traversal fails (no connection)",f"{SA} sh_a_http_200_errors"),
 ("sh_a/fault_products_missing_field.json","SH-A","CatalogProducts",CAT,[U["variant"]],F,[],"selected nullable field sku absent: Concord Decode error, not None",f"{SA} sh_a_http_200_errors"),
 ("sh_a/fault_products_next_without_cursor.json","SH-A","CatalogProducts",CAT,[U["pagination"]],F,[],"hasNextPage true with endCursor null: protocol failure, no guessed continuation",f"{SA} sh_a_http_200_errors"),
 ("sh_b/run_created.json","SH-B","BulkRunQuery",BULK,[U["bulk"],U["bulkop"]],S,["variables.query = CatalogExport::document() (filter as GraphQL string literal inside a JSON string)","shop-one token"],"job CREATED",f"{SB} sh_b_completed"),
 ("sh_b/run_in_progress.json","SH-B","BulkRunQuery",BULK,[U["usererr"]],S,["no status poll follows"],"userErrors OPERATION_IN_PROGRESS: Rejected, no job",f"{SB} sh_b_a_rejected"),
 ("sh_b/status_running.json","SH-B","BulkOperationStatus",BULK,[U["bulkop"]],S,["variables.id = gid://shopify/BulkOperation/9001"],"RUNNING, objectCount \"3\" (UnsignedInt64 string)",f"{SB} sh_b_completed / sh_b_polling"),
 ("sh_b/status_completed.json","SH-B","BulkOperationStatus",BULK,[U["bulkop"],U["bulk"]],S,["__STORAGE__ replaced by the storage server origin"],"COMPLETED with signed url on another host",f"{SB} sh_b_completed"),
 ("sh_b/status_completed_empty.json","SH-B","BulkOperationStatus",BULK,[U["bulk"]],S,["no download request"],"COMPLETED with url null: empty successful export",f"{SB} sh_b_empty"),
 ("sh_b/status_failed_partial.json","SH-B","BulkOperationStatus",BULK,[U["bulkop"]],S,["download from partialDataUrl"],"FAILED TIMEOUT with partialDataUrl: Partial outcome",f"{SB} sh_b_failed_job"),
 ("sh_b/status_failed_no_data.json","SH-B","BulkOperationStatus",BULK,[U["bulkop"]],S,["no download request"],"FAILED ACCESS_DENIED without partial data",f"{SB} sh_b_failed_job"),
 ("sh_b/status_throttled.json","SH-B","BulkOperationStatus",BULK,[U["admin"]],F,[],"THROTTLED poll: RequestFailed(Status), bucket recorded",f"{SB} sh_b_polling"),
 ("sh_b/export.jsonl","SH-B","bulk result download",EXP,[U["bulk"]],S,["GET signed URL verbatim on storage host","no x-shopify-access-token"],"3 products + 4 variants with __parentId, children after parents but interleaved; split at chunk sizes 1,2,3,5,7,64,all (inside multi-byte UTF-8)",f"{SB} sh_b_completed"),
 ("sh_b/export_partial.jsonl","SH-B","bulk partial result download",EXP,[U["bulk"]],S,[],"1 product + 1 variant",f"{SB} sh_b_failed_job"),
 ("sh_b/export_malformed.jsonl","SH-B","bulk result download",EXP,[U["bulk"]],F,[],"truncated JSON line (Format) and a line of an unselected type (Decode): Stop / Skip{1} / Skip{2}",f"{SB} sh_b_malformed"),
 ("sh_b/expired_signature.xml","SH-B","bulk result download",EXP,[U["bulk"]],F,["served with status 403"],"refused before the body is read; response kept unread (expired-signature status not documented by Shopify; 403 chosen per contract)",f"{SB} sh_b_expired"),
]
out = []
for (f, wf, op, src, urls, prov, wire, outcome, test) in entries:
    data = open(os.path.join(root, f), "rb").read()
    out.append({"file": f, "workflow": wf, "operation": op, "sdk_source": src,
                "provider_family": "Shopify GraphQL Admin API", "provider_version": "2026-10",
                "schema": {"digest": None, "documentation": urls, "retrieved": R},
                "authoritative_url": urls[0], "provenance": prov,
                "sha256": hashlib.sha256(data).hexdigest(), "wire_assertions": wire,
                "expected_outcome": outcome, "test_command": test})
manifest = {
  "provider": "Shopify GraphQL Admin API", "version": "2026-10",
  "schema_note": "No machine-readable schema digest was pinned (the public reference pages were used); each entry names the versioned 2026-10 reference pages checked, retrieved 2026-11-04. No fixture is a capture of a real shop: ids, tokens, names and URLs are synthetic.",
  "generator": "tests/fixtures/generate.py (python3 -I generate.py .), deterministic",
  "placeholders": {"__STORAGE__": "replaced at test time by http://storage.example.test:<port>, the storage loopback server (the signed-URL host is resolved by name with reqwest::ClientBuilder::resolve)"},
  "served_headers": "GraphQL fixtures are served with status 200, content-type application/json; charset=utf-8 and x-shopify-api-version: 2026-10",
  "inline_cases": [
    {"test": f"{SB} sh_b_an_oversized_line", "provenance": F, "description": "a 4 KiB product line built in the test after two fixture lines; limit 1024 bytes"},
    {"test": f"{SB} sh_b_consumer_cancellation", "provenance": F, "description": "1,000,000 generated product lines, never materialized; the sink stops after 5"},
    {"test": f"{SB} sh_b_a_transport_error", "provenance": F, "description": "export.jsonl cut 20 bytes into its third line, then the connection is dropped"},
    {"test": f"{SB} sh_b_a_large_generated_export", "provenance": S, "description": "40,000 generated product/variant lines (7,915,560 bytes), 16 KiB chunks, 1 ms per chunk"}
  ],
  "fixtures": out}
with open(os.path.join(root, "MANIFEST.json"), "w", encoding="utf-8") as fh:
    json.dump(manifest, fh, ensure_ascii=False, indent=2); fh.write("\n")
print(len(out))
