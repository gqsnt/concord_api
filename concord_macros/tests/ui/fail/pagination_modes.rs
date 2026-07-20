use concord_macros::api;

api! {
    client RequestBodyPagination {
        base "https://example.test";
        POST Read(body: Text) -> Json<Vec<u8>> {
            paginate custom crate::factory();
        }
    }
}

api! {
    client BytesPagination {
        base "https://example.test";
        GET Read() -> Bytes {
            paginate custom crate::factory();
        }
    }
}

api! {
    client NoContentPagination {
        base "https://example.test";
        GET Read() -> NoContent {
            paginate custom crate::factory();
        }
    }
}

api! {
    client StreamPagination {
        base "https://example.test";
        GET Read() -> Stream<OctetStream> {
            paginate custom crate::factory();
        }
    }
}

fn main() {}
