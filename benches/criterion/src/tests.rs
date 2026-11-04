//! Equivalence at the registered consuming boundary, plus comparator discrimination controls.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::header::{ACCEPT, HeaderValue};

use super::*;
use crate::contract::{Observation, Observe, Prepared};

fn fx() -> Fixtures {
    Fixtures::new(http_client())
}

/// A test runner for the existing registry. Neither benchmark runner calls `observe`.
#[test]
fn registered_pairs_match() {
    #[derive(Default)]
    struct Pairs {
        group: &'static str,
        outputs: BTreeMap<String, Vec<(Case, Observation)>>,
        ids: Vec<String>,
    }
    impl Runner for Pairs {
        fn case<I, O: Observe>(
            &mut self,
            case: Case,
            mut input: impl FnMut() -> I,
            mut run: impl FnMut(I) -> O,
        ) {
            let id = format!("{}/{}", self.group, case.id);
            let observed = run(input()).observe();
            // Reconstruct each input: one-shot responses and owned bodies must do fresh work.
            assert_eq!(observed, run(input()).observe(), "fresh input: {id}");
            assert!(!case.setup.is_empty(), "setup inventory: {id}");
            let mut stages = vec![case.run_stage.to_owned()];
            if !case.trivial_input {
                println!(
                    "INVENTORY {id}/input | {} | different features: input representation | no equal-work claim",
                    case.setup
                );
            }
            if case.total {
                stages.push(format!("input_{}", case.run_stage));
            }
            for stage in stages {
                println!(
                    "INVENTORY {id}/{stage} | {} | equal-work group {}/{} | {}",
                    case.setup,
                    self.group,
                    case.pair,
                    case.comparator()
                );
            }
            self.ids.push(id);
            self.outputs
                .entry(format!("{}/{}", self.group, case.pair))
                .or_default()
                .push((case, observed));
        }
    }
    let fx = fx();
    let mut pairs = Pairs::default();
    for group in GROUPS {
        pairs.group = group;
        run_group(group, &fx, &mut pairs);
    }
    let mut ids = pairs.ids.clone();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), pairs.ids.len(), "benchmark IDs must be unique");
    assert_eq!(ids.len(), 92, "update the inventory if cases change");
    assert_eq!(pairs.outputs.len(), 45, "every equal-work group is covered");
    let mut comparisons = 0;
    for (pair, members) in pairs.outputs {
        assert!(members.len() >= 2, "unpaired equal-work case: {pair}");
        let (reference, expected) = &members[0];
        for (case, actual) in &members[1..] {
            assert_eq!(reference.run_stage, case.run_stage, "{pair}");
            assert_eq!(reference.comparator(), case.comparator(), "{pair}");
            assert_eq!(
                expected, actual,
                "{pair}: {} versus {}",
                reference.id, case.id
            );
            comparisons += 1;
        }
        println!("PAIR {pair}: {} registered outputs equal", members.len());
    }
    assert_eq!(comparisons, 47);
    println!(
        "EQUIVALENCE: 92 registered cases; 45 groups; 47 comparisons; fresh inputs checked twice"
    );
}

/// Pin the S3 prepared contract independently of the pair's implementation.
#[test]
fn s3_prepares_json_accept_before_client_defaults() {
    let mut defaults = reqwest::header::HeaderMap::new();
    defaults.insert(ACCEPT, HeaderValue::from_static("text/plain"));
    let http = reqwest::Client::builder()
        .no_proxy()
        .default_headers(defaults)
        .build()
        .unwrap();
    let fx = Fixtures::new(http);
    let c = fx.s3.search().prepare().unwrap();
    let m = fx.s3_manual.search().into_request().build().unwrap();
    assert_eq!(Prepared::observe(c.request()), Prepared::observe(&m));
    for request in [c.request(), &m] {
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(
            request.url().as_str(),
            "https://api.example.com/v1/items/search?limit=20"
        );
        assert_eq!(request.headers()[ACCEPT], "application/json");
        assert!(request.headers()["authorization"].is_sensitive());
        assert!(request.body().is_none());
    }
}

fn control() -> reqwest::Request {
    let mut request = reqwest::Request::new(
        reqwest::Method::POST,
        "https://example.invalid/control?a=1&b=2".parse().unwrap(),
    );
    request
        .headers_mut()
        .insert(ACCEPT, HeaderValue::from_static("application/json"));
    for value in ["first", "second", "second"] {
        request
            .headers_mut()
            .append("x-repeat", HeaderValue::from_static(value));
    }
    *request.body_mut() = Some("body".into());
    request
}

#[track_caller]
fn detected(original: &reqwest::Request, changed: &reqwest::Request) {
    assert_ne!(Prepared::observe(original), Prepared::observe(changed));
}

