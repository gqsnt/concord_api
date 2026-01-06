mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;
use core::time::Duration;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        timeout: Duration::from_secs(30),
        params { }
        headers { }
    }

    // Path-level timeout
    path "v1" {
        timeout:Duration::from_secs(10),
        path "ping" {
            // Endpoint-level timeout (no delimiter before `->` must parse)
            GET Ping "" timeout:Duration::from_secs(5) -> TextEncoding<String>;

            // Inherit from path
            GET PingInherit "" -> TextEncoding<String>;
        }
    }

    // Inherit from client
    GET Root "" -> TextEncoding<String>;
}

#[test]
fn timeout_layering_client_path_endpoint_and_runtime_override() {
    let vars = client::ClientVars::new();

    // Endpoint timeout wins over path/client
    let ep = client::endpoints::Ping::new();
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars.clone(), &ep);
    assert_eq!(timeout(&p), Some(Duration::from_secs(5)));

    // Inherit from path timeout
    let ep = client::endpoints::PingInherit::new();
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars.clone(), &ep);
    assert_eq!(timeout(&p), Some(Duration::from_secs(10)));

    // Inherit from client timeout
    let ep = client::endpoints::Root::new();
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars.clone(), &ep);
    assert_eq!(timeout(&p), Some(Duration::from_secs(30)));

    // Runtime override: Set
    let ep = client::endpoints::Ping::new().with_timeout(Duration::from_secs(2));
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars.clone(), &ep);
    assert_eq!(timeout(&p), Some(Duration::from_secs(2)));

    // Runtime override: Clear
    let ep = client::endpoints::Ping::new().without_timeout();
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);
    assert_eq!(timeout(&p), None);
}
