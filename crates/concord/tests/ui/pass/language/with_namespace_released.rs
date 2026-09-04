use concord::{Binary, Bytes, api};

api! {
    client EndpointNameApi {
        timeout 30s;
        base "http://endpoint-name.example.test";
        values { locale?: String; }

        GET with_locale() -> Bytes via Binary;
    }
}

api! {
    client ScopeNameApi {
        timeout 30s;
        base "http://scope-name.example.test";
        values { locale?: String; }

        scope with_locale() {
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client NestedNameApi {
        timeout 30s;
        base "http://nested-name.example.test";

        scope regional(region?: String) {
            GET with_region() -> Bytes via Binary;
        }
    }
}

fn main() {
    let endpoint_client = EndpointNameApi::builder()
        .set_locale(Some("fr".to_owned()))
        .build()
        .expect("configured endpoint-name client");
    let _ = endpoint_client.with_locale();

    let scope_client = ScopeNameApi::builder()
        .omit_locale()
        .build()
        .expect("configured scope-name client");
    let _ = scope_client.with_locale().read();

    let nested_client = NestedNameApi::new().expect("zero-input nested-name client");
    let _ = nested_client
        .regional()
        .omit_region()
        .with_region();
}
