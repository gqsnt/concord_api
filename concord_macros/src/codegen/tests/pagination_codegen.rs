use super::helpers::*;
use quote::quote;

#[test]
fn generated_pagination_marker_covers_controller_types() {
    let out = expanded(quote! {
        client PaginationModelApi {
            base "https://example.com"
        }

        GET Offset(start: u64 = 0, count: u64 = 20)
            path ["offset"]
            query {
                start
                count
            }
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
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = OffsetLimitPagination",
            ":: concord_core :: advanced :: PaginateBinding",
            "let __pagination_plan = :: core :: option :: Option :: Some",
            "PaginationMarker",
        ],
    );
}

#[test]
fn generated_pagination_contains_pagination_marker() {
    let out = expanded(quote! {
        client SnapshotPagination {
            base "https://example.com"
        }

        GET List(start: u64 = 0, count: u64 = 20)
            as list
            path ["items"]
            query {
                start
                count
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "let __pagination_plan = :: core :: option :: Option :: Some",
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "PaginateBinding",
            "PaginationMarker",
        ],
    );
}

#[test]
fn generated_non_paginated_endpoint_sets_pagination_marker_none() {
    let out = expanded(quote! {
        client SnapshotNoPagination {
            base "https://example.com"
        }

        GET Ping()
            path ["ping"]
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &["let __pagination_plan = :: core :: option :: Option :: None"],
    );
    assert!(!out.contains("PaginationMarker"));
}

#[test]
fn generated_pagination_bindings_for_offset_limit() {
    let out = expanded(quote! {
        client SnapshotPaginationBindings {
            base "https://example.com"
        }

        GET List(start: u64 = 0, count: u64 = 20)
            query {
                "from" = start
                "pageSize" = count
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            ":: concord_core :: advanced :: PaginateBinding < OffsetLimitPagination >",
            "fn load_pagination",
            "fn store_pagination",
            "pagination . offset = self . start . clone ()",
            "pagination . limit = self . count . clone ()",
            "self . start = pagination . offset . clone ()",
            "self . count = pagination . limit . clone ()",
        ],
    );
}

#[test]
fn generated_offset_limit_uses_type_driven_pagination_only() {
    let out = expanded(quote! {
        client SnapshotOffsetLimitRuntime {
            base "https://example.com"
        }

        GET List(start: u64 = 0, count: u64 = 20)
            headers {
                "X-Start" = start,
                "X-Count" = count,
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = OffsetLimitPagination",
            ":: concord_core :: advanced :: PaginateBinding < OffsetLimitPagination >",
        ],
    );
}

#[test]
fn generated_offset_limit_exposes_pagination_binding_and_type() {
    let out = expanded(quote! {
        client SnapshotOffsetLimitSingleObjectRuntime {
            base "https://example.com"
        }

        GET List(start: u64 = 0, count: u64 = 20)
            headers {
                "X-Start" = start,
                "X-Count" = count,
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = OffsetLimitPagination",
            ":: concord_core :: advanced :: PaginateBinding < OffsetLimitPagination >",
        ],
    );
}

#[test]
fn generated_offset_limit_emits_paginate_binding_impl() {
    let out = expanded(quote! {
        client SnapshotOffsetLimitBinding {
            base "https://example.com"
        }

        GET List(start: u64 = 0, count: u64 = 20)
            headers {
                "X-Start" = start,
                "X-Count" = count,
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            ":: concord_core :: advanced :: PaginateBinding < OffsetLimitPagination >",
            "fn load_pagination",
            "fn store_pagination",
            "pagination . offset = self . start . clone ()",
            "pagination . limit = self . count . clone ()",
            "self . start = pagination . offset . clone ()",
            "self . count = pagination . limit . clone ()",
        ],
    );
}

#[test]
fn generated_paged_uses_type_driven_pagination_only() {
    let out = expanded(quote! {
        client SnapshotPagedRuntime {
            base "https://example.com"
        }

        GET List(page: u64 = 1, count: u64 = 2)
            headers {
                "X-Page" = page,
                "X-Count" = count,
            }
            paginate PagedPagination {
                page = page,
                per_page = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = PagedPagination",
            ":: concord_core :: advanced :: PaginateBinding < PagedPagination >",
        ],
    );
}

#[test]
fn generated_paged_emits_paginate_binding_impl() {
    let out = expanded(quote! {
        client SnapshotPagedBinding {
            base "https://example.com"
        }

        GET List(page: u64 = 1, count: u64 = 20)
            headers {
                "X-Page" = page,
                "X-Count" = count,
            }
            paginate PagedPagination {
                page = page,
                per_page = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            ":: concord_core :: advanced :: PaginateBinding < PagedPagination >",
            "fn load_pagination",
            "fn store_pagination",
            "pagination . page = self . page . clone ()",
            "pagination . per_page = self . count . clone ()",
            "self . page = pagination . page . clone ()",
            "self . count = pagination . per_page . clone ()",
        ],
    );
}

#[test]
fn generated_paged_exposes_pagination_binding_and_type() {
    let out = expanded(quote! {
        client SnapshotPagedSingleObjectRuntime {
            base "https://example.com"
        }

        GET List(page: u64 = 1, count: u64 = 20)
            headers {
                "X-Page" = page,
                "X-Count" = count,
            }
            paginate PagedPagination {
                page = page,
                per_page = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = PagedPagination",
            ":: concord_core :: advanced :: PaginateBinding < PagedPagination >",
        ],
    );
}

#[test]
fn generated_custom_uses_type_driven_pagination_only() {
    let out = expanded(quote! {
        client SnapshotCustomPagination {
            base "https://example.com"
        }

        GET List(page: u64 = 1, count: u64 = 2)
            headers {
                "X-Page" = page,
                "X-Count" = count,
            }
            paginate HeaderPagePagination {
                page = page,
                count = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = HeaderPagePagination",
            ":: concord_core :: advanced :: PaginateBinding < HeaderPagePagination >",
            "HeaderPagePagination",
        ],
    );
    assert_contains_all(
        &out,
        &[
            "let __pagination_plan = :: core :: option :: Option :: Some",
            "PaginationMarker",
        ],
    );
}

#[test]
fn generated_custom_emits_paginate_binding_impl() {
    let out = expanded(quote! {
        client SnapshotCustomPaginationBinding {
            base "https://example.com"
        }

        GET List(page: u64 = 1, count: u64 = 2)
            headers {
                "X-Page" = page,
                "X-Count" = count,
            }
            paginate HeaderPagePagination {
                page = page,
                count = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            ":: concord_core :: advanced :: PaginateBinding < HeaderPagePagination >",
            "fn load_pagination",
            "fn store_pagination",
            "pagination . page = self . page . clone ()",
            "pagination . count = self . count . clone ()",
            "self . page = pagination . page . clone ()",
            "self . count = pagination . count . clone ()",
        ],
    );
}

#[test]
fn generated_custom_literal_assignment_is_load_only() {
    let out = expanded(quote! {
        client SnapshotCustomPaginationLiteralBinding {
            base "https://example.com"
        }

        GET List(page: u64 = 1, count: u64 = 2)
            headers {
                "X-Page" = page,
                "X-Count" = count,
            }
            paginate HeaderPagePagination {
                page = page,
                count = count,
                max_pages = 3
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            ":: concord_core :: advanced :: PaginateBinding < HeaderPagePagination >",
            "fn load_pagination",
            "fn store_pagination",
            "pagination . page = self . page . clone ()",
            "pagination . count = self . count . clone ()",
            "pagination . max_pages =",
            "self . page = pagination . page . clone ()",
            "self . count = pagination . count . clone ()",
        ],
    );
    assert!(
        !out.contains("self . max_pages"),
        "literal config fields must not be stored back to endpoint state"
    );
}

#[test]
fn generated_custom_exposes_pagination_binding_and_type() {
    let out = expanded(quote! {
        client SnapshotCustomPaginationSingleObjectRuntime {
            base "https://example.com"
        }

        GET List(page: u64 = 1, count: u64 = 2)
            headers {
                "X-Page" = page,
                "X-Count" = count,
            }
            paginate HeaderPagePagination {
                page = page,
                count = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = HeaderPagePagination",
            ":: concord_core :: advanced :: PaginateBinding < HeaderPagePagination >",
        ],
    );
}

#[test]
fn generated_custom_pagination_uses_type_driven_pagination_only() {
    generated_custom_uses_type_driven_pagination_only();
}

#[test]
fn generated_cursor_uses_type_driven_pagination_only() {
    let out = expanded(quote! {
        client SnapshotCursorRuntime {
            base "https://example.com"
        }

        GET List(cursor?: String, count: u64 = 2)
            headers {
                "X-Cursor" = cursor,
                "X-Count" = count,
            }
            paginate CursorPagination<String> {
                cursor = cursor,
                per_page = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = CursorPagination < String >",
            ":: concord_core :: advanced :: PaginateBinding < CursorPagination < String > >",
        ],
    );
}

#[test]
fn generated_cursor_emits_paginate_binding_impl() {
    let out = expanded(quote! {
        client SnapshotCursorBinding {
            base "https://example.com"
        }

        GET List(cursor?: String, count: u64 = 2)
            headers {
                "X-Cursor" = cursor,
                "X-Count" = count,
            }
            paginate CursorPagination<String> {
                cursor = cursor,
                per_page = count,
                send_cursor_on_first = true,
                stop_when_cursor_missing = false
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            ":: concord_core :: advanced :: PaginateBinding < CursorPagination < String > >",
            "fn load_pagination",
            "fn store_pagination",
            "pagination . cursor = self . cursor . clone ()",
            "pagination . per_page = self . count . clone ()",
            "pagination . send_cursor_on_first = (true)",
            "pagination . stop_when_cursor_missing = (false)",
            "self . cursor = pagination . cursor . clone ()",
            "self . count = pagination . per_page . clone ()",
        ],
    );
    assert!(
        !out.contains("self.send_cursor_on_first=pagination.send_cursor_on_first"),
        "cursor flags must not be stored back to endpoint state"
    );
    assert!(
        !out.contains("self.stop_when_cursor_missing=pagination.stop_when_cursor_missing"),
        "cursor flags must not be stored back to endpoint state"
    );
}

#[test]
fn generated_cursor_exposes_pagination_binding_and_type() {
    let out = expanded(quote! {
        client SnapshotCursorSingleObjectRuntime {
            base "https://example.com"
        }

        GET List(cursor?: String, count: u64 = 2)
            headers {
                "X-Cursor" = cursor,
                "X-Count" = count,
            }
            paginate CursorPagination<String> {
                cursor = cursor,
                per_page = count,
                send_cursor_on_first = true,
                stop_when_cursor_missing = false
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = CursorPagination < String >",
            ":: concord_core :: advanced :: PaginateBinding < CursorPagination < String > >",
        ],
    );
}

#[test]
fn generated_cursor_pagination_preserves_controller_flags() {
    let out = expanded(quote! {
        client SnapshotCursorFlags {
            base "https://example.com"
        }

        GET List(cursor?: String, count: u64 = 2)
            headers {
                "X-Cursor" = cursor,
                "X-Count" = count,
            }
            paginate CursorPagination<String> {
                cursor = cursor,
                per_page = count,
                send_cursor_on_first = true,
                stop_when_cursor_missing = false
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: PaginatedEndpoint",
            "type Pagination = CursorPagination < String >",
            ":: concord_core :: advanced :: PaginateBinding < CursorPagination < String > >",
            "send_cursor_on_first = (true)",
            "stop_when_cursor_missing = (false)",
        ],
    );
}

#[test]
fn generated_pagination_binding_clones_non_copy_cursor() {
    let out = expanded(quote! {
        client SnapshotPaginationCursorFlow {
            base "https://example.com"
        }

        GET List(cursor?: String, count: u64 = 20)
            query {
                cursor
                count
            }
            paginate CursorPagination<String> {
                cursor = cursor,
                per_page = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(
        &out,
        &[
            ":: concord_core :: advanced :: PaginateBinding < CursorPagination < String > >",
            "pagination . cursor = self . cursor . clone ()",
            "self . cursor = pagination . cursor . clone ()",
        ],
    );
}

#[test]
fn generated_pagination_public_surface_exposes_collect_only() {
    let out = expanded(quote! {
        client SnapshotPaginationSurface {
            base "https://example.com"
        }

        GET List(start: u64 = 0, count: u64 = 20)
            path ["items"]
            query {
                start
                count
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            -> Json<Vec<String>>
    });

    assert_contains_all(&out, &["#[doc=\"Pagination: OffsetLimitPagination\"]"]);
}
