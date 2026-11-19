#![deny(deprecated)]
::concord::api! {
    client Catalog {
        base "https://catalog.test";
        #[deprecated]
        GET old(n?: u8) -> Vec<u8> {
            query { n; }
            read concord::Json;
            paginate move |_call, _page| None;
        }
    }
}
fn main() {}
