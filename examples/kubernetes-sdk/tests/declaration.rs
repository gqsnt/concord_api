//! Every declared operation, prepared (no I/O): method, path, query, headers and redaction,
//! checked against the paths and parameter names of the pinned OpenAPI v3 documents
//! (Kubernetes v1.37.2: `api__v1_openapi.json`, `apis__apps__v1_openapi.json`,
//! `apis__batch__v1_openapi.json`, `apis__networking.k8s.io__v1_openapi.json`,
//! `version_openapi.json`).

use concord::reqwest::{self, Method};
use concord_example_kubernetes_sdk::{
    BearerToken, ContinueToken, FieldSelector, Kube, KubeContext, LabelSelector, NamespaceName,
    ObjectName, ResourceVersion, ResourceVersionMatch,
};

const BASE: &str = "https://k8s.example.test:6443";
const TOKEN: &str = "synthetic-bearer-7f3a";

fn kube() -> KubeContext {
    KubeContext::new(reqwest::Client::new())
        .with_base_url(BASE)
        .unwrap()
        .with_bearer(&BearerToken::new(TOKEN))
        .unwrap()
}

fn ns(name: &str) -> NamespaceName {
    NamespaceName::new(name).unwrap()
}

fn name(name: &str) -> ObjectName {
    ObjectName::new(name).unwrap()
}

fn target<T>(prepared: &concord::Prepared<T>) -> String {
    let url = prepared.request().url();
    match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_owned(),
    }
}

fn check<T>(prepared: concord::Prepared<T>, expected: &str, accept: Option<&str>) {
    assert_eq!(prepared.request().method(), Method::GET);
    assert_eq!(target(&prepared), expected);
    let headers = prepared.request().headers();
    assert_eq!(headers["authorization"], format!("Bearer {TOKEN}").as_str());
    assert!(headers["authorization"].is_sensitive());
    assert_eq!(
        headers.get("accept").map(|v| v.to_str().unwrap()),
        accept,
        "{expected}"
    );
    let shown = format!("{prepared:?}");
    assert!(!shown.contains(TOKEN), "{shown}");
}

const JSON: Option<&str> = Some("application/json");

#[test]
fn header_names_compose() {
    Kube::validate_header_names().unwrap();
}

