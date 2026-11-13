concord::api! { client C { base "https://span.test"; GET get(); } }
fn main() {
    let _ = C::new().with_retry(concord::Retry::<u32>::none());
    let _ = C::new().get().with_retry(concord::Retry::<u32>::none());
}
