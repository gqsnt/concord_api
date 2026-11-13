concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(loop: String) { query { loop; } } }
}
fn main() {}
