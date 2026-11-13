concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(enum: String) { query { enum; } } }
}
fn main() {}
