use concord_macros::api;

api! {
    client UndefinedField {
        base "https://example.test";
        GET Read(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination {
                offset = ep.missing,
                page_size = ep.limit
            }
        }
    }
}

api! {
    client WrongOffsetType {
        base "https://example.test";
        GET Read(offset: String, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination {
                offset = ep.offset,
                page_size = ep.limit
            }
        }
    }
}

api! {
    client WrongCursorType {
        base "https://example.test";
        GET Read(cursor: String) -> Json<Vec<u8>> {
            paginate CursorPagination<String> { cursor = ep.cursor }
        }
    }
}

fn main() {}
