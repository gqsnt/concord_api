concord::api! {
client Tasks { base "https://a.example"; part keyed(k: String) { headers { "idempotency-key" = k; } } GET list(); }
}
fn main() {}
