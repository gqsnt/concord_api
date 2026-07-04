use super::helpers::{analyze_ok, endpoint_by_name, endpoint_pagination};

#[test]
fn pagination_contracts_resolve_bindings_independently_of_query_keys() {
    let api = analyze_ok(
        r#"
        api! {
            client PageApi {
                base "https://example.com"
            }

            GET QueryList(start: u64 = 0, count: u64 = 20)
                path ["query"]
                query {
                    "from" = start,
                    "pageSize" = count,
                }
                paginate OffsetLimitPagination {
                    offset = start,
                    limit = count
                }
                -> Json<Vec<String>>

            GET HeaderList(page: u64 = 1, count: u64 = 20)
                path ["headers"]
                headers {
                    "X-Page" = page,
                    "X-Count" = count,
                }
                paginate PagedPagination {
                    page = page,
                    per_page = count
                }
                -> Json<Vec<String>>
        }
        "#,
    );

    let query = endpoint_pagination(endpoint_by_name(&api, "QueryList"));
    assert_eq!(query.assigns.len(), 2);
    assert_eq!(query.bindings.len(), 2);
    assert_eq!(query.bindings[0].controller_field.to_string(), "offset");
    assert_eq!(query.bindings[0].endpoint_rust_field.to_string(), "start");
    assert_eq!(query.bindings[1].controller_field.to_string(), "limit");
    assert_eq!(query.bindings[1].endpoint_rust_field.to_string(), "count");
    assert_ne!(query.bindings[0].endpoint_rust_field.to_string(), "from");
    assert_ne!(
        query.bindings[1].endpoint_rust_field.to_string(),
        "pageSize"
    );

    let header = endpoint_pagination(endpoint_by_name(&api, "HeaderList"));
    assert_eq!(header.assigns.len(), 2);
    assert_eq!(header.bindings.len(), 2);
    assert_eq!(header.bindings[0].controller_field.to_string(), "page");
    assert_eq!(header.bindings[0].endpoint_rust_field.to_string(), "page");
    assert_eq!(header.bindings[1].controller_field.to_string(), "per_page");
    assert_eq!(header.bindings[1].endpoint_rust_field.to_string(), "count");
}
