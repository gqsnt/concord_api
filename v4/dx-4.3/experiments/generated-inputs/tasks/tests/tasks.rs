//! Runs the baseline and candidate solutions against the same fixture and compares what reached
//! the wire. Without the `candidate` feature only the baseline runs (on any Concord).

use concord::reqwest;
use concord_example_fixture::{Recorded, Server, services};
use concord_example_sdk::{Catalog, CatalogConfig, CatalogTenantListing};

fn listings(server: &Server) -> (CatalogTenantListing, CatalogTenantListing) {
    let api = Catalog::new(reqwest::Client::new(), CatalogConfig { key: "k".into() })
        .with_base_url(format!("{}/catalog/v1", server.url()))
        .unwrap()
        .set_region("south");
    let featured = vec!["featured".to_owned()];
    (
        api.tenant("acme").listing().set_default_tags(featured.clone()),
        api.tenant("studio").listing().set_default_tags(featured),
    )
}

fn wire(server: &Server) -> Vec<(String, Option<String>)> {
    server
        .requests()
        .iter()
        .map(|r: &Recorded| (r.target.clone(), r.header("x-region").map(str::to_owned)))
        .collect()
}

macro_rules! run_tasks {
    ($m:ident) => {{
        use concord_g1_tasks::$m;
        let (one, two, three) = (services().await, services().await, services().await);
        let (acme, studio) = listings(&one);
        let t1 = $m::t1_saved_search(&acme, &studio).await.unwrap();
        let (acme, _) = listings(&two);
        let t2 = $m::t2_clear_for_one_call(&acme).await.unwrap();
        let (acme, studio) = listings(&three);
        let t3 = $m::t3_rerun_elsewhere(concord_g1_tasks::lamp_searches(&acme), &studio)
            .await
            .unwrap();
        ((t1, t2, t3), (wire(&one), wire(&two), wire(&three)))
    }};
}

#[tokio::test]
async fn baseline_solves_the_tasks() {
    let ((t1, t2, t3), (w1, w2, w3)) = run_tasks!(baseline);
    assert_eq!(t1, [3, 3, 3, 1, 2, 3, 4]);
    assert_eq!(t2, ("limit=20".to_owned(), false, 6, 4));
    assert_eq!(t3, (vec![5, 20], vec![1, 1]));
    let targets: Vec<&str> = w1.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(
        targets,
        [
            "/catalog/v1/tenants/acme/items?tag=featured&q=e&limit=3",
            "/catalog/v1/tenants/studio/items?tag=featured&q=e&limit=3",
            "/catalog/v1/tenants/acme/items?q=e&limit=3",
            "/catalog/v1/tenants/studio/items?tag=furniture&q=e&limit=3",
            "/catalog/v1/tenants/acme/items?tag=featured&q=e&limit=2",
            "/catalog/v1/tenants/acme/items?q=e&limit=3",
            "/catalog/v1/tenants/acme/items?tag=featured&limit=20",
        ]
    );
    assert!(w1.iter().all(|(_, region)| region.as_deref() == Some("south")));
    assert_eq!(w2[0].1, None);
    assert_eq!(w2[1].1.as_deref(), Some("south"));
    assert!(w3.iter().all(|(t, _)| t.starts_with("/catalog/v1/tenants/studio/")));
}

#[cfg(feature = "candidate")]
#[tokio::test]
async fn candidate_solves_the_same_tasks_with_the_same_requests() {
    let (baseline, baseline_wire) = run_tasks!(baseline);
    let (candidate, candidate_wire) = run_tasks!(candidate);
    assert_eq!(candidate, baseline);
    assert_eq!(candidate_wire, baseline_wire);
}
