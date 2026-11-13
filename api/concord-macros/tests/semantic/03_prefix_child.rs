concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET with_budget(); }
}
fn main() {}
