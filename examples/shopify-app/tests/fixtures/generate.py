"""Writes the Shopify SH-A / SH-B fixtures (schema-derived synthetic and fault-injection).

Run from this directory: `python3 -I generate.py .` It is deterministic; MANIFEST.json records the
sha256 of every file it writes, and `tests/manifest.rs` checks them.
"""
import json, os, sys

root = sys.argv[1]
A = os.path.join(root, "sh_a"); B = os.path.join(root, "sh_b")
os.makedirs(A, exist_ok=True); os.makedirs(B, exist_ok=True)

def cost(req, act, avail):
    return {"cost": {"requestedQueryCost": req, "actualQueryCost": act,
            "throttleStatus": {"maximumAvailable": 2000.0, "currentlyAvailable": avail, "restoreRate": 100.0}}}

def write(d, name, obj):
    with open(os.path.join(d, name), "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=2)
        f.write("\n")

def page(has_next, end):
    return {"hasNextPage": has_next, "endCursor": end}

def variant(vid, title, sku, price, cap, qty, pos, item, tracked):
    return {"id": f"gid://shopify/ProductVariant/{vid}", "title": title, "sku": sku, "price": price,
            "compareAtPrice": cap, "inventoryQuantity": qty, "position": pos,
            "inventoryItem": {"id": f"gid://shopify/InventoryItem/{item}", "tracked": tracked}}

def product(pid, title, handle, status, vendor, ptype, tracks, total, variants, vpage):
    return {"id": f"gid://shopify/Product/{pid}", "title": title, "handle": handle, "status": status,
            "vendor": vendor, "productType": ptype, "tracksInventory": tracks, "totalInventory": total,
            "updatedAt": "2026-10-20T09:30:00Z", "variants": {"nodes": variants, "pageInfo": vpage}}

def level(lid, item, sku, avail, onhand, cost_):
    return {"id": f"gid://shopify/InventoryLevel/{lid}?inventory_item_id={item}", "updatedAt": "2026-10-21T08:00:00Z",
            "quantities": [{"name": "available", "quantity": avail}, {"name": "on_hand", "quantity": onhand}],
            "item": {"id": f"gid://shopify/InventoryItem/{item}", "sku": sku, "tracked": True, "unitCost": cost_}}

# ---------------- SH-A, shop-one ----------------
write(A, "one_locations.json", {"data": {"locations": {"nodes": [
    {"id": "gid://shopify/Location/1001", "name": "Synthetic Warehouse", "isActive": True,
     "fulfillsOnlineOrders": True, "address": {"city": None, "countryCode": "FR"}}],
    "pageInfo": page(False, "bG9jLTE=")}}, "extensions": cost(4, 3, 1997)})

write(A, "one_products_p1.json", {"data": {"products": {"nodes": [
    product(101, "Synthetic Tee", "synthetic-tee", "ACTIVE", "Synthetic Vendor", "Shirts", True, 8,
            [variant(1101, "S", "SYN-TEE-S", "19.90", "24.90", 5, 1, 2101, True),
             variant(1102, "M", "SYN-TEE-M", "19.90", None, 3, 2, 2102, True)],
            page(True, "dmFyLXAxLTI=")),
    product(102, "Synthetic Cap", "synthetic-cap", "ACTIVE", "Synthetic Vendor", "Hats", True, 7,
            [variant(1201, "Default Title", None, "12.00", None, 7, 1, 2201, True)],
            page(False, "dmFyLXAyLTE="))],
    "pageInfo": page(True, "cHJvZC0y")}}, "extensions": cost(52, 14, 1983)})

write(A, "one_products_p2.json", {"data": {"products": {"nodes": [
    product(103, "Synthetic Mug « Ünïcødé »", "synthetic-mug", "DRAFT", "Synthetic Vendor", "Mugs", False, 0,
            [variant(1301, "Default Title", "SYN-MUG", "9.50", None, None, 1, 2301, False)],
            page(False, "dmFyLXAzLTE="))],
    "pageInfo": page(False, "cHJvZC0z")}}, "extensions": cost(52, 8, 1975)})

write(A, "one_variants_p101_a.json", {"data": {"product": {"id": "gid://shopify/Product/101", "variants": {
    "nodes": [variant(1103, "L", "SYN-TEE-L", "19.90", None, 0, 3, 2103, True)],
    "pageInfo": page(True, "dmFyLXAxLTM=")}}}, "extensions": cost(52, 4, 1971)})

write(A, "one_variants_p101_b.json", {"data": {"product": {"id": "gid://shopify/Product/101", "variants": {
    "nodes": [variant(1104, "XL", "SYN-TEE-S", "21.90", None, -1, 4, 2104, True)],
    "pageInfo": page(False, "dmFyLXAxLTQ=")}}}, "extensions": cost(52, 4, 1967)})

write(A, "one_collections.json", {"data": {"collections": {"nodes": [
    {"id": "gid://shopify/Collection/501", "title": "Synthetic Summer", "handle": "synthetic-summer",
     "updatedAt": "2026-10-01T00:00:00Z", "productsCount": {"count": 2, "precision": "EXACT"},
     "products": {"nodes": [{"id": "gid://shopify/Product/101"}], "pageInfo": page(True, "Y29sLTUwMS0x")}},
    {"id": "gid://shopify/Collection/502", "title": "Synthetic Empty", "handle": "synthetic-empty",
     "updatedAt": "2026-10-02T00:00:00Z", "productsCount": None,
     "products": {"nodes": [], "pageInfo": page(False, None)}}],
    "pageInfo": page(False, "Y29sLTUwMg==")}}, "extensions": cost(102, 9, 1958)})

write(A, "one_collection_products_501.json", {"data": {"collection": {"id": "gid://shopify/Collection/501",
    "products": {"nodes": [{"id": "gid://shopify/Product/103"}], "pageInfo": page(False, "Y29sLTUwMS0y")}}},
    "extensions": cost(102, 3, 1955)})

denied = lambda i: {"message": "Access denied for unitCost field. Required access: `read_product_costs`.",
                    "locations": [{"line": 1, "column": 236}],
                    "path": ["location", "inventoryLevels", "nodes", i, "item", "unitCost"],
                    "extensions": {"code": "ACCESS_DENIED", "documentation": "https://shopify.dev/api/usage/access-scopes"}}
write(A, "one_inventory_p1.json", {"data": {"location": {"id": "gid://shopify/Location/1001", "name": "Synthetic Warehouse",
    "inventoryLevels": {"nodes": [level(31, 2101, "SYN-TEE-S", 5, 5, None), level(32, 2102, "SYN-TEE-M", 2, 2, None)],
                        "pageInfo": page(True, "aW52LTI=")}}},
    "errors": [denied(0), denied(1)], "extensions": cost(104, 12, 1943)})

write(A, "one_inventory_p2.json", {"data": {"location": {"id": "gid://shopify/Location/1001", "name": "Synthetic Warehouse",
    "inventoryLevels": {"nodes": [
        level(34, 2104, "SYN-TEE-S", -1, 0, {"amount": "6.10", "currencyCode": "EUR"}),
        level(35, 2201, None, 7, 7, {"amount": "3.50", "currencyCode": "EUR"}),
        level(39, 2999, "SYN-ORPHAN", 4, 4, {"amount": "1.00", "currencyCode": "EUR"})],
        "pageInfo": page(False, "aW52LTU=")}}}, "extensions": cost(104, 14, 1929)})

# ---------------- SH-A, shop-two ----------------
write(A, "two_locations.json", {"data": {"locations": {"nodes": [
    {"id": "gid://shopify/Location/2001", "name": "Second Shop Store", "isActive": True,
     "fulfillsOnlineOrders": False, "address": {"city": "Gent", "countryCode": "BE"}}],
    "pageInfo": page(False, "dHdvLWxvYw==")}}, "extensions": cost(4, 3, 997.5)})
write(A, "two_products.json", {"data": {"products": {"nodes": [
    product(201, "Second Shop Lamp", "lamp", "ACTIVE", "O'Brien \"Lights\"", "Lamps", True, 1,
            [variant(3101, "Default Title", "TWO-LAMP", "49.00", None, 1, 1, 4101, True)], page(False, "dHdvLXZhcg=="))],
    "pageInfo": page(False, "dHdvLXByb2Q=")}}, "extensions": cost(52, 6, 991.5)})
write(A, "two_collections.json", {"data": {"collections": {"nodes": [
    {"id": "gid://shopify/Collection/601", "title": "Lights", "handle": "lights", "updatedAt": "2026-10-03T00:00:00Z",
     "productsCount": {"count": 1, "precision": "EXACT"},
     "products": {"nodes": [{"id": "gid://shopify/Product/201"}], "pageInfo": page(False, "dHdvLWNvbA==")}}],
    "pageInfo": page(False, "dHdvLWNvbHM=")}}, "extensions": cost(102, 5, 986.5)})
write(A, "two_inventory.json", {"data": {"location": {"id": "gid://shopify/Location/2001", "name": "Second Shop Store",
    "inventoryLevels": {"nodes": [level(41, 4101, "TWO-LAMP", 1, 1, {"amount": "20.00", "currencyCode": "EUR"})],
                        "pageInfo": page(False, "dHdvLWludg==")}}}, "extensions": cost(104, 5, 981.5)})

# ---------------- SH-A, faults ----------------
write(A, "fault_products_throttled.json", {"errors": [{"message": "Throttled",
    "extensions": {"code": "THROTTLED", "documentation": "https://shopify.dev/api/usage/limits#rate-limits"}}],
    "extensions": {"cost": {"requestedQueryCost": 752, "actualQueryCost": None,
                   "throttleStatus": {"maximumAvailable": 2000.0, "currentlyAvailable": 151, "restoreRate": 100.0}}}})
write(A, "fault_products_access_denied.json", {"data": None, "errors": [{
    "message": "Access denied for products field. Required access: `read_products` access scope.",
    "locations": [{"line": 1, "column": 101}], "path": ["products"],
    "extensions": {"code": "ACCESS_DENIED", "documentation": "https://shopify.dev/api/usage/access-scopes",
                   "requiredAccess": "`read_products` access scope."}}],
    "extensions": cost(52, 1, 1999)})
bad = product(101, "Synthetic Tee", "synthetic-tee", "ACTIVE", "Synthetic Vendor", "Shirts", True, 8,
              [variant(1101, "S", "SYN-TEE-S", "19.90", None, 5, 1, 2101, True)], page(False, None))
del bad["variants"]["nodes"][0]["sku"]
write(A, "fault_products_missing_field.json", {"data": {"products": {"nodes": [bad], "pageInfo": page(False, None)}},
    "extensions": cost(52, 4, 1990)})
write(A, "fault_products_next_without_cursor.json", {"data": {"products": {"nodes": [],
    "pageInfo": page(True, None)}}, "extensions": cost(52, 2, 1990)})

# ---------------- SH-B ----------------
STORAGE = "__STORAGE__"
SIG = "X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Credential=synthetic%40example.iam&X-Goog-Expires=604800&X-Goog-SignedHeaders=host&X-Goog-Signature=5yn7h371c51gn47ur3"
def op(status, oc, roc, url=None, partial=None, err=None, size=None, completed=None):
    return {"id": "gid://shopify/BulkOperation/9001", "status": status, "errorCode": err,
            "createdAt": "2026-10-22T10:00:00Z", "completedAt": completed, "objectCount": oc,
            "rootObjectCount": roc, "fileSize": size, "url": url, "partialDataUrl": partial, "type": "QUERY"}
write(B, "run_created.json", {"data": {"bulkOperationRunQuery": {"bulkOperation": op("CREATED", "0", "0"),
    "userErrors": []}}, "extensions": cost(10, 10, 1990)})
write(B, "run_in_progress.json", {"data": {"bulkOperationRunQuery": {"bulkOperation": None, "userErrors": [
    {"field": None, "message": "A bulk query operation for this app and shop is already in progress: gid://shopify/BulkOperation/9000.",
     "code": "OPERATION_IN_PROGRESS"}]}}, "extensions": cost(10, 10, 1990)})
write(B, "status_running.json", {"data": {"bulkOperation": op("RUNNING", "3", "1")}, "extensions": cost(1, 1, 1989)})
write(B, "status_completed.json", {"data": {"bulkOperation": op("COMPLETED", "7", "3",
    url=f"{STORAGE}/bulk/9001.jsonl?{SIG}", size="1234", completed="2026-10-22T10:01:07Z")}, "extensions": cost(1, 1, 1988)})
write(B, "status_completed_empty.json", {"data": {"bulkOperation": op("COMPLETED", "0", "0",
    completed="2026-10-22T10:00:09Z")}, "extensions": cost(1, 1, 1988)})
write(B, "status_failed_partial.json", {"data": {"bulkOperation": op("FAILED", "2", "1", err="TIMEOUT",
    partial=f"{STORAGE}/bulk/9001-partial.jsonl?{SIG}", completed="2026-10-22T10:30:00Z")}, "extensions": cost(1, 1, 1988)})
write(B, "status_failed_no_data.json", {"data": {"bulkOperation": op("FAILED", "0", "0", err="ACCESS_DENIED",
    completed="2026-10-22T10:00:03Z")}, "extensions": cost(1, 1, 1988)})
write(B, "status_throttled.json", {"errors": [{"message": "Throttled", "extensions": {"code": "THROTTLED"}}],
    "extensions": {"cost": {"requestedQueryCost": 1, "actualQueryCost": None,
                   "throttleStatus": {"maximumAvailable": 2000.0, "currentlyAvailable": 0.5, "restoreRate": 100.0}}}})

def pline(pid, title, handle, status="ACTIVE", vendor="Synthetic Vendor"):
    return json.dumps({"id": f"gid://shopify/Product/{pid}", "title": title, "handle": handle, "status": status,
                       "vendor": vendor, "updatedAt": "2026-10-20T09:30:00Z"}, ensure_ascii=False, separators=(",", ":"))
def vline(vid, pid, title, sku, price, qty, item, tracked):
    return json.dumps({"id": f"gid://shopify/ProductVariant/{vid}", "title": title, "sku": sku, "price": price,
                       "inventoryQuantity": qty, "inventoryItem": {"id": f"gid://shopify/InventoryItem/{item}", "tracked": tracked},
                       "__parentId": f"gid://shopify/Product/{pid}"}, ensure_ascii=False, separators=(",", ":"))
export = [pline(101, "Café crème — tasse 👕", "cafe-creme"),
          pline(102, "Synthetic Cap", "synthetic-cap", vendor="O'Brien \"Hats\""),
          vline(1101, 101, "S", "SYN-CC-S", "19.90", 5, 2101, True),
          pline(103, "日本語の商品", "nihongo", status="DRAFT"),
          vline(1102, 101, "M", None, "19.90", None, 2102, False),
          vline(1201, 102, "Default Title", "SYN-CAP", "12.00", 7, 2201, True),
          vline(1301, 103, "Default Title", "SYN-JP", "1000.00", 0, 2301, True)]
with open(os.path.join(B, "export.jsonl"), "w", encoding="utf-8", newline="") as f:
    f.write("\n".join(export) + "\n")
with open(os.path.join(B, "export_partial.jsonl"), "w", encoding="utf-8", newline="") as f:
    f.write("\n".join([export[0], export[2]]) + "\n")
malformed = [export[0], '{"id":"gid://shopify/Product/104","title":"cut', export[2],
             '{"id":"gid://shopify/Collection/501","title":"not selected"}', export[1]]
with open(os.path.join(B, "export_malformed.jsonl"), "w", encoding="utf-8", newline="") as f:
    f.write("\n".join(malformed) + "\n")
with open(os.path.join(B, "expired_signature.xml"), "w", encoding="utf-8", newline="") as f:
    f.write("<?xml version='1.0' encoding='UTF-8'?><Error><Code>AccessDenied</Code><Message>Access denied.</Message><Details>Request has expired: synthetic</Details></Error>")
