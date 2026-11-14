concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; query { "mode" = 1; } GET fetch() { query { "mode" = 2; } } }
}
fn main() {}
