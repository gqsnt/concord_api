concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part fragment() { paginate pager; } scope branch(..fragment) { GET fetch(); } }
}
fn main() {}
