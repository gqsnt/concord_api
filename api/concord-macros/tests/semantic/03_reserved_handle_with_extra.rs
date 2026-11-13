concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope branch(with_extra?: String) { query { with_extra; } GET fetch(); } }
}
fn main() {}
