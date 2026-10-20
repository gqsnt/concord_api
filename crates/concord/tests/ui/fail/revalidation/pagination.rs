use concord::{Text, api};

api! {
    client Paged {
        base "http://example.test/";
        GET items(query page: u32 = 1) -> String via Text {
            revalidate etag;
            paginate via Pages {
                init Start { page: page };
                cursor -> page;
            }
        }
    }
}

fn main() {}