#[test]
fn comparator_detects_altered_and_missing_accept() {
    let original = control();
    let mut changed = original.try_clone().unwrap();
    changed
        .headers_mut()
        .insert(ACCEPT, HeaderValue::from_static("text/plain"));
    detected(&original, &changed);
    changed.headers_mut().remove(ACCEPT);
    detected(&original, &changed);
}

#[test]
fn comparator_detects_repeated_value_order_and_multiplicity() {
    let original = control();
    for values in [
        ["first", "changed", "second"].as_slice(),
        ["second", "first", "second"].as_slice(),
        ["first", "second"].as_slice(),
    ] {
        let mut changed = original.try_clone().unwrap();
        changed.headers_mut().remove("x-repeat");
        for value in values {
            changed
                .headers_mut()
                .append("x-repeat", HeaderValue::from_static(value));
        }
        detected(&original, &changed);
    }
}

#[test]
fn comparator_detects_exact_url_and_method() {
    let original = control();
    let mut changed = original.try_clone().unwrap();
    // Same query multiset, different exact URL.
    changed.url_mut().set_query(Some("b=2&a=1"));
    detected(&original, &changed);
    changed = original.try_clone().unwrap();
    *changed.method_mut() = reqwest::Method::GET;
    detected(&original, &changed);
}

#[test]
fn comparator_detects_body_changes_and_absence_versus_empty() {
    let original = control();
    let mut changed = original.try_clone().unwrap();
    for body in [Some("changed"), Some(""), None] {
        *changed.body_mut() = body.map(reqwest::Body::from);
        detected(&original, &changed);
    }
    let absent = changed.try_clone().unwrap();
    *changed.body_mut() = Some("".into());
    detected(&absent, &changed);
}

#[test]
fn comparator_detects_native_configuration_and_sensitivity() {
    let original = control();
    let mut changed = original.try_clone().unwrap();
    *changed.timeout_mut() = Some(Duration::from_secs(1));
    detected(&original, &changed);
    changed = original.try_clone().unwrap();
    *changed.version_mut() = reqwest::Version::HTTP_2;
    detected(&original, &changed);
    changed = original.try_clone().unwrap();
    changed
        .headers_mut()
        .get_mut(ACCEPT)
        .unwrap()
        .set_sensitive(true);
    detected(&original, &changed);
}

#[test]
fn comparator_accepts_reconstruction_and_cross_name_insertion_order() {
    let original = control();
    assert_eq!(Prepared::observe(&original), Prepared::observe(&control()));
    let mut reordered = original.try_clone().unwrap();
    reordered.headers_mut().clear();
    for value in original.headers().get_all("x-repeat") {
        reordered.headers_mut().append("x-repeat", value.clone());
    }
    reordered
        .headers_mut()
        .insert(ACCEPT, original.headers()[ACCEPT].clone());
    assert_eq!(Prepared::observe(&original), Prepared::observe(&reordered));
}

/// These send benchmarks are explicitly partial floors, with equal decoded values on the
/// selected successful fixture. Their prepared requests and general metadata/error paths differ.
#[test]
fn loopback_partial_floors_return_the_same_values() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let large = payload::json_bytes(ROUNDTRIP_LARGE.target);
        let addr = server::serve(&large).await;
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let base_url = format!("http://{addr}/v1");
        let s1 = shapes::s1_simple::SimpleApi::new(http.clone())
            .with_base_url(base_url.clone())
            .unwrap();
        let s1_manual = base::s1_simple::SimpleApi::with_client(
            http.clone(),
            format!("{base_url}/").parse().unwrap(),
        );
        assert_eq!(
            s1.health().send().await.unwrap().status,
            s1_manual.health().await.unwrap().status
        );
        let prepared = s1.health().prepare().unwrap();
        let request = http.get(format!("{base_url}/health")).build().unwrap();
        assert!(prepared.request().headers().contains_key(ACCEPT));
        assert!(!request.headers().contains_key(ACCEPT));
        let c = prepared.send().await.unwrap();
        let m = http
            .execute(request)
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<base::s1_simple::Health>()
            .await
            .unwrap();
        assert_eq!(c.status, m.status);
        assert_eq!(c.status, "ok");
        let bodies = apis::bodies::Bodies::new(http.clone())
            .with_base_url(base_url.clone())
            .unwrap();
        let manual = manual::Bodies::new(http.clone(), &base_url);
        assert_eq!(
            bodies.get_json().send().await.unwrap(),
            manual.get_json().await.unwrap()
        );
        let c = bodies.get_json().prepare().unwrap().send().await.unwrap();
        let request = http.get(format!("{base_url}/json")).build().unwrap();
        let m = http
            .execute(request)
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<payload::Payload>()
            .await
            .unwrap();
        assert_eq!(c, m);
        assert_eq!(c, payload::payload(ROUNDTRIP_LARGE.target));
    });
    // Dropping this owned runtime stops its loopback listener and connection tasks.
}
