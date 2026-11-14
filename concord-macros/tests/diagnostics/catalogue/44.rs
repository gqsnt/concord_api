concord::api! {
client Tasks { base "https://a.example"; read Json; status { 200 { body } => body } GET get() -> u64 { status { override 2xx => 1 } } }
}
fn main() {}
