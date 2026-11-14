concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope branch(clone?: String) { query { clone; } GET fetch(); } }
}
fn main() {}
