// F01 negative, other order: the query value (`b`) is not `Clone`; the body (`a`) is.
use concord_clone_hygiene_check::reverse::Probe;

fn main() {
    let request = Probe::new(concord::reqwest::Client::new())
        .body(concord_clone_hygiene_b::Payload(1), concord_clone_hygiene_a::Payload(2));
    let _copy = request.clone();
}
