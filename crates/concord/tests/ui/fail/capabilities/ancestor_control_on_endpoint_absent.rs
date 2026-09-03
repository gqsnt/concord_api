use concord::api;

api! {
    client TreeControlApi {
        timeout off;
        base "https://tree-controls.example.test";

        scope regional(region: String, language?: String) {
            GET item(id: String) -> concord::Bytes using concord::Bytes;
        }
    }
}

fn main() {
    let endpoint = TreeControlApi::unbound()
        .regional("eu".to_owned())
        .item("42".to_owned());
    let _ = endpoint.with_language("fr".to_owned());
}
