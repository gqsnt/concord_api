concord::api! {
client Tasks(per_second?: u32) { base "https://a.example"; middleware Limit::new(per_second); GET get(); }
}
fn main() {}
