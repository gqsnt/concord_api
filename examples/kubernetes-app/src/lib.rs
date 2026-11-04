//! The external caller of the Kubernetes SDK slice: two workflows as library functions over a
//! configured [`KubeContext`](concord_example_kubernetes_sdk::KubeContext), using only the SDK's
//! public exports, `concord` and `concord-pagination`.
//!
//! - **KU-A** [`inventory::inventory`] — typed workload inventory across Namespaces, Pods,
//!   Deployments, Services, ConfigMaps and Jobs, every collection read in consistent chunks
//!   ([`collect::collect_list`] over `concord_pagination::sequential`), with completion and
//!   resource-version metadata per collection and an explicit restart outcome when a snapshot
//!   expires.
//! - **KU-B** [`list_watch::watch_pods`] / [`list_watch::watch_deployments`] — initial list,
//!   watch from its resource version through `concord::jsonl::JsonValuesReader`, typed changes
//!   applied to a current-state map, reconnect from the last resource version, relist with an
//!   explicit reset boundary after `410`, bounded by a policy and a caller-owned stop signal.
//!
//! Each workflow runs end to end against the loopback test server in `tests/ku_a_inventory.rs` and
//! `tests/ku_b_list_watch.rs`; `examples/ku_a.rs` / `examples/ku_b.rs` run the same functions
//! against a real API server when `KUBE_API_URL` is set (read-only), and print `NOT_RUN`
//! otherwise.

pub mod collect;
pub mod inventory;
// pub mod list_watch; (waits for the JSON values reader)
mod stop;
