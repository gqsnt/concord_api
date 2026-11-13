concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(with_extra?: String) { query { with_extra; } } }
}
fn main() {}
