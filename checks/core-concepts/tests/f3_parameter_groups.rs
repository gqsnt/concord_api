//! F3: a repeated parameter block declared once (`params Name(..);` on the client, `..Name` in an
//! endpoint) instead of repeated on every endpoint, and the generated trait `Name` that lets an
//! SDK write one adapter over every endpoint including it (Kubernetes' hand-written
//! `ListOperation` + `macro_rules!` forwarding is the baseline it replaces).

use concord::{Endpoint, Json};
use serde::{Deserialize, Serialize};

/// A list envelope.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct List {
    /// Item names.
    pub items: Vec<String>,
}

/// One object.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Item {
    /// Its name.
    pub name: String,
}

concord::api! {
    /// A cluster-like API: list endpoints in two scopes share their list options.
    pub client Cluster(#[sensitive] header("authorization") token?: String) {
        base "http://cluster.invalid";
        format Json;

        /// Options every list endpoint accepts.
        params ListOptions(
            /// `labelSelector`.
            query("labelSelector") label_selector?: String,
            /// Page size.
            query limit?: u32,
            /// Opaque continuation (withheld from `Debug`).
            #[sensitive] query("continue") continue_token?: String,
            query("timeoutSeconds") timeout_seconds: u32 = 30,
        );

        /// Acting on behalf of another account (a header group).
        params OnBehalf(#[sensitive] header("x-on-behalf-of") account?: String);

        scope core {
            path "api/v1";
            GET pods(..ListOptions) -> List { path "pods"; }
            GET services(..ListOptions, ..OnBehalf) -> List { path "services"; }
            /// A single object: no list options (a scope value would have reached it).
            GET pod(name: String) -> Item { path "pods/{name}"; }
        }
        scope apps(namespace: String) {
            path "apis/apps/v1/namespaces/{namespace}";
            /// Values declared around the group keep their written order on the wire.
            GET deployments(query("watch") watch?: bool, ..ListOptions, query("pretty") pretty?: bool)
                -> List { path "deployments"; }
            POST scale(name: String, ..OnBehalf, body item: Item) -> Item { path "deployments/{name}/scale"; }
        }
    }
}

/// One adapter for every list endpoint: written once over the generated trait, with no
/// per-endpoint forwarding.
fn first_page<E: ListOptions>(endpoint: E, size: u32) -> E {
    endpoint
        .set_limit(size)
        .maybe_continue_token(None)
        .set_timeout_seconds(5u32)
}

fn next_page<E: ListOptions + Endpoint<List>>(endpoint: E, token: &str) -> concord::Prepared<List> {
    endpoint
        .set_continue_token(token)
        .prepare()
        .expect("builds")
}

fn cluster() -> Cluster {
    Cluster::new(concord::reqwest::Client::new())
}

#[test]
fn a_group_reaches_exactly_the_endpoints_that_include_it() {
    let c = cluster();
    let pods = first_page(c.core().pods(), 50).prepare().unwrap();
    assert_eq!(
        pods.request().url().as_str(),
        "http://cluster.invalid/api/v1/pods?limit=50&timeoutSeconds=5"
    );
    let services = first_page(c.core().services(), 10)
        .set_label_selector("app=web")
        .set_account("acct_9")
        .prepare()
        .unwrap();
    assert_eq!(
        services.request().url().as_str(),
        "http://cluster.invalid/api/v1/services?labelSelector=app%3Dweb&limit=10&timeoutSeconds=5"
    );
    assert_eq!(services.request().headers()["x-on-behalf-of"], "acct_9");
    // The single-object endpoint has no list option (this does not compile:
    // `c.core().pod("x").set_limit(1)`), so its request carries none.
    let pod = c.core().pod("web-0").prepare().unwrap();
    assert_eq!(pod.request().url().query(), None);
}

#[test]
fn written_order_and_initialized_values_are_kept() {
    let deployments = cluster()
        .apps("prod")
        .deployments()
        .set_pretty(true)
        .set_watch(false)
        .set_limit(3u32)
        .prepare()
        .unwrap();
    assert_eq!(
        deployments.request().url().as_str(),
        "http://cluster.invalid/apis/apps/v1/namespaces/prod/deployments?watch=false&limit=3&timeoutSeconds=30&pretty=true"
    );
}

#[test]
fn the_trait_continues_any_including_endpoint_and_sensitivity_follows_the_group() {
    let c = cluster();
    let continued = next_page(c.apps("dev").deployments(), "tok-SECRET");
    let shown = format!("{continued:?}");
    assert!(!shown.contains("tok-SECRET"), "{shown}");
    assert!(
        continued
            .request()
            .url()
            .as_str()
            .contains("continue=tok-SECRET")
    );
    let on_behalf = c
        .apps("dev")
        .scale("web", Item { name: "web".into() })
        .set_account("acct-SECRET")
        .prepare()
        .unwrap();
    assert!(!format!("{on_behalf:?}").contains("acct-SECRET"));
}

#[test]
fn the_generated_trait_is_the_endpoint_setter_under_another_name() {
    // A trait method call and the inherent setter build the same request.
    let c = cluster();
    let via_trait = <ClusterCorePods as ListOptions>::set_limit(c.core().pods(), 7u32)
        .prepare()
        .unwrap();
    let inherent = c.core().pods().set_limit(7u32).prepare().unwrap();
    assert_eq!(via_trait.request().url(), inherent.request().url());
}
