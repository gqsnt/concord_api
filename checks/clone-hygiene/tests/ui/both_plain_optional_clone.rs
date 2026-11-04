// F01 negative control: neither same-printed payload is `Clone`, both values optional.
use concord_clone_hygiene_check::both_plain::Probe;

fn main() {
    let request = Probe::new(concord::reqwest::Client::new()).optional();
    let _copy = request.clone();
}
