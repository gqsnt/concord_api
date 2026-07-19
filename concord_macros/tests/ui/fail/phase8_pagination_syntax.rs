use concord_macros::api;

api! {
    client MissingBlock {
        base "https://example.test";
        GET Read(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination
        }
    }
}

api! {
    client MissingAssignment {
        base "https://example.test";
        GET Read(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination { offset = ep.offset }
        }
    }
}

api! {
    client DuplicateAssignment {
        base "https://example.test";
        GET Read(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination {
                offset = ep.offset,
                offset = ep.offset,
                page_size = ep.limit
            }
        }
    }
}

api! {
    client AdditionalAssignment {
        base "https://example.test";
        GET Read(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination {
                offset = ep.offset,
                page_size = ep.limit,
                extra = ep.limit
            }
        }
    }
}

api! {
    client IndirectAssignment {
        base "https://example.test";
        var offset: u64;
        GET Read(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination {
                offset = vars.offset,
                page_size = ep.limit
            }
        }
    }
}

api! {
    client BuiltInTrailingComma {
        base "https://example.test";
        GET Read(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination {
                offset = ep.offset,
                page_size = ep.limit,
            }
        }
    }
}

api! {
    client CustomTrailingComma {
        base "https://example.test";
        GET Read() -> Json<Vec<u8>> {
            paginate custom crate::factory("value",);
        }
    }
}

api! {
    client PaginationClauseOrder {
        base "https://example.test";
        GET Read() -> Json<Vec<u8>> {
            paginate custom crate::factory();
            timeout 1s;
        }
    }
}

fn main() {}
