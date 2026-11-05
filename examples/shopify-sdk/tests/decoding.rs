//! Envelope, model and adapter decoding without a network: the pinned schema's null/absent
//! rules, partial data, error extensions, scalars, connection continuations and bulk JSONL lines.

use concord::Decode;
use concord::Json;
use concord::bytes::Bytes;
use concord::jsonl::RecordErrors;
use concord::stream::{Chunks, ReadErrorKind};
use concord_example_shopify_sdk::bulk_export::{self, BulkRecord, CatalogExport, CatalogLine};
use concord_example_shopify_sdk::connection::{DataShape, Protocol};
use concord_example_shopify_sdk::graphql::{
    Data, ErrorCode, GraphqlResponse, PathSegment, string_literal,
};
use concord_example_shopify_sdk::ops::bulk::{BulkOperationData, BulkOperationStatus};
use concord_example_shopify_sdk::ops::catalog::ProductsData;
use concord_example_shopify_sdk::ops::inventory::{LocationInventoryData, QuantityName};
use concord_example_shopify_sdk::scalars::{InventoryLevelId, ProductId};
use concord_example_shopify_sdk::{Cursor, PageError, RequestError, SearchQuery, connection_page};
use serde_json::json;

fn decode<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> Result<T, concord::CodecError> {
    <Json as Decode<T>>::decode_body((), Bytes::from(value.to_string()))
}

fn variant(id: u32, sku: serde_json::Value) -> serde_json::Value {
    json!({"id": format!("gid://shopify/ProductVariant/{id}"), "title": "Default Title",
           "sku": sku, "price": "19.90", "compareAtPrice": null, "inventoryQuantity": 4,
           "position": 1, "inventoryItem": {"id": format!("gid://shopify/InventoryItem/{id}"), "tracked": true}})
}

fn products(nodes: serde_json::Value, has_next: bool, end: Option<&str>) -> serde_json::Value {
    json!({"products": {"nodes": nodes, "pageInfo": {"hasNextPage": has_next, "endCursor": end}}})
}

fn product(id: u32, variants: serde_json::Value) -> serde_json::Value {
    json!({"id": format!("gid://shopify/Product/{id}"), "title": "Tee", "handle": "tee",
           "status": "ACTIVE", "vendor": "Synthetic", "productType": "", "tracksInventory": true,
           "totalInventory": 4, "updatedAt": "2026-10-01T00:00:00Z",
           "variants": {"nodes": variants, "pageInfo": {"hasNextPage": false, "endCursor": null}}})
}

#[test]
fn nullable_fields_accept_null_and_refuse_absence() {
    let ok: GraphqlResponse<ProductsData> = decode(json!({
        "data": products(json!([product(1, json!([variant(1, json!(null))]))]), false, None)
    }))
    .unwrap();
    let data = ok.data.present().unwrap();
    let v = &data.products.nodes[0].variants.nodes[0];
    assert_eq!(v.sku, None);
    assert_eq!(v.compare_at_price, None);
    assert_eq!(v.price.as_str(), "19.90");

    // A selected nullable field that is missing is not silently `None`.
    let mut missing = variant(1, json!("SKU-1"));
    missing.as_object_mut().unwrap().remove("sku");
    let error = decode::<GraphqlResponse<ProductsData>>(json!({
        "data": products(json!([product(1, json!([missing]))]), false, None)
    }))
    .unwrap_err();
    assert_eq!(error.label(), "the JSON does not match the expected type");

    // A non-null field that is null is a decode error, too.
    let mut null_title = product(1, json!([]));
    null_title["title"] = json!(null);
    assert!(
        decode::<GraphqlResponse<ProductsData>>(
            json!({"data": products(json!([null_title]), false, None)})
        )
        .is_err()
    );
}

