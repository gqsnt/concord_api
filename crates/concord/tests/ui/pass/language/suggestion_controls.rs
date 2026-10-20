// Nearby spellings do not alter valid resolution. Current values remain bare,
// ancestors remain qualified, and raw identifiers retain their Rust spelling.
use concord::{Binary, Bytes, api};

api! {
    client SuggestionControls {
        base "http://example.test";
        values { query tenant: String; }

        scope regional(query tenants: String) {
            GET read(tenet: String, r#type: String) -> Bytes via Binary {
                path "{tenet}";
                query { "type" = r#type; }
                headers {
                    "X-Root" = client.tenant;
                    "X-Regional" = regional.tenants;
                    "X-Combined" = concat [client.tenant, regional.tenants, tenet];
                }
            }
        }

        // A local exact match also wins over the identical ancestor spelling.
        GET local(tenant: String) -> Bytes via Binary {
            path "{tenant}";
            headers { "X-Root" = client.tenant; }
        }
    }
}

fn main() {
    let client = SuggestionControls::builder()
        .set_tenant("root")
        .build()
        .expect("client");
    let _ = client.regional("regional").read("local", "kind");
    let _ = client.local("local");
}
