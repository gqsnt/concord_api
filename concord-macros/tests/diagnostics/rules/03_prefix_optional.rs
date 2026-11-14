concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(with_budget?: u8) { query { with_budget; } } }
}
fn main() {}
