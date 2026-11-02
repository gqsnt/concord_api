//! The two `$crate::KEY` contributions keep their own names, values and classification.

use concord::reqwest;
use concord_hygiene_check::Hygiene;

#[test]
fn equal_tokens_from_two_crates_are_two_headers() {
    assert_eq!(concord_hygiene_key_a::KEY.as_str(), "x-public");
    assert_eq!(concord_hygiene_key_b::KEY.as_str(), "authorization");
    let p = Hygiene::new(reqwest::Client::new())
        .get()
        .prepare()
        .unwrap();
    let headers = p.request().headers();
    assert_eq!(headers["x-public"], "PUBLIC");
    assert!(!headers["x-public"].is_sensitive());
    assert_eq!(headers["authorization"], "SECRET");
    assert!(headers["authorization"].is_sensitive());
    assert_eq!(headers.get_all("x-public").iter().count(), 1);
    let shown = format!("{p:?}");
    assert!(!shown.contains("SECRET"), "{shown}");
    assert!(shown.contains("PUBLIC"), "{shown}");
}
