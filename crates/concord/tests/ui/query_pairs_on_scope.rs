struct Filter;
impl concord::QueryPairs for Filter {
    fn query_pairs(&self, _: &mut concord::QueryPairsWriter<'_>) {}
}

concord::api! {
    pub client C() {
        base "https://x.test";
        scope items(query(..) filter: Filter) {
            GET list() { path "items"; }
        }
    }
}

fn main() {}
