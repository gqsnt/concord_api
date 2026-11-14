concord::api! {
    client Workshop(call?: String) { base "https://names.test"; query { call; } GET fetch(); }
}
fn main() {}