#[test]
fn every_operation_has_the_schema_path() -> Result<(), concord::Error> {
    let kube = kube();
    let a = kube.namespace(ns("team-a"));

    check(kube.client().version().prepare()?, "/version/", JSON);

    // core/v1, cluster-wide
    check(
        kube.core().namespaces().prepare()?,
        "/api/v1/namespaces",
        JSON,
    );
    check(
        kube.core().namespace(ns("team-a")).prepare()?,
        "/api/v1/namespaces/team-a",
        JSON,
    );
    check(kube.core().pods().prepare()?, "/api/v1/pods", JSON);
    check(kube.core().services().prepare()?, "/api/v1/services", JSON);
    check(
        kube.core().config_maps().prepare()?,
        "/api/v1/configmaps",
        JSON,
    );
    check(
        kube.core().watch_pods().prepare()?,
        "/api/v1/pods?allowWatchBookmarks=true&watch=1",
        JSON,
    );

    // core/v1, namespaced
    check(
        a.core().pods().prepare()?,
        "/api/v1/namespaces/team-a/pods",
        JSON,
    );
    check(
        a.core().pod(name("web-1")).prepare()?,
        "/api/v1/namespaces/team-a/pods/web-1",
        JSON,
    );
    check(
        a.core().watch_pods().prepare()?,
        "/api/v1/namespaces/team-a/pods?allowWatchBookmarks=true&watch=1",
        JSON,
    );
    check(
        a.core().services().prepare()?,
        "/api/v1/namespaces/team-a/services",
        JSON,
    );
    check(
        a.core().service(name("web")).prepare()?,
        "/api/v1/namespaces/team-a/services/web",
        JSON,
    );
    check(
        a.core().config_maps().prepare()?,
        "/api/v1/namespaces/team-a/configmaps",
        JSON,
    );
    check(
        a.core().config_map(name("app.config")).prepare()?,
        "/api/v1/namespaces/team-a/configmaps/app.config",
        JSON,
    );

    // apps/v1
    check(
        kube.apps().deployments().prepare()?,
        "/apis/apps/v1/deployments",
        JSON,
    );
    check(
        kube.apps().watch_deployments().prepare()?,
        "/apis/apps/v1/deployments?allowWatchBookmarks=true&watch=1",
        JSON,
    );
    check(
        a.apps().deployments().prepare()?,
        "/apis/apps/v1/namespaces/team-a/deployments",
        JSON,
    );
    check(
        a.apps().deployment(name("web")).prepare()?,
        "/apis/apps/v1/namespaces/team-a/deployments/web",
        JSON,
    );
    check(
        a.apps().watch_deployments().prepare()?,
        "/apis/apps/v1/namespaces/team-a/deployments?allowWatchBookmarks=true&watch=1",
        JSON,
    );

    // batch/v1
    check(kube.batch().jobs().prepare()?, "/apis/batch/v1/jobs", JSON);
    check(
        a.batch().jobs().prepare()?,
        "/apis/batch/v1/namespaces/team-a/jobs",
        JSON,
    );
    check(
        a.batch().job(name("migrate-42")).prepare()?,
        "/apis/batch/v1/namespaces/team-a/jobs/migrate-42",
        JSON,
    );

    // networking.k8s.io/v1: the dotted group name is a literal path segment, sent unescaped.
    check(
        kube.networking().ingresses().prepare()?,
        "/apis/networking.k8s.io/v1/ingresses",
        JSON,
    );
    check(
        kube.networking().watch_ingresses().prepare()?,
        "/apis/networking.k8s.io/v1/ingresses?allowWatchBookmarks=true&watch=1",
        JSON,
    );
    check(
        a.networking().ingresses().prepare()?,
        "/apis/networking.k8s.io/v1/namespaces/team-a/ingresses",
        JSON,
    );
    check(
        a.networking()
            .ingress(name("storefront.public"))
            .prepare()?,
        "/apis/networking.k8s.io/v1/namespaces/team-a/ingresses/storefront.public",
        JSON,
    );
    check(
        a.networking().watch_ingresses().prepare()?,
        "/apis/networking.k8s.io/v1/namespaces/team-a/ingresses?allowWatchBookmarks=true&watch=1",
        JSON,
    );
    Ok(())
}

#[test]
fn ingress_lists_share_the_list_parameters() -> Result<(), concord::Error> {
    let continued = kube()
        .namespace(ns("team-b"))
        .networking()
        .ingresses()
        .set_label_selector(
            LabelSelector::builder()
                .in_set("app.kubernetes.io/part-of", ["shop", "checkout"])
                .build()
                .unwrap(),
        )
        .set_field_selector(
            FieldSelector::builder()
                .not_equals("metadata.name", "legacy")
                .build()
                .unwrap(),
        )
        .set_limit(20)
        .set_continue_token(ContinueToken::from_server("synthetic-ingress-continue"))
        .prepare()?;
    assert_eq!(
        target(&continued),
        "/apis/networking.k8s.io/v1/namespaces/team-b/ingresses\
         ?labelSelector=app.kubernetes.io%2Fpart-of+in+%28checkout%2Cshop%29\
         &fieldSelector=metadata.name%21%3Dlegacy&limit=20&continue=synthetic-ingress-continue"
    );
    let shown = format!("{continued:?}");
    assert!(!shown.contains("synthetic-ingress-continue"), "{shown}");
    Ok(())
}

#[test]
fn list_parameters_use_the_schema_names_and_encode_selectors() -> Result<(), concord::Error> {
    let kube = kube();
    let labels = LabelSelector::builder()
        .equals("app.kubernetes.io/name", "web")
        .in_set("tier", ["api", "frontend"])
        .not_exists("canary")
        .build()
        .unwrap();
    let fields = FieldSelector::builder()
        .equals("status.phase", "Running")
        .not_equals("metadata.name", "a,b")
        .build()
        .unwrap();
    let request = kube
        .namespace(ns("team-b"))
        .core()
        .pods()
        .set_label_selector(labels)
        .set_field_selector(fields)
        .set_limit(2)
        .set_resource_version(ResourceVersion::new("4021"))
        .set_resource_version_match(ResourceVersionMatch::NotOlderThan)
        .set_timeout_seconds(30)
        .prepare()?;
    assert_eq!(
        target(&request),
        "/api/v1/namespaces/team-b/pods\
         ?labelSelector=app.kubernetes.io%2Fname%3Dweb%2Ctier+in+%28api%2Cfrontend%29%2C%21canary\
         &fieldSelector=status.phase%3DRunning%2Cmetadata.name%21%3Da%5C%2Cb\
         &limit=2&resourceVersion=4021&resourceVersionMatch=NotOlderThan&timeoutSeconds=30"
    );
    Ok(())
}