#[test]
fn data_absent_null_and_partial_are_distinct() {
    let absent: GraphqlResponse<ProductsData> = decode(
        json!({"errors": [{"message": "Parse error", "locations": [{"line": 1, "column": 9}]}]}),
    )
    .unwrap();
    assert!(matches!(absent.data, Data::Absent));
    assert_eq!(absent.errors[0].locations[0].column, 9);
    assert_eq!(absent.errors[0].code(), None);

    let null: GraphqlResponse<ProductsData> = decode(json!({"data": null, "errors": [
        {"message": "Internal error", "extensions": {"code": "INTERNAL_SERVER_ERROR", "requestId": "r-1"}}]}))
    .unwrap();
    assert!(matches!(null.data, Data::Null));
    assert_eq!(null.errors[0].code(), Some(&ErrorCode::InternalServerError));

    // Partial: a per-field ACCESS_DENIED nulls `unitCost` of one level; the rest is data.
    let partial: GraphqlResponse<LocationInventoryData> = decode(json!({
        "data": {"location": {"id": "gid://shopify/Location/1", "name": "Main", "inventoryLevels": {
            "nodes": [{"id": "gid://shopify/InventoryLevel/1?inventory_item_id=7",
                       "updatedAt": "2026-10-01T00:00:00Z",
                       "quantities": [{"name": "available", "quantity": 3}, {"name": "on_hand", "quantity": 5}],
                       "item": {"id": "gid://shopify/InventoryItem/7", "sku": "SKU-7", "tracked": true, "unitCost": null}}],
            "pageInfo": {"hasNextPage": false, "endCursor": null}}}},
        "errors": [{"message": "Access denied for unitCost field.",
                    "path": ["location", "inventoryLevels", "nodes", 0, "item", "unitCost"],
                    "extensions": {"code": "ACCESS_DENIED"}}]
    }))
    .unwrap();
    assert!(partial.is_partial());
    let error = &partial.errors[0];
    assert!(error.path_starts_with(&["location", "inventoryLevels", "nodes", "item", "unitCost"]));
    assert_eq!(error.path.as_ref().unwrap()[3], PathSegment::Index(0));
    assert_eq!(
        error.to_string(),
        "GraphQL error ACCESS_DENIED at location.inventoryLevels.nodes.0.item.unitCost"
    );
    let level = &partial
        .data
        .as_present()
        .unwrap()
        .location
        .as_ref()
        .unwrap()
        .inventory_levels
        .nodes[0];
    assert_eq!(level.item.unit_cost, None);
    assert_eq!(level.quantity(QuantityName::OnHand), Some(5));
    let id: &InventoryLevelId = &level.id;
    assert_eq!(id.tail(), "1?inventory_item_id=7");
}

#[test]
fn error_extensions_stay_dynamic_where_shopify_varies_them() {
    // The documented MAX_COST_EXCEEDED example: `extensions.cost` of an *error* is a number.
    let answer: GraphqlResponse<ProductsData> = decode(json!({"errors": [{
        "message": "Query cost is 2003, which exceeds the single query max cost limit (1000).",
        "extensions": {"code": "MAX_COST_EXCEEDED", "cost": 2003, "maxCost": 1000,
                       "documentation": "https://shopify.dev/api/usage/limits#rate-limits"}}]}))
    .unwrap();
    let ext = answer.errors[0].extensions.as_ref().unwrap();
    assert_eq!(ext.code, Some(ErrorCode::MaxCostExceeded));
    assert_eq!(ext.other["cost"], json!(2003));
    assert!(answer.cost().is_none());

    let other: GraphqlResponse<ProductsData> =
        decode(json!({"errors": [{"message": "m", "extensions": {"code": "SOMETHING_NEW"}}]}))
            .unwrap();
    assert_eq!(
        other.errors[0].code(),
        Some(&ErrorCode::Other("SOMETHING_NEW".into()))
    );
    assert!(!format!("{:?}", other.errors[0]).contains("\"m\""));
}

