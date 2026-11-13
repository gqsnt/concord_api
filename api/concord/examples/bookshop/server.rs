//! Two helpers over the test kit's `Script`, the server of this example.

use concord_testkit::Script;

/// The next try gets this status and this JSON body.
pub fn answer(script: &Script, status: u16, body: &str) {
    script
        .answer(status)
        .header("content-type", "application/json")
        .body(body.to_owned());
}

/// Prints the requests after the first `seen`, and gives how many there are now.
pub fn wire(script: &Script, seen: usize) -> usize {
    let requests = script.requests();
    for request in requests.iter().skip(seen) {
        let number = request.info().number();
        println!("   > {} {} (try {number})", request.method(), request.url());
    }
    requests.len()
}
