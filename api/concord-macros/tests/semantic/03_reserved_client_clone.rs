concord::api! {
    client Workshop(clone?: String) { base "https://names.test"; query { clone; } GET fetch(); }
}
fn main() {}
