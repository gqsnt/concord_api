concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { override idempotent; } }
}
fn main() {}
