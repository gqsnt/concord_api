concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { body make { override "mode" = 1; } } }
}
fn main() {}
