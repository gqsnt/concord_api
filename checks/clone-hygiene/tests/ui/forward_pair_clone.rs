// F01 negative: the second value (`b`, written `$crate::Payload` like `a`'s) is not `Clone`.
use concord_clone_hygiene_check::forward::Probe;

fn main() {
    let request = Probe::new(concord::reqwest::Client::new())
        .pair(concord_clone_hygiene_a::Payload(1), concord_clone_hygiene_b::Payload(2));
    let _copy = request.clone();
}