#[test]
fn continue_token_is_sent_but_withheld_from_diagnostics() -> Result<(), concord::Error> {
    let secretish = "eyJ2IjoibWV0YS5rOHMuaW8vdjEiLCJydiI6NDAyMX0";
    let request = kube()
        .core()
        .pods()
        .set_limit(500)
        .set_continue_token(ContinueToken::from_server(secretish))
        .prepare()?;
    assert_eq!(
        request.request().url().query(),
        Some(format!("limit=500&continue={secretish}").as_str())
    );
    let shown = format!("{request:?}");
    assert!(!shown.contains(secretish), "{shown}");
    assert!(shown.contains("continue=<redacted>"), "{shown}");
    Ok(())
}

#[test]
fn watch_parameters() -> Result<(), concord::Error> {
    let request = kube()
        .namespace(ns("team-a"))
        .apps()
        .watch_deployments()
        .set_label_selector(LabelSelector::equals("app", "web").unwrap())
        .set_resource_version(ResourceVersion::new("1500"))
        .set_timeout_seconds(300)
        .prepare()?;
    assert_eq!(
        target(&request),
        "/apis/apps/v1/namespaces/team-a/deployments\
         ?labelSelector=app%3Dweb&resourceVersion=1500&allowWatchBookmarks=true\
         &timeoutSeconds=300&watch=1"
    );
    let no_bookmarks = kube()
        .core()
        .watch_pods()
        .maybe_allow_watch_bookmarks(None)
        .prepare()?;
    assert_eq!(no_bookmarks.request().url().query(), Some("watch=1"));
    Ok(())
}

#[test]
fn two_namespaces_do_not_share_scope() -> Result<(), concord::Error> {
    let kube = kube();
    let a = kube.namespace(ns("team-a"));
    let b = kube.namespace(ns("team-b"));
    let pa = a.core().pods().prepare()?;
    let pb = b.core().pods().prepare()?;
    assert_eq!(target(&pa), "/api/v1/namespaces/team-a/pods");
    assert_eq!(target(&pb), "/api/v1/namespaces/team-b/pods");
    assert_eq!(
        target(&b.apps().deployments().prepare()?),
        "/apis/apps/v1/namespaces/team-b/deployments"
    );
    assert_eq!(
        target(&a.batch().jobs().prepare()?),
        "/apis/batch/v1/namespaces/team-a/jobs"
    );
    assert_eq!(
        target(&b.networking().ingresses().prepare()?),
        "/apis/networking.k8s.io/v1/namespaces/team-b/ingresses"
    );
    assert_eq!(a.namespace().as_str(), "team-a");
    assert_eq!(b.namespace().as_str(), "team-b");
    Ok(())
}

#[test]
fn credentials_are_per_context_and_redacted() -> Result<(), concord::Error> {
    let with = kube();
    let without = KubeContext::new(reqwest::Client::new()).with_base_url(BASE)?;
    let other = without
        .clone()
        .with_bearer(&BearerToken::new("other-token-91"))
        .unwrap();

    let none = without.core().pods().prepare()?;
    assert!(none.request().headers().get("authorization").is_none());
    let one = with.core().pods().prepare()?;
    let two = other.core().pods().prepare()?;
    assert_eq!(
        one.request().headers()["authorization"],
        "Bearer synthetic-bearer-7f3a"
    );
    assert_eq!(
        two.request().headers()["authorization"],
        "Bearer other-token-91"
    );

    for shown in [
        format!("{with:?}"),
        format!("{other:?}"),
        format!("{:?}", with.client()),
        format!("{:?}", with.namespace(ns("team-a"))),
    ] {
        assert!(
            !shown.contains(TOKEN) && !shown.contains("other-token-91"),
            "{shown}"
        );
    }
    Ok(())
}
