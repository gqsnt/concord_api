// §7.3: intents are constructed only by navigation from `unbound()`; there is
// no flat constructor that bypasses scope construction.
use concord::api;

api! {
    client TreeApi {
        timeout 30s;
        base "https://tree.example.test";

        scope regional(region: String) {
            path "regions/{region}";

            GET item(id: String) -> concord::Bytes via concord::Binary {
                path "items/{id}";
            }
        }
    }
}

fn main() {
    let _: tree_api::regional::ItemEndpoint = TreeApi::unbound().regional("eu").item("42");
    let _ = tree_api::regional::ItemEndpoint::from_flat_values(
        "eu".to_owned(),
        "42".to_owned(),
    );
}
