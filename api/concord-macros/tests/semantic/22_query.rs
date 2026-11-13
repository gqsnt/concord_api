concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { query { override "mode" = 1; } } }
}
fn main() {}
