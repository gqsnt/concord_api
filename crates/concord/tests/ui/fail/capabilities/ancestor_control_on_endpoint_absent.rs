// §4.5 / §7.3: a scope value is selected on the scope handle before the
// endpoint is created; neither the endpoint intent nor the request exposes
// ancestor value controls.
use concord::{Binary, api};

api! {
    client TreeControlApi {
        timeout 30s;
        base "https://tree-controls.example.test";

        scope regional(region: String, language?: String) {
            path "{region}";
            query { "language" = language; }

            GET item(id: String) -> concord::Bytes via Binary {
                path "items/{id}";
            }
        }
    }
}

fn main() {
    let _ = TreeControlApi::unbound()
        .regional("eu")
        .set_language("fr")
        .item("42");
    let endpoint = TreeControlApi::unbound()
        .regional("eu".to_owned())
        .item("42".to_owned());
    let _ = endpoint.set_language("fr");
    let client = TreeControlApi::new().unwrap();
    let _ = client.regional("eu").item("42").set_language("fr");
}
