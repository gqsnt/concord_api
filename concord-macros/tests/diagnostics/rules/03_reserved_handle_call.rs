concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope branch(call?: String) { query { call; } GET fetch(); } }
}
fn main() {}
