concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { paginate pager; } GET fetch(..one) { paginate pager; } }
}
fn main() {}
