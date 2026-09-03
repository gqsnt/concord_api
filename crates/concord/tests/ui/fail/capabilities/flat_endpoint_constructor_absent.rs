use concord::api;

api! {
    client TreeApi {
        timeout off;
        base "https://tree.example.test";

        scope regional(region: String) {
            GET item(id: String) -> concord::Bytes using concord::Bytes;
        }
    }
}

fn main() {
    let _ = tree_api::regional::Item::from_flat_values(
        "eu".to_owned(),
        "42".to_owned(),
    );
}
