concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(into_future?: String) { query { into_future; } } }
}
fn main() {}
