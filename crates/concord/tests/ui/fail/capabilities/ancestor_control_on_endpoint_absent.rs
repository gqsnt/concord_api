use concord::{api, Binary};

api! {
    client TreeControlApi {
        timeout 30s;
        base "https://tree-controls.example.test";

        scope regional(region: String, language?: String) {
            GET item(id: String) -> concord::Bytes via Binary;
        }
    }
}

fn main() {
    let endpoint = TreeControlApi::unbound()
        .regional("eu".to_owned())
        .item("42".to_owned());
    let _ = endpoint.set_language(Some("fr".to_owned()));
}
