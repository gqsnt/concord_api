#![forbid(unsafe_code)]
use concord::{Bytes, api};
api! { client NegativeZeroApi {
    base "http://negative-zero.example.test/";
    timeout 5s;
    GET zero() -> Bytes via Binary { path [-0]; }
    GET control() -> Bytes via Binary { path [-1, 2]; }
} }
#[test]
fn s5_baseline_negative_zero_plan() {
    let client = NegativeZeroApi::new().expect("client");
    let actual = client.zero().plan().expect("zero plan");
    assert_eq!(
        client.control().plan().expect("control").observed_url(),
        "http://negative-zero.example.test/-1/2"
    );
    println!("S5_BASELINE_NATIVE_URL={}", actual.observed_url());
    assert_eq!(actual.observed_url(), "http://negative-zero.example.test/0");
}
