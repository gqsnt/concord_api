//! Typed models against schema-derived documents (field names of the pinned v1.37.2 OpenAPI
//! v3 schemas), list-envelope item extraction and continuation keys, and watch event shapes from
//! the Kubernetes API concepts page.

use concord_example_kubernetes_sdk::list::{ListError, into_page};
use concord_example_kubernetes_sdk::model::{
    ConfigMap, Deployment, Ingress, IngressList, IntOrString, Job, JobState, Namespace,
    NamespacePhase, PathType, Pod, PodList, PodPhase, Resource, Service, ServiceType, Status,
};
use concord_example_kubernetes_sdk::watch::WatchEvent;
use concord_example_kubernetes_sdk::{ObjectKey, ResourceVersion};

#[test]
fn ingress_decodes_rules_backends_tls_and_status() {
    // Field names of `io.k8s.api.networking.v1.Ingress` in
    // `apis__networking.k8s.io__v1_openapi.json` (v1.37.2); values synthetic.
    let ingress: Ingress = json(
        r#"{"kind":"Ingress","apiVersion":"networking.k8s.io/v1",
            "metadata":{"name":"storefront","namespace":"team-a","resourceVersion":"5201",
                        "annotations":{"nginx.ingress.kubernetes.io/rewrite-target":"/"}},
            "spec":{"ingressClassName":"nginx",
                    "defaultBackend":{"service":{"name":"storefront","port":{"number":80}}},
                    "tls":[{"hosts":["shop.example.test"],"secretName":"shop-tls"}],
                    "rules":[{"host":"shop.example.test","http":{"paths":[
                        {"path":"/","pathType":"Prefix","backend":{"service":{"name":"storefront","port":{"name":"http"}}}},
                        {"path":"/cart","pathType":"Exact","backend":{"service":{"name":"cart","port":{"number":8080}}}},
                        {"path":"/static","pathType":"ImplementationSpecific",
                         "backend":{"resource":{"apiGroup":"k8s.example.test","kind":"StorageBucket","name":"assets"}}}]}},
                             {"http":{"paths":[{"path":"/x","pathType":"RegularExpression",
                                                "backend":{"service":{"name":"other"}}}]}}]},
            "status":{"loadBalancer":{"ingress":[{"ip":"192.0.2.10","ports":[{"port":443,"protocol":"TCP"}]}]}}}"#,
    );
    assert_eq!(ingress.spec.ingress_class_name.as_deref(), Some("nginx"));
    assert_eq!(ingress.spec.tls[0].secret_name.as_deref(), Some("shop-tls"));
    let rule = &ingress.spec.rules[0];
    assert_eq!(rule.host.as_deref(), Some("shop.example.test"));
    let paths = &rule.http.as_ref().unwrap().paths;
    assert_eq!(paths[0].path_type, Some(PathType::Prefix));
    assert_eq!(paths[1].path_type, Some(PathType::Exact));
    assert_eq!(paths[2].path_type, Some(PathType::ImplementationSpecific));
    assert_eq!(
        paths[2].backend.resource.as_ref().unwrap().kind,
        "StorageBucket"
    );
    // A rule without a host matches every host; an unknown path type still decodes.
    assert_eq!(ingress.spec.rules[1].host, None);
    assert_eq!(
        ingress.spec.rules[1].http.as_ref().unwrap().paths[0].path_type,
        Some(PathType::Other)
    );
    let backends: Vec<_> = ingress
        .service_backends()
        .map(|b| {
            let port = b.port.as_ref();
            (
                b.name.as_str(),
                port.and_then(|p| p.name.as_deref()),
                port.and_then(|p| p.number),
            )
        })
        .collect();
    assert_eq!(
        backends,
        [
            ("storefront", None, Some(80)),
            ("storefront", Some("http"), None),
            ("cart", None, Some(8080)),
            ("other", None, None),
        ]
    );
    assert!(ingress.has_address());
    assert_eq!(
        ObjectKey::of(&ingress).unwrap().to_string(),
        "team-a/storefront"
    );
    assert_eq!(Ingress::API_VERSION, "networking.k8s.io/v1");
    const { assert!(Ingress::NAMESPACED) };

    let list: IngressList = json(
        r#"{"kind":"IngressList","apiVersion":"networking.k8s.io/v1",
            "metadata":{"resourceVersion":"5300","continue":"T"},
            "items":[{"metadata":{"name":"storefront","namespace":"team-a"}}]}"#,
    );
    let page = into_page(list, None).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.next.unwrap().snapshot().as_str(), "5300");

    let event: WatchEvent<Ingress> = json(
        r#"{"type":"DELETED","object":{"kind":"Ingress","apiVersion":"networking.k8s.io/v1",
            "metadata":{"name":"storefront","namespace":"team-a","resourceVersion":"5310"}}}"#,
    );
    assert_eq!(event.type_name(), "DELETED");
    assert_eq!(event.resource_version().unwrap().as_str(), "5310");
}

fn json<T: serde::de::DeserializeOwned>(text: &str) -> T {
    serde_json::from_str(text).unwrap()
}

