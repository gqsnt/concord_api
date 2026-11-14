concord::api! {
client Tasks { base "https://a.example"; headers { "bad header" = "v"; } GET get(); }
}
fn main() {}
