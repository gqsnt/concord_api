concord::api! {
    client Workshop(new?: String) { base "https://names.test"; query { new; } GET fetch(); }
}
fn main() {}
