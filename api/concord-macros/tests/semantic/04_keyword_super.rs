concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(super: String) { query { super; } } }
}
fn main() {}