#[test]
fn bulk_operation_scalars_follow_the_schema() {
    let answer: GraphqlResponse<BulkOperationData> = decode(json!({"data": {"bulkOperation": {
        "id": "gid://shopify/BulkOperation/720", "status": "COMPLETED", "errorCode": null,
        "createdAt": "2026-10-01T00:00:00Z", "completedAt": "2026-10-01T00:01:00Z",
        "objectCount": "57", "rootObjectCount": "12", "fileSize": "358",
        "url": "https://storage.googleapis.com/shopify/synthetic.jsonl?X-Goog-Signature=abc",
        "partialDataUrl": null, "type": "QUERY"}}}))
    .unwrap();
    let op = answer.data.present().unwrap().bulk_operation.unwrap();
    assert_eq!(op.status, BulkOperationStatus::Completed);
    assert!(op.status.is_terminal());
    assert_eq!(op.object_count.0, 57);
    assert_eq!(op.file_size.unwrap().0, 358);
    let shown = format!("{op:?}");
    assert!(
        shown.contains("storage.googleapis.com") && !shown.contains("X-Goog-Signature"),
        "{shown}"
    );

    // A malformed UnsignedInt64 or a signed URL that is not absolute is a decode error.
    let mut bad = json!({"data": {"bulkOperation": {
        "id": "gid://shopify/BulkOperation/720", "status": "RUNNING", "errorCode": null,
        "createdAt": "2026-10-01T00:00:00Z", "completedAt": null, "objectCount": "-1",
        "rootObjectCount": "0", "fileSize": null, "url": null, "partialDataUrl": null, "type": "QUERY"}}});
    assert!(decode::<GraphqlResponse<BulkOperationData>>(bad.clone()).is_err());
    bad["data"]["bulkOperation"]["objectCount"] = json!("3");
    bad["data"]["bulkOperation"]["url"] = json!("relative/path");
    assert!(decode::<GraphqlResponse<BulkOperationData>>(bad).is_err());
}

#[test]
fn global_ids_are_checked_against_their_type() {
    assert!(ProductId::parse("gid://shopify/Product/1").is_ok());
    assert!(ProductId::parse("gid://shopify/ProductVariant/1").is_err());
    assert!(ProductId::parse("gid://shopify/Product/").is_err());
    let wrong = decode::<GraphqlResponse<ProductsData>>(json!({"data": products(
        json!([{"id": "gid://shopify/Collection/1"}]), false, None)}));
    assert!(wrong.is_err());
}

async fn page_of(
    value: serde_json::Value,
    requested: Option<&Cursor>,
) -> Result<
    concord_pagination::Page<
        concord_example_shopify_sdk::ops::catalog::ProductNode,
        Cursor,
        concord_example_shopify_sdk::PageMeta,
    >,
    PageError,
> {
    let answer: GraphqlResponse<ProductsData> = decode(value).unwrap();
    connection_page(
        async { Ok::<_, RequestError>(answer) },
        requested,
        |d: ProductsData| Some(d.products),
    )
    .await
}

#[tokio::test]
async fn the_adapter_selects_the_dependent_cursor_and_refuses_inconsistent_page_info() {
    let page = page_of(
        json!({"data": products(json!([product(1, json!([]))]), true, Some("c1"))}),
        None,
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.next.as_ref().map(Cursor::expose), Some("c1"));
    assert!(page.meta.errors.is_empty());

    // An empty page that says more follows is followed (not the end).
    let empty = page_of(
        json!({"data": products(json!([]), true, Some("c2"))}),
        Some(&Cursor::new("c1")),
    )
    .await
    .unwrap();
    assert!(empty.items.is_empty() && empty.next.is_some());

    let last = page_of(json!({"data": products(json!([]), false, None)}), None)
        .await
        .unwrap();
    assert!(last.next.is_none());

    let error = page_of(json!({"data": products(json!([]), true, None)}), None)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        PageError::Protocol {
            violation: Protocol::NextWithoutCursor,
            ..
        }
    ));
    let error = page_of(
        json!({"data": products(json!([]), true, Some("c1"))}),
        Some(&Cursor::new("c1")),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        PageError::Protocol {
            violation: Protocol::CursorDidNotAdvance,
            ..
        }
    ));
    assert!(!format!("{error} {error:?}").contains("c1"));

    let throttled = page_of(json!({"errors": [{"message": "Throttled", "extensions": {"code": "THROTTLED"}}],
        "extensions": {"cost": {"requestedQueryCost": 10, "actualQueryCost": null,
        "throttleStatus": {"maximumAvailable": 1000.0, "currentlyAvailable": 2, "restoreRate": 50.0}}}}), None)
        .await
        .unwrap_err();
    assert!(throttled.is_throttled());
    assert!(matches!(
        throttled,
        PageError::NoConnection {
            data: DataShape::Absent,
            ..
        }
    ));
    assert_eq!(
        throttled
            .cost()
            .unwrap()
            .throttle_status
            .currently_available,
        2.0
    );
}

