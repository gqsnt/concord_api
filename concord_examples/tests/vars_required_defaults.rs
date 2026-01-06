// Path: concord_macros/tests/ex01_vars_required_defaults.rs
mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params {
            api_key: String;
            user_agent: String = "UA/1.0".to_string();
            client_trace?: bool;
        }
        headers {
            "user-agent" => user_agent,
            "x-api-key" => api_key,
            "x-trace" => client_trace,
        }
    }

    path "ping" {
        GET Ping "" -> TextEncoding<String>;
    }
}

#[test]
fn required_var_and_defaults_are_applied_in_base_policy() {
    let vars = client::ClientVars::new("KEY123".to_string());
    let ep = client::endpoints::Ping::new();

    let (_route, policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert_eq!(header(&policy, "user-agent").as_deref(), Some("UA/1.0"));
    assert_eq!(header(&policy, "x-api-key").as_deref(), Some("KEY123"));
    assert!(header(&policy, "x-trace").is_none());
}
