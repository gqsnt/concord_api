//! The benchmarks compare like with like: each Concord request equals its hand-written
//! counterpart (method, URL, header multiset, body). Not a correctness suite for Concord.

use std::collections::BTreeMap;

use super::*;

fn header_multiset(r: &reqwest::Request) -> BTreeMap<String, Vec<Vec<u8>>> {
    let mut map: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();
    for (k, v) in r.headers() {
        map.entry(k.as_str().to_owned())
            .or_default()
            .push(v.as_bytes().to_vec());
    }
    map
}

fn body(r: &reqwest::Request) -> Option<Vec<u8>> {
    r.body()
        .map(|b| b.as_bytes().expect("buffered body").to_vec())
}

#[track_caller]
fn same<T>(concord: concord::Prepared<T>, manual: reqwest::Request) {
    let c = concord.request();
    assert_eq!(c.method(), manual.method());
    assert_eq!(c.url().as_str(), manual.url().as_str());
    assert_eq!(header_multiset(c), header_multiset(&manual));
    assert_eq!(body(c), body(&manual));
}

fn fx() -> Fixtures {
    Fixtures::new(http_client())
}

#[test]
fn realistic_requests_match() {
    let fx = fx();
    same(
        fx.s1.health().prepare().unwrap(),
        fx.simple_manual.health().unwrap(),
    );
    same(
        tracker_full(&fx).prepare().unwrap(),
        tracker_full_manual(&fx).build().unwrap(),
    );
    same(
        fx.tracker
            .org("acme")
            .project("rocket")
            .issues(42)
            .prepare()
            .unwrap(),
        fx.tracker_manual
            .org("acme")
            .project("rocket")
            .issues(42)
            .build()
            .unwrap(),
    );
    let full = tracker_full(&fx).prepare().unwrap();
    assert_eq!(full.request().url().query().unwrap().split('&').count(), 10);
}

#[test]
fn header_requests_match() {
    let fx = fx();
    let h = &fx.headers;
    let m = &fx.headers_manual;
    let [a, b, c, d] = texts::<4>();
    same(
        h.text_4(a, b, c, d).prepare().unwrap(),
        m.text(texts::<4>()).unwrap(),
    );
    same(
        h.text_4(
            s(manual::HEADER_TEXT[0]),
            s(manual::HEADER_TEXT[1]),
            s(manual::HEADER_TEXT[2]),
            s(manual::HEADER_TEXT[3]),
        )
        .prepare()
        .unwrap(),
        m.prebuilt(4).unwrap(),
    );
    same(
        h.text_1(s(manual::HEADER_TEXT[0])).prepare().unwrap(),
        m.from_static(1).unwrap(),
    );
    let v = numbers::<16>();
    same(
        h.numeric_16(
            v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13],
            v[14], v[15],
        )
        .prepare()
        .unwrap(),
        m.numeric(v).unwrap(),
    );
    let t = texts::<16>();
    let [a, b, c, d, e, f, g, h0, i, j, k, l, mm, n, o, p] = t.clone();
    same(
        h.text_16(a, b, c, d, e, f, g, h0, i, j, k, l, mm, n, o, p)
            .prepare()
            .unwrap(),
        m.text(t).unwrap(),
    );
}

#[test]
fn native_and_literal_header_requests_match() {
    let fx = fx();
    let h = &fx.headers;
    let m = &fx.headers_manual;
    let v = &m.prebuilt;
    same(h.native_1(&v[0]).prepare().unwrap(), m.prebuilt(1).unwrap());
    same(
        h.native_4(&v[0], &v[1], &v[2], &v[3]).prepare().unwrap(),
        m.prebuilt(4).unwrap(),
    );
    same(
        h.native_16(
            &v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11],
            &v[12], &v[13], &v[14], &v[15],
        )
        .prepare()
        .unwrap(),
        m.prebuilt(16).unwrap(),
    );
    same(h.literal_1().prepare().unwrap(), m.from_static(1).unwrap());
    same(h.literal_4().prepare().unwrap(), m.from_static(4).unwrap());
    same(
        h.literal_16().prepare().unwrap(),
        m.from_static(16).unwrap(),
    );
}

