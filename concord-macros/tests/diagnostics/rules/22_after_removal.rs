concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; query { "mode" = 1; } scope s { query { remove "mode"; } GET fetch() { query { override "mode" = 2; } } } }
}
fn main() {}