#[test]
fn objects_decode_with_unknown_fields_tolerated() {
    let pod: Pod = json(
        r#"{"kind":"Pod","apiVersion":"v1",
            "metadata":{"name":"web-1","namespace":"team-a","uid":"7d0c1f2e",
                        "resourceVersion":"1201","labels":{"app":"web"},
                        "managedFields":[{"manager":"kubelet"}],"futureField":{"x":1}},
            "spec":{"nodeName":"node-1","containers":[{"name":"app","image":"registry.example/web:1"}],
                    "restartPolicy":"Always"},
            "status":{"phase":"Running","podIP":"10.0.0.7","qosClass":"BestEffort",
                      "containerStatuses":[{"name":"app","ready":true,"restartCount":2,
                                            "image":"registry.example/web:1","imageID":"sha256:00"}]}}"#,
    );
    assert_eq!(pod.status.phase, Some(PodPhase::Running));
    assert_eq!(pod.status.pod_ip.as_deref(), Some("10.0.0.7"));
    assert_eq!(pod.spec.node_name.as_deref(), Some("node-1"));
    assert!(pod.all_containers_ready());
    assert_eq!(pod.status.container_statuses[0].restart_count, 2);
    let key = ObjectKey::of(&pod).unwrap();
    assert_eq!(key.to_string(), "team-a/web-1");

    let odd: Pod = json(r#"{"metadata":{"name":"x"},"status":{"phase":"Hibernating"}}"#);
    assert_eq!(odd.status.phase, Some(PodPhase::Other));

    let deployment: Deployment = json(
        r#"{"metadata":{"name":"web","namespace":"team-a","generation":3},
            "spec":{"replicas":2,"selector":{"matchLabels":{"app":"web"}},"template":{}},
            "status":{"observedGeneration":3,"replicas":2,"readyReplicas":2,"availableReplicas":2,
                      "conditions":[{"type":"Available","status":"True","reason":"MinimumReplicasAvailable"}]}}"#,
    );
    assert!(deployment.is_available());
    assert_eq!(deployment.spec.selector.match_labels["app"], "web");
    let stale: Deployment = json(
        r#"{"metadata":{"name":"web","generation":4},"spec":{"replicas":2,"selector":{}},
            "status":{"observedGeneration":3,"availableReplicas":2}}"#,
    );
    assert!(!stale.is_available());

    let service: Service = json(
        r#"{"metadata":{"name":"web","namespace":"team-a"},
            "spec":{"type":"ClusterIP","clusterIP":"10.96.0.10","selector":{"app":"web"},
                    "ports":[{"name":"http","port":80,"targetPort":8080,"protocol":"TCP"},
                             {"name":"metrics","port":9090,"targetPort":"metrics"}]}}"#,
    );
    assert_eq!(service.spec.kind, Some(ServiceType::ClusterIP));
    assert_eq!(
        service.spec.ports[0].target_port,
        Some(IntOrString::Int(8080))
    );
    assert_eq!(
        service.spec.ports[1].target_port,
        Some(IntOrString::String("metrics".into()))
    );

    let config: ConfigMap = json(
        r#"{"metadata":{"name":"app.config","namespace":"team-a"},
            "data":{"LOG_LEVEL":"debug","DB_URL":"postgres://synthetic-secret@db"}}"#,
    );
    let shown = format!("{config:?}");
    assert!(
        shown.contains("LOG_LEVEL") && !shown.contains("synthetic-secret"),
        "{shown}"
    );

    let job: Job = json(
        r#"{"metadata":{"name":"migrate-42","namespace":"team-b"},
            "spec":{"completions":1,"backoffLimit":6,"template":{}},
            "status":{"succeeded":1,"conditions":[{"type":"SuccessCriteriaMet","status":"True"},
                                                   {"type":"Complete","status":"True"}]}}"#,
    );
    assert_eq!(job.state(), JobState::Complete);

    let namespace: Namespace =
        json(r#"{"metadata":{"name":"team-a"},"status":{"phase":"Terminating"}}"#);
    assert_eq!(namespace.status.phase, Some(NamespacePhase::Terminating));
    assert_eq!(ObjectKey::of(&namespace).unwrap().to_string(), "team-a");
}