#[test]
fn composition_requests_match() {
    let fx = fx();
    let [a, b, c, d, e, f, g, h] = deep_values();
    let scope = fx.deep.l1(a).l2(b).l3(c).l4(d).l5(e).l6(f).l7(g).l8(h);
    same(
        scope.leaf().prepare().unwrap(),
        fx.deep_manual.leaf(deep_values()).unwrap(),
    );
    same(
        scope.leaf().prepare().unwrap(),
        fx.deep_manual.scope(deep_values()).leaf().unwrap(),
    );

    let team = fx.compose.team("core");
    let tm = fx.compose_manual.team("core");
    same(
        team.inherit().prepare().unwrap(),
        tm.request(manual::Op::Inherit).unwrap(),
    );
    same(
        team.override_value().set_v("endpoint-a").prepare().unwrap(),
        tm.request(manual::Op::Override(Some(s("endpoint-a"))))
            .unwrap(),
    );
    same(
        team.override_value().prepare().unwrap(),
        tm.request(manual::Op::Override(None)).unwrap(),
    );
    same(
        team.append_value("endpoint-b").prepare().unwrap(),
        tm.request(manual::Op::Append(s("endpoint-b"))).unwrap(),
    );
    same(
        team.remove_value().prepare().unwrap(),
        tm.request(manual::Op::Remove).unwrap(),
    );
    let appended = team.append_value("endpoint-b").prepare().unwrap();
    assert_eq!(
        appended.request().headers().get_all("x-b").iter().count(),
        3
    );
}

#[test]
fn body_requests_match() {
    let fx = fx();
    let p = payload::payload(payload::MEDIUM.target);
    same(
        fx.bodies.post_json(p.clone()).prepare().unwrap(),
        fx.bodies_manual.post_json(&p).unwrap(),
    );
    let t = payload::text(100);
    same(
        fx.bodies.post_text(t.clone()).prepare().unwrap(),
        fx.bodies_manual.post_text(t).unwrap(),
    );
    let b = payload::binary(100);
    same(
        fx.bodies.post_bytes(b.clone()).prepare().unwrap(),
        fx.bodies_manual.post_bytes(b).unwrap(),
    );
}

#[test]
fn in_memory_decoding_completes_and_matches() {
    let fx = fx();
    let decoder = fx.bodies.get_json().prepare().unwrap().into_parts().2;
    let json = payload::json_bytes(payload::MEDIUM.target);
    let a = complete(decoder.decode(response("application/json", json.clone()))).unwrap();
    let b = complete(manual::decode::json(response("application/json", json))).unwrap();
    assert_eq!(a, b);
    assert_eq!(a, payload::payload(payload::MEDIUM.target));
}

/// Every group runs every case once (smoke check of the scenario code the benches execute).
#[test]
fn every_case_runs() {
    struct Once(Vec<String>, &'static str);
    impl Runner for Once {
        fn case<I, O>(
            &mut self,
            case: Case,
            mut input: impl FnMut() -> I,
            mut run: impl FnMut(I) -> O,
        ) {
            let _ = run(input());
            self.0.push(format!("{}/{}", self.1, case.id));
        }
    }
    let fx = fx();
    let mut once = Once(Vec::new(), "");
    for g in GROUPS {
        once.1 = g;
        run_group(g, &fx, &mut once);
    }
    let mut ids = once.0.clone();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), once.0.len(), "benchmark IDs are unique");
    assert_eq!(
        ids.len(),
        84,
        "the suite's case list changed: update README.md"
    );
}

#[test]
fn loopback_round_trip_matches() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let large = payload::json_bytes(ROUNDTRIP_LARGE.target);
        let addr = server::serve(&large).await;
        let http = http_client();
        let base = format!("http://{addr}/v1");
        let s1 = shapes::s1_simple::SimpleApi::new(http.clone())
            .with_base_url(base.clone())
            .unwrap();
        assert_eq!(s1.health().send().await.unwrap().status, "ok");
        let bodies = apis::bodies::Bodies::new(http.clone())
            .with_base_url(base.clone())
            .unwrap();
        let manual = manual::Bodies::new(http, &base);
        let large = bodies.get_json().send().await.unwrap();
        assert_eq!(large, manual.get_json().await.unwrap());
        assert_eq!(large, payload::payload(ROUNDTRIP_LARGE.target));
    });
}
