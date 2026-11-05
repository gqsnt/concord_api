//! KU-A against a real API server (read-only: `GET /version/`, namespace gets and lists).
//!
//! `cargo run -p concord-example-kubernetes-app --example ku_a` with `KUBE_API_URL` (see
//! `support/live.rs`), optional `KUBE_NAMESPACES=a,b` (default: all namespaces) and
//! `KUBE_LABEL=key=value`. Without `KUBE_API_URL` it prints `NOT_RUN` and exits 0.

#[path = "support/live.rs"]
mod live;

use std::num::{NonZeroU32, NonZeroUsize};

use concord_example_kubernetes_app::collect::ListPolicy;
use concord_example_kubernetes_app::inventory::{InventoryRequest, InventoryScope, inventory};
use concord_example_kubernetes_sdk::{LabelSelector, NamespaceName};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(kube) = live::context()? else {
        println!("NOT_RUN: KUBE_API_URL is not set (KU-A live/real-server lane)");
        return Ok(());
    };
    let scope = match std::env::var("KUBE_NAMESPACES") {
        Ok(list) => InventoryScope::Namespaces(
            list.split(',')
                .map(NamespaceName::new)
                .collect::<Result<_, _>>()?,
        ),
        Err(_) => InventoryScope::AllNamespaces,
    };
    let label_selector = match std::env::var("KUBE_LABEL") {
        Ok(pair) => {
            let (k, v) = pair.split_once('=').ok_or("KUBE_LABEL must be key=value")?;
            Some(LabelSelector::equals(k, v)?)
        }
        Err(_) => None,
    };
    let request = InventoryRequest {
        scope,
        label_selector,
        pod_field_selector: None,
        list: ListPolicy::new(
            NonZeroU32::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        ),
    };
    let inv = inventory(&kube, &request).await;
    match &inv.server.observed {
        Ok(v) => println!(
            "server {} (pinned {}, same minor: {:?})",
            v.git_version,
            inv.server.pinned,
            inv.server.same_minor()
        ),
        Err(e) => println!("server version not read: {e}"),
    }
    println!("{:?}", inv.summary());
    for (family, completions) in [
        (
            "pods",
            inv.pods
                .iter()
                .map(|f| (&f.scope, &f.completion))
                .collect::<Vec<_>>(),
        ),
        (
            "deployments",
            inv.deployments
                .iter()
                .map(|f| (&f.scope, &f.completion))
                .collect(),
        ),
        (
            "services",
            inv.services
                .iter()
                .map(|f| (&f.scope, &f.completion))
                .collect(),
        ),
        (
            "configmaps",
            inv.config_maps
                .iter()
                .map(|f| (&f.scope, &f.completion))
                .collect(),
        ),
        (
            "jobs",
            inv.jobs.iter().map(|f| (&f.scope, &f.completion)).collect(),
        ),
    ] {
        for (scope, c) in completions {
            println!(
                "{family} {scope:?}: complete={} snapshot={:?} chunks={} restarts={}",
                c.outcome.is_complete(),
                c.snapshot.as_ref().map(|s| s.as_str()),
                c.chunks,
                c.restarts.len()
            );
        }
    }
    println!(
        "{}",
        if inv.is_complete() {
            "COMPLETE"
        } else {
            "INCOMPLETE"
        }
    );
    Ok(())
}
