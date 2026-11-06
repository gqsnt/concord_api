// A group's setter exists only on the endpoints that include the group.
concord::api! {
    pub client C() {
        base "https://x.test";
        params Paging(query limit?: u32);
        GET list(..Paging) { path "items"; }
        GET item(id: u64) { path "items/{id}"; }
    }
}

fn main() {
    let c = C::new(concord::reqwest::Client::new());
    let _ = c.list().set_limit(5u32);
    let _ = c.item(7).set_limit(5u32);
}
