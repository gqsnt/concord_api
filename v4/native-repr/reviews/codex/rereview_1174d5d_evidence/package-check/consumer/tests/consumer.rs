use package_consumer::*;

fn shop() -> Shop {
    Shop::new(concord::reqwest::Client::new(), ShopConfig { token: "Bearer t".into() })
}

#[test]
fn builds_requests_from_the_packaged_sources() {
    let health = shop().health().prepare().unwrap();
    assert_eq!(health.request().url().as_str(), "https://shop.example.test/v1/health");
    assert_eq!(health.request().headers()["accept"], "application/json");
    assert!(health.request().headers()["authorization"].is_sensitive());
    assert!(!format!("{health:?}").contains("Bearer t"));
    let upload = shop().upload(concord::bytes::Bytes::from_static(b"png")).prepare().unwrap();
    assert_eq!(upload.request().headers()["content-type"], "image/png");
}

#[cfg(feature = "json")]
#[test]
fn json_defaults_from_the_packaged_sources() {
    let create = shop().json_api().create(Item { id: 7 }).prepare().unwrap();
    assert_eq!(create.request().headers()["content-type"], "application/json");
    assert_eq!(create.request().body().and_then(|b| b.as_bytes()), Some(&br#"{"id":7}"#[..]));
}
