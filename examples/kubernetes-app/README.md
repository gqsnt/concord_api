# Kubernetes caller workflows (KU-A, KU-B)

External caller of [`concord-example-kubernetes-sdk`](../kubernetes-sdk) (a bounded, read-only
Kubernetes v1.37.2 slice; see its `PROVIDER.md` and `FRICTION.md`). It depends only on the SDK's
public exports, `concord` and `concord-pagination`.

| Workflow | Public function | Output |
|---|---|---|
| KU-A workload inventory | `inventory::inventory(&KubeContext, &InventoryRequest) -> Inventory` | typed objects per family (Namespaces, Pods, Deployments, Services, ConfigMaps, Jobs) and scope, each with a `collect::Completion` (outcome `Complete` / `Bounded { resume }` / `Expired { boundary }` / `Failed { error, resume }`, snapshot `resourceVersion`, chunks, `remainingItemCount` estimates, restart boundaries); `ServerCheck` against the pinned version |
| KU-B list/watch/resync | `list_watch::watch_pods` / `watch_deployments(&KubeContext, &ResourceSelector, WatchPolicy, sink, stop) -> WatchReport<T>` (generic: `list_watch::list_watch`) | typed `Change` events to the sink; `WatchReport` with the current-state map, last resource version, every `ResetBoundary`, every `ConnectionRecord` and the `WatchOutcome` |

The reusable consistent-list reader is `collect::collect_list(template, ListPolicy, Resume)`: the
SDK's chunk factory (`list::ListPager::fetch`) driven by `concord_pagination::sequential`, with an
explicit `OnExpired` policy.

## Commands

Offline (default gate; loopback test server, no credentials):

```sh
cargo test --locked -p concord-example-kubernetes-app --test ku_a_inventory     # KU-A
cargo test --locked -p concord-example-kubernetes-app --test ku_b_list_watch    # KU-B (wire)
cargo test --locked -p concord-example-kubernetes-app --test ku_b_segmentation  # KU-B (framing)
cargo test --locked -p concord-example-kubernetes-app --test watch_operation    # one watch, generated send()
cargo test --locked -p concord-example-kubernetes-app --test fixtures_manifest  # fixture digests
cargo test --locked -p concord-example-kubernetes-sdk                           # SDK
```

Against a real API server (read-only; the same functions; prints `NOT_RUN` and exits 0 without
`KUBE_API_URL`):

```sh
KUBE_API_URL=https://127.0.0.1:6443 KUBE_TOKEN=... KUBE_CA_FILE=ca.pem \
  KUBE_NAMESPACES=team-a,team-b cargo run -p concord-example-kubernetes-app --example ku_a
KUBE_API_URL=... KUBE_NAMESPACE=team-a KUBE_KIND=pods KUBE_WATCH_SECONDS=30 \
  cargo run -p concord-example-kubernetes-app --example ku_b
```

Neither runner creates, modifies or deletes anything. A disposable-cluster lane that creates
objects in its own namespace to observe real events is not implemented; use only a cluster you
are authorized to read.

## Tests by acceptance item

| Acceptance (contract KU-A / KU-B) | Test |
|---|---|
| typed get/list across the six families, two namespaces, scope isolation | `ku_a_two_namespaces_selectors_and_an_empty_chunk`, `ku_a_cluster_wide_reports_remaining_item_count_without_using_it` |
| selector encoding | `ku_a_two_namespaces_selectors_and_an_empty_chunk` (wire); SDK `tests/declaration.rs::list_parameters_use_the_schema_names_and_encode_selectors` |
| optional `remainingItemCount`, never used to plan | `ku_a_cluster_wide_reports_remaining_item_count_without_using_it`, `ku_a_long_chain_is_strictly_sequential` |
| consistent collection `resourceVersion` across a chain | `ku_a_two_namespaces_selectors_and_an_empty_chunk`, `ku_a_a_different_snapshot_mid_chain_is_a_failure`, `ku_a_bounded_chain_resumes_from_its_checkpoint` |
| expired token and explicit restart outcome | `ku_a_expired_token_restarts_with_an_explicit_boundary`, `ku_a_expired_token_reported_when_restarts_are_not_allowed` |
| initial list, watch from its resource version, typed changes | `ku_b_list_watch_applies_typed_changes_and_reconnects_after_normal_end` |
| split JSON tokens / UTF-8 | `ku_b_list_watch_applies_typed_changes_and_reconnects_after_normal_end`, `ku_b_split_utf8_is_decoded_intact`, `ku_b_segmentation::every_split_offset_decodes_the_same_events` |
| normal stream end | `ku_b_list_watch_applies_typed_changes_and_reconnects_after_normal_end`, `ku_b_deployments_in_a_second_namespace` |
| transport loss → reconnect from last resource version | `ku_b_transport_loss_reconnects_from_the_last_resource_version` |
| bookmark events | `ku_b_list_watch_applies_typed_changes_and_reconnects_after_normal_end`, `ku_b_transport_loss_reconnects_from_the_last_resource_version` |
| malformed / oversized events | `ku_b_malformed_events_end_the_connection_and_reconnect`, `ku_b_an_oversized_event_is_a_limit_not_a_clean_end`, `ku_b_segmentation::events_need_no_newline_and_an_unfinished_event_is_not_an_end` |
| history expiry / 410 → relist with reset boundary | `ku_b_expired_history_relists_with_an_explicit_reset_boundary`, `ku_b_410_on_open_relists_and_the_relist_budget_is_explicit` |
| consumer drop | `ku_b_dropping_the_workflow_releases_the_watch_connection`, `ku_b_a_consumer_can_stop_the_run` |
| cancellation while awaiting data | `ku_b_cancellation_while_awaiting_data` |
| bounded lifetime | `WatchPolicy` budgets in every KU-B test; `ku_b_a_long_watch_keeps_bounded_state` |
| one watch through the SDK's generated `send()` (no workflow): returns before the body is read, chunk splits, `410` answer and `ERROR` event, refused head kept unread, declared identity in read errors, early drop is not end of input | `watch_operation.rs` (seven tests) |

The workflow opens each watch with `watch::open` and a reader configured from
`WatchPolicy::max_event_bytes` (an application bound); a caller content with the reader's
defaults uses the generated `send()` directly, as `watch_operation.rs` does.
