struct Filter;
impl concord::QueryPairs for Filter {
    fn query_pairs(&self, _: &mut concord::QueryPairsWriter<'_>) {}
}

concord::api! {
    pub client C() {
        base "https://x.test";
        GET list(#[sensitive] query(..) filter: Filter) { path "items"; }
    }
}

fn main() {}
