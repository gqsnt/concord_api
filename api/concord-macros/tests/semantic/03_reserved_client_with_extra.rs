concord::api! {
    client Workshop(with_extra?: String) { base "https://names.test"; query { with_extra; } GET fetch(); }
}
fn main() {}
