// The right format without its import: Rust's own unresolved-type error at the name, with Rust's
// own suggestion to import it. Concord adds nothing (it does not know the name).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Item {
    pub id: u64,
}

concord::api! {
    pub client NoImport {
        base "http://x.test";
        format Json;
        POST create(body item: Item) -> Item;
    }
}

fn main() {}
