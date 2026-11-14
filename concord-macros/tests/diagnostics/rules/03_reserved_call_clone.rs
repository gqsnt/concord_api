concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(clone?: String) { query { clone; } } }
}
fn main() {}
