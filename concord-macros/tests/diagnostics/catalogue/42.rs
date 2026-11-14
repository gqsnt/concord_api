concord::api! {
client Tasks(h: String) { base "https://a.example"; headers { "host" = h; } GET get(); }
}
fn main() {}
