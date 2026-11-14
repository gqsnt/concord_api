concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(trace_id: String) { headers { trace_id; } } }
}
fn main() {}
