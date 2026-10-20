// §7.1 / VALUE-03: v3 value controls are `set_`/`replace_`/`set_x_if_some`/
// `omit_`/`reset_`; the v2 `with_` control namespace is released, so child
// endpoints and scopes may be named `with_<value>` next to that value.
use concord::{Binary, Bytes, api};

api! {
    client EndpointNameApi {
        timeout 30s;
        base "http://endpoint-name.example.test";
        values { locale?: String; }
        query { "locale" = locale; }

        GET with_locale() -> Bytes via Binary;
    }
}

api! {
    client ScopeNameApi {
        timeout 30s;
        base "http://scope-name.example.test";
        values { locale?: String; }
        query { "locale" = locale; }

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
            query { "region" = region; }

            GET with_region() -> Bytes via Binary;
        }
    }
}

fn main() {
    let endpoint_client = EndpointNameApi::builder()
        .replace_locale(Some("fr".to_owned()))
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
