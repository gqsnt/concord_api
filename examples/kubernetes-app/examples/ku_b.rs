//! KU-B against a real API server (read-only: list and watch; it creates nothing).
//!
//! `cargo run -p concord-example-kubernetes-app --example ku_b` with `KUBE_API_URL` (see
//! `support/live.rs`), optional `KUBE_NAMESPACE` (default: all namespaces), `KUBE_KIND=pods|deployments`
//! (default pods) and `KUBE_WATCH_SECONDS` (default 30, the run's lifetime). Without
//! `KUBE_API_URL` it prints `NOT_RUN` and exits 0.

#[path = "support/live.rs"]
mod live;

use std::num::{NonZeroU32, NonZeroUsize};
use std::ops::ControlFlow;
use std::time::Duration;

use concord_example_kubernetes_app::collect::ListPolicy;
use concord_example_kubernetes_app::list_watch::{
    Change, ResourceSelector, WatchPolicy, watch_deployments, watch_pods,
};
use concord_example_kubernetes_sdk::NamespaceName;

fn print<T>(change: Change<'_, T>) -> ControlFlow<()> {
    match change {
        Change::Reset(b) => println!(
            "reset {:?}: snapshot {} objects {} vanished {}",
            b.cause,
            b.snapshot.as_str(),
            b.objects,
            b.vanished
        ),
        Change::Applied { kind, key, .. } => println!("{kind:?} {key}"),
        Change::Bookmark(rv) => println!("bookmark {}", rv.as_str()),
        Change::Reconnecting { from, after } => {
            println!("reconnect from {} after {after:?}", from.as_str())
        }
    }
    ControlFlow::Continue(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(kube) = live::context()? else {
        println!("NOT_RUN: KUBE_API_URL is not set (KU-B live/real-server lane)");
        return Ok(());
    };
    let selector = ResourceSelector {
        namespace: std::env::var("KUBE_NAMESPACE")
            .ok()
            .map(NamespaceName::new)
            .transpose()?,
        labels: None,
        fields: None,
    };
    let seconds: u64 = std::env::var("KUBE_WATCH_SECONDS")
        .ok()
        .map_or(Ok(30), |s| s.parse())?;
    let policy = WatchPolicy {
        list: ListPolicy::new(
            NonZeroU32::new(500).unwrap(),
            NonZeroUsize::new(100).unwrap(),
        ),
        server_timeout_seconds: Some(u32::try_from(seconds).unwrap_or(u32::MAX).min(300)),
        bookmarks: true,
        max_event_bytes: 4 * 1024 * 1024,
        max_events: 100_000,
        max_reconnects: 20,
        max_relists: 3,
    };
    let stop = tokio::time::sleep(Duration::from_secs(seconds));
    if std::env::var("KUBE_KIND").as_deref() == Ok("deployments") {
        let report = watch_deployments(&kube, &selector, policy, print, stop).await;
        println!(
            "outcome {:?}; {} objects; {:?}",
            report.outcome,
            report.state.len(),
            report.counts
        );
    } else {
        let report = watch_pods(&kube, &selector, policy, print, stop).await;
        println!(
            "outcome {:?}; {} objects; {:?}",
            report.outcome,
            report.state.len(),
            report.counts
        );
    }
    Ok(())
}
