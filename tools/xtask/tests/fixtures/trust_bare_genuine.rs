#[macro_use]
extern crate concord;

api! {
    client GenuineBarePositive {
        base "http://positive.example.test/";
        GET read() -> () via ::concord::Empty { path "bare-positive"; }
    }
}

fn main() {
    let client = GenuineBarePositive::new().expect("valid client");
    let plan = client.read().plan().expect("inert plan");
    assert_eq!(plan.observed_url(), "http://positive.example.test/bare-positive");
    println!("GENUINE_BARE_POSITIVE {}", plan.observed_url());
}
