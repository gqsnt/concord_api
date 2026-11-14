concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { paginate pager; } part two() { paginate pager; } GET fetch(..one,..two); }
}
fn main() {}