#[test]
fn bulk_documents_escape_the_filter_as_a_graphql_string() {
    assert_eq!(
        string_literal("a\"b\\c\nd\u{1}é"),
        "\"a\\\"b\\\\c\\nd\\u0001é\""
    );
    let export = CatalogExport::products()
        .filter(SearchQuery::new().term("vendor", "O'Brien \"Q\""))
        .with_variants(true);
    let document = export.document();
    assert_eq!(
        document,
        r#"{ products(query: "vendor:\"O'Brien \\\"Q\\\"\"") { edges { node { id title handle status vendor updatedAt variants { edges { node { id title sku price inventoryQuantity inventoryItem { id tracked } } } } } } } }"#
    );
    assert_eq!(
        CatalogExport::products().document(),
        "{ products { edges { node { id title handle status vendor updatedAt } } } }"
    );
}

#[tokio::test]
async fn bulk_lines_are_typed_with_their_parent_and_split_anywhere() {
    let body = concat!(
        r#"{"id":"gid://shopify/Product/1","title":"Ünïcødé tee","handle":"tee","status":"ACTIVE","vendor":"V","updatedAt":"2026-10-01T00:00:00Z"}"#, "\n",
        r#"{"id":"gid://shopify/ProductVariant/11","title":"S","sku":null,"price":"10.00","inventoryQuantity":null,"inventoryItem":{"id":"gid://shopify/InventoryItem/11","tracked":false},"__parentId":"gid://shopify/Product/1"}"#, "\n",
    )
    .as_bytes();
    // Every split point, including inside the multi-byte characters.
    for split in 0..body.len() {
        let reader = bulk_export::result_reader(1024, RecordErrors::Stop);
        let chunks = Chunks::new([body[..split].to_vec(), body[split..].to_vec()]);
        let mut records = reader.read::<BulkRecord, _>(chunks);
        let BulkRecord::Product(p) = records.next().await.unwrap().unwrap() else {
            panic!()
        };
        assert_eq!(p.title, "Ünïcødé tee");
        let BulkRecord::Variant(v) = records.next().await.unwrap().unwrap() else {
            panic!()
        };
        assert_eq!(v.product_id, p.id);
        assert_eq!((v.sku, v.inventory_quantity), (None, None));
        assert!(records.next().await.is_none());
    }

    // A variant without its parent, or of an unselected type, is a Decode error of that line.
    let bad = concat!(
        r#"{"id":"gid://shopify/ProductVariant/11","title":"S","sku":null,"price":"1.00","inventoryQuantity":1,"inventoryItem":{"id":"gid://shopify/InventoryItem/11","tracked":true}}"#,
        "\n",
        r#"{"id":"gid://shopify/Collection/5"}"#,
        "\n",
        r#"{"id":"gid://shopify/Product/2","title":"t","handle":"h","status":"DRAFT","vendor":"v","updatedAt":"2026-10-01T00:00:00Z"}"#,
        "\n",
    );
    let reader = bulk_export::result_reader(1024, RecordErrors::Yield);
    let mut records = reader.read::<BulkRecord, _>(Chunks::new([bad.as_bytes().to_vec()]));
    assert_eq!(
        records.next().await.unwrap().unwrap_err().kind(),
        ReadErrorKind::Decode
    );
    assert_eq!(
        records.next().await.unwrap().unwrap_err().kind(),
        ReadErrorKind::Decode
    );
    assert!(matches!(
        records.next().await.unwrap().unwrap(),
        BulkRecord::Product(_)
    ));
}

