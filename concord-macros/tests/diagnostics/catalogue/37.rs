concord::api! {
client Tasks { base "https://a.example"; part v(limit?: u32) { path "v1"; query { limit; } } scope s(..v) { GET list(); } }
}
fn main() {}
