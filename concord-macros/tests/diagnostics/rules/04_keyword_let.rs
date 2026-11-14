concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(let: String) { query { let; } } }
}
fn main() {}