#[test]
fn list_envelope_items_move_out_and_continue_keys_bind_the_snapshot() {
    // The three chunks of the API-concepts example (pods 1-500, 501-1000, 1001-1253), reduced.
    let first: PodList = json(
        r#"{"kind":"PodList","apiVersion":"v1",
            "metadata":{"resourceVersion":"10245","continue":"ENCODED_CONTINUE_TOKEN","remainingItemCount":753},
            "items":[{"metadata":{"name":"p1","namespace":"team-a"}}]}"#,
    );
    let page = into_page(first, None).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.meta.remaining_item_count, Some(753));
    assert_eq!(page.meta.snapshot, Some(ResourceVersion::new("10245")));
    let next = page.next.unwrap();
    assert_eq!(next.token().expose(), "ENCODED_CONTINUE_TOKEN");
    assert_eq!(next.snapshot().as_str(), "10245");
    assert!(!format!("{next:?}").contains("ENCODED"));

    // An empty chunk with a token continues; `items: null` reads as empty.
    let empty: PodList =
        json(r#"{"metadata":{"resourceVersion":"10245","continue":"T2"},"items":null}"#);
    let page = into_page(empty, Some(&next)).unwrap();
    assert!(page.items.is_empty());
    assert!(page.next.is_some());
    assert_eq!(page.meta.remaining_item_count, None);

    // The last chunk: `continue: ""` ends the chain.
    let last: PodList =
        json(r#"{"metadata":{"resourceVersion":"10245","continue":""},"items":[]}"#);
    assert!(into_page(last, Some(&next)).unwrap().next.is_none());

    // A continued chunk from another snapshot is refused, not merged.
    let moved: PodList = json(r#"{"metadata":{"resourceVersion":"10300"},"items":[]}"#);
    assert!(matches!(
        into_page(moved, Some(&next)),
        Err(ListError::InconsistentSnapshot)
    ));
    let missing: PodList = json(r#"{"metadata":{},"items":[]}"#);
    assert!(matches!(
        into_page(missing, Some(&next)),
        Err(ListError::InconsistentSnapshot)
    ));
    // A continued chunk returning the token it was requested with would repeat forever.
    let stuck: PodList = json(
        r#"{"metadata":{"resourceVersion":"10245","continue":"ENCODED_CONTINUE_TOKEN"},
            "items":[{"metadata":{"name":"p1","namespace":"team-a"}}]}"#,
    );
    assert!(matches!(
        into_page(stuck, Some(&next)),
        Err(ListError::ContinueDidNotAdvance)
    ));
    // A token without a snapshot cannot be checked later.
    let unbound: PodList = json(r#"{"metadata":{"continue":"T"},"items":[]}"#);
    assert!(matches!(
        into_page(unbound, None),
        Err(ListError::MissingSnapshot)
    ));
}

#[test]
fn watch_events_decode_by_type() {
    let added: WatchEvent<Pod> = json(
        r#"{"type":"ADDED","object":{"kind":"Pod","apiVersion":"v1",
            "metadata":{"name":"web-1","namespace":"team-a","resourceVersion":"10596"}}}"#,
    );
    assert!(
        matches!(&added, WatchEvent::Added(p) if p.metadata.name.as_ref().unwrap().as_str() == "web-1")
    );
    assert_eq!(added.resource_version().unwrap().as_str(), "10596");

    // Members in the other order decode the same way.
    let modified: WatchEvent<Pod> = json(
        r#"{"object":{"metadata":{"name":"web-1","resourceVersion":"11020"}},"type":"MODIFIED"}"#,
    );
    assert_eq!(modified.type_name(), "MODIFIED");
    assert_eq!(modified.resource_version().unwrap().as_str(), "11020");

    let bookmark: WatchEvent<Pod> = json(
        r#"{"type":"BOOKMARK","object":{"kind":"Pod","apiVersion":"v1","metadata":{"resourceVersion":"12746"}}}"#,
    );
    match &bookmark {
        WatchEvent::Bookmark(b) => assert!(!b.is_initial_events_end()),
        other => panic!("{other:?}"),
    }
    assert_eq!(bookmark.resource_version().unwrap().as_str(), "12746");

    let end: WatchEvent<Pod> = json(
        r#"{"type":"BOOKMARK","object":{"metadata":{"resourceVersion":"9",
            "annotations":{"k8s.io/initial-events-end":"true"}}}}"#,
    );
    assert!(matches!(end, WatchEvent::Bookmark(b) if b.is_initial_events_end()));

    let error: WatchEvent<Pod> = json(
        r#"{"type":"ERROR","object":{"kind":"Status","apiVersion":"v1","metadata":{},
            "status":"Failure","message":"too old resource version: 100 (2000)",
            "reason":"Expired","code":410}}"#,
    );
    match &error {
        WatchEvent::Error(status) => {
            assert!(status.is_expired());
            assert!(!format!("{status:?}").contains("too old"));
        }
        other => panic!("{other:?}"),
    }
    assert!(error.resource_version().is_none());

    assert!(serde_json::from_str::<WatchEvent<Pod>>(r#"{"type":"SYNC","object":{}}"#).is_err());
    assert!(serde_json::from_str::<WatchEvent<Pod>>(r#"{"type":"ADDED"}"#).is_err());
}

#[test]
fn expired_status_document() {
    // The shape `apierrors.NewResourceExpired` produces for an expired continue token, with the
    // inconsistent-continuation token the `continue` parameter documentation describes.
    let status: Status = json(
        r#"{"kind":"Status","apiVersion":"v1","metadata":{"continue":"INCONSISTENT"},
            "status":"Failure","message":"The provided continue parameter is too old ...",
            "reason":"Expired","code":410}"#,
    );
    assert!(status.is_expired());
    assert_eq!(
        status.metadata.next_token().unwrap().expose(),
        "INCONSISTENT"
    );
    let shown = format!("{status:?}");
    assert!(
        !shown.contains("INCONSISTENT") && !shown.contains("too old"),
        "{shown}"
    );
}
