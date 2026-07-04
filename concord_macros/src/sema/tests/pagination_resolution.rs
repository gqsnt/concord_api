use super::helpers::{analyze_ok, endpoint_by_name, endpoint_pagination, single_endpoint};
use crate::sema::PaginationValueKind;

#[test]
fn pagination_resolution_lowers_multiple_endpoint_pagination_shapes() {
    let api = analyze_ok(
        r#"
        api! {
            client PageApi {
                base "https://example.com"
            }

            GET Offset(start: u64 = 0, count: u64 = 20)
                path ["offset"]
                paginate OffsetLimitPagination {
                    offset = start,
                    limit = count
                }
                -> Json<Vec<String>>

            GET Cursor(cursor?: String, count: u64 = 20)
                path ["cursor"]
                query {
                    cursor
                    count
                }
                paginate CursorPagination<String> {
                    cursor = cursor,
                    per_page = count
                }
                -> Json<Vec<String>>

            GET Paged(page: u64 = 1, count: u64 = 20)
                path ["paged"]
                query {
                    page
                    count
                }
                paginate PagedPagination {
                    page = page,
                    per_page = count
                }
                -> Json<Vec<String>>

            GET Custom(page: u64 = 1)
                path ["custom"]
                paginate HeaderCursorPagination {
                    page = page
                }
                -> Json<Vec<String>>
        }
        "#,
    );

    let offset = endpoint_pagination(endpoint_by_name(&api, "Offset"));
    let controller_ty = &offset.controller_ty;
    assert_eq!(
        quote::quote!(#controller_ty).to_string(),
        "OffsetLimitPagination"
    );
    assert_eq!(offset.assigns.len(), 2);
    assert_eq!(offset.bindings.len(), 2);
    assert_eq!(offset.assigns[0].field.to_string(), "offset");
    assert!(matches!(
        &offset.assigns[0].value,
        PaginationValueKind::EpField(field) if field.to_string() == "start"
    ));
    assert_eq!(offset.assigns[1].field.to_string(), "limit");
    assert!(matches!(
        &offset.assigns[1].value,
        PaginationValueKind::EpField(field) if field.to_string() == "count"
    ));

    let cursor = endpoint_pagination(endpoint_by_name(&api, "Cursor"));
    let controller_ty = &cursor.controller_ty;
    assert_eq!(
        quote::quote!(#controller_ty).to_string(),
        "CursorPagination < String >"
    );
    assert_eq!(cursor.assigns.len(), 2);
    assert_eq!(cursor.bindings.len(), 2);

    let paged = endpoint_pagination(endpoint_by_name(&api, "Paged"));
    let controller_ty = &paged.controller_ty;
    assert_eq!(quote::quote!(#controller_ty).to_string(), "PagedPagination");
    assert_eq!(paged.assigns.len(), 2);
    assert_eq!(paged.bindings.len(), 2);

    let custom = endpoint_pagination(endpoint_by_name(&api, "Custom"));
    let controller_ty = &custom.controller_ty;
    assert_eq!(
        quote::quote!(#controller_ty).to_string(),
        "HeaderCursorPagination"
    );
    assert_eq!(custom.assigns.len(), 1);
    assert_eq!(custom.bindings.len(), 1);
}

#[test]
fn pagination_resolution_lowers_cursor_control_assignments() {
    let api = analyze_ok(
        r#"
        api! {
            client PageApi {
                base "https://example.com"
            }

            GET List(cursor?: String, count: u64 = 20)
                paginate CursorPagination<String> {
                    cursor = cursor,
                    per_page = count,
                    send_cursor_on_first = true,
                    stop_when_cursor_missing = false
                }
                -> Json<Vec<String>>
        }
        "#,
    );

    let pagination = endpoint_pagination(single_endpoint(&api));
    let controller_ty = &pagination.controller_ty;
    assert_eq!(
        quote::quote!(#controller_ty).to_string(),
        "CursorPagination < String >"
    );
    assert_eq!(pagination.assigns.len(), 4);
    assert_eq!(pagination.bindings.len(), 2);
    assert_eq!(pagination.assigns[0].field.to_string(), "cursor");
    assert_eq!(pagination.assigns[1].field.to_string(), "per_page");
    assert_eq!(
        pagination.assigns[2].field.to_string(),
        "send_cursor_on_first"
    );
    assert_eq!(
        pagination.assigns[3].field.to_string(),
        "stop_when_cursor_missing"
    );
    assert_eq!(
        pagination.bindings[0].controller_field.to_string(),
        "cursor"
    );
    assert_eq!(
        pagination.bindings[0].endpoint_rust_field.to_string(),
        "cursor"
    );
    assert_eq!(
        pagination.bindings[1].controller_field.to_string(),
        "per_page"
    );
    assert_eq!(
        pagination.bindings[1].endpoint_rust_field.to_string(),
        "count"
    );
}

#[test]
fn pagination_resolution_lowers_type_driven_assignment_binding() {
    let api = analyze_ok(
        r#"
        api! {
            client PageApi {
                base "https://example.com"
            }

            GET Offset(count: u64 = 20)
                path ["offset"]
                query {
                    count
                }
                paginate OffsetLimitPagination {
                    per_page = count
                }
                -> Json<Vec<String>>
        }
        "#,
    );

    let pagination = endpoint_pagination(single_endpoint(&api));
    let controller_ty = &pagination.controller_ty;
    assert_eq!(
        quote::quote!(#controller_ty).to_string(),
        "OffsetLimitPagination"
    );
    assert_eq!(pagination.assigns.len(), 1);
    assert_eq!(pagination.assigns[0].field.to_string(), "per_page");
    assert!(matches!(
        &pagination.assigns[0].value,
        PaginationValueKind::EpField(field) if field.to_string() == "count"
    ));
    assert_eq!(pagination.bindings.len(), 1);
    assert_eq!(
        pagination.bindings[0].controller_field.to_string(),
        "per_page"
    );
    assert_eq!(
        pagination.bindings[0].endpoint_rust_field.to_string(),
        "count"
    );
}

#[test]
fn pagination_resolution_lowers_custom_controller_bindings() {
    let api = analyze_ok(
        r#"
        api! {
            client PageApi {
                base "https://example.com"
            }

            GET List(page: u64 = 1, count: u64 = 2)
                path ["items"]
                paginate HeaderPagePagination {
                    page = page,
                    count = count
                }
                -> Json<Vec<String>>
        }
        "#,
    );

    let pagination = endpoint_pagination(single_endpoint(&api));
    let controller_ty = &pagination.controller_ty;
    assert_eq!(
        quote::quote!(#controller_ty).to_string(),
        "HeaderPagePagination"
    );
    assert_eq!(pagination.assigns.len(), 2);
    assert_eq!(pagination.bindings.len(), 2);
    assert_eq!(pagination.bindings[0].controller_field.to_string(), "page");
    assert_eq!(
        pagination.bindings[0].endpoint_rust_field.to_string(),
        "page"
    );
    assert_eq!(pagination.bindings[1].controller_field.to_string(), "count");
    assert_eq!(
        pagination.bindings[1].endpoint_rust_field.to_string(),
        "count"
    );
    assert!(matches!(
        &pagination.assigns[0].value,
        PaginationValueKind::EpField(field) if field.to_string() == "page"
    ));
    assert!(matches!(
        &pagination.assigns[1].value,
        PaginationValueKind::EpField(field) if field.to_string() == "count"
    ));
}
