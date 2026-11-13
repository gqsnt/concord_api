concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(label: Vec<Self>) { query { label; } } }
}
fn main() {}