#[tokio::test]
async fn metafield_lines_are_children_of_their_product_and_only_catalog_line_reads_them() {
    let export = CatalogExport::products()
        .with_variants(true)
        .with_metafields(Some("mm-google-shopping \"feed\""));
    assert!(export.includes_metafields());
    assert_eq!(
        export.document(),
        r#"{ products { edges { node { id title handle status vendor updatedAt variants { edges { node { id title sku price inventoryQuantity inventoryItem { id tracked } } } } metafields(namespace: "mm-google-shopping \"feed\"") { edges { node { id namespace key value type } } } } } } }"#
    );
    assert_eq!(
        CatalogExport::products().with_metafields(None).document(),
        "{ products { edges { node { id title handle status vendor updatedAt metafields { edges { node { id namespace key value type } } } } } } }"
    );

    // Children of two products, interleaved: each line names its parent.
    let body = concat!(
        r#"{"id":"gid://shopify/Product/1","title":"Tee","handle":"tee","status":"ACTIVE","vendor":"V","updatedAt":"2026-10-01T00:00:00Z"}"#, "\n",
        r#"{"id":"gid://shopify/Product/2","title":"Cap","handle":"cap","status":"ACTIVE","vendor":"V","updatedAt":"2026-10-01T00:00:00Z"}"#, "\n",
        r#"{"id":"gid://shopify/ProductVariant/11","title":"S","sku":null,"price":"10.00","inventoryQuantity":null,"inventoryItem":{"id":"gid://shopify/InventoryItem/11","tracked":false},"__parentId":"gid://shopify/Product/1"}"#, "\n",
        r#"{"id":"gid://shopify/Metafield/21","namespace":"mm-google-shopping","key":"gender","value":"unisex","type":"single_line_text_field","__parentId":"gid://shopify/Product/1"}"#, "\n",
        r#"{"id":"gid://shopify/Metafield/22","namespace":"custom","key":"care","value":"[\"wash cold\",\"dry flat\"]","type":"list.single_line_text_field","__parentId":"gid://shopify/Product/2"}"#, "\n",
    )
    .as_bytes();
    for split in [1, 7, 150, 300, body.len() - 1] {
        let reader = bulk_export::result_reader(1024, RecordErrors::Stop);
        let chunks = Chunks::new([body[..split].to_vec(), body[split..].to_vec()]);
        let mut lines = reader.read::<CatalogLine, _>(chunks);
        let mut kinds = Vec::new();
        while let Some(line) = lines.next().await {
            match line.unwrap() {
                CatalogLine::Record(BulkRecord::Product(p)) => {
                    kinds.push(format!("P{}", p.id.tail()))
                }
                CatalogLine::Record(BulkRecord::Variant(v)) => {
                    kinds.push(format!("V{}<{}", v.id.tail(), v.product_id.tail()));
                }
                CatalogLine::Metafield(m) => {
                    kinds.push(format!(
                        "M{}<{} {}.{}",
                        m.id.tail(),
                        m.product_id.tail(),
                        m.namespace,
                        m.key
                    ));
                    if m.key == "care" {
                        assert_eq!(m.value, r#"["wash cold","dry flat"]"#);
                        assert_eq!(m.value_type, "list.single_line_text_field");
                    }
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(
            kinds,
            [
                "P1",
                "P2",
                "V11<1",
                "M21<1 mm-google-shopping.gender",
                "M22<2 custom.care"
            ]
        );
    }

    // `BulkRecord` (the reader of selections without metafields) refuses a metafield line as a
    // line of an unselected type; a metafield without its product parent is refused by both.
    let bad = concat!(
        r#"{"id":"gid://shopify/Metafield/21","namespace":"n","key":"k","value":"v","type":"t","__parentId":"gid://shopify/Product/1"}"#,
        "\n",
        r#"{"id":"gid://shopify/Metafield/23","namespace":"n","key":"k","value":"v","type":"t","__parentId":"gid://shopify/ProductVariant/11"}"#,
        "\n",
        r#"{"id":"gid://shopify/Metafield/24","namespace":"n","key":"k","type":"t","__parentId":"gid://shopify/Product/1"}"#,
        "\n",
    );
    let reader = bulk_export::result_reader(1024, RecordErrors::Yield);
    let mut old = reader.read::<BulkRecord, _>(Chunks::new([bad.as_bytes().to_vec()]));
    let error = old.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Decode);
    assert_eq!(
        error.source_unredacted().unwrap().to_string(),
        "a line of an object type the selection does not contain"
    );
    let mut new = reader.read::<CatalogLine, _>(Chunks::new([bad.as_bytes().to_vec()]));
    assert!(matches!(
        new.next().await.unwrap().unwrap(),
        CatalogLine::Metafield(_)
    ));
    for expected in [
        "metafield line whose __parentId is not a product",
        "metafield line without value",
    ] {
        let error = new.next().await.unwrap().unwrap_err();
        assert_eq!(error.kind(), ReadErrorKind::Decode);
        assert_eq!(error.source_unredacted().unwrap().to_string(), expected);
    }
}
