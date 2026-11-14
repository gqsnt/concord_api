concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(label: Vec<&str>) { query { label; } } }
}
fn main() {}
