//! F3 review fixes: a group whose every endpoint is compiled out generates no trait (its value
//! types may not exist), raw identifiers name the same group, and the docs list names.

concord::api! {
    /// Groups under `cfg` and raw spellings.
    pub client Edge() {
        base "http://edge.invalid";

        /// Only for a disabled endpoint: `Missing` is never defined.
        params Hidden(query value?: Missing);
        /// Spelled raw at the inclusion.
        params Paging(query limit?: u32);

        #[cfg(any())]
        GET hidden(..Hidden) { path "hidden"; }
        GET live(..r#Paging) { path "live"; }
    }
}

#[test]
fn a_raw_spelling_includes_the_group() {
    let edge = Edge::new(concord::reqwest::Client::new());
    let live = <EdgeLive as Paging>::set_limit(edge.live(), 3u32)
        .prepare()
        .unwrap();
    assert_eq!(live.request().url().query(), Some("limit=3"));
}
