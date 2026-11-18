concord::api! {
pub client Index {
        base "https://index.example.test";
        read Json;

        GET search(q: String, after?: String) -> Hits {
            query { q; after; }
            paginate {
                let mut pager = by_cursor(after, |page| page.next.clone());
                move |call, page| {
                    if page.value().timed_out {
                        return Err(concord::mismatch("a page timed out"));
                    }
                    pager.next_call(call, page)
                }
            };
        }
    }
}
