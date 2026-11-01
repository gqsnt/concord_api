concord::api! {
    pub client Api {
        base "http://x.test";
        GET leading() { path "/items"; }
        GET empty_segment() { path "a//b"; }
        GET dot() { path "a/../b"; }
        GET percent() { path "a%2Fb"; }
        GET space() { path "a b"; }
        GET brace() { path "a/{id"; }
    }
}
fn main() {}
