// F01 negative control with direct, differently printed paths.
use concord_clone_hygiene_check::direct::Probe;

fn main() {
    let request = Probe::new(concord::reqwest::Client::new())
        .pair(concord_clone_hygiene_a::Payload(1), concord_clone_hygiene_b::Payload(2));
    let _copy = request.clone();
}
