concord::api! {
client Tasks { base "https://a.example"; read Json; HEAD exists() -> bool { status { 2xx { body } => body } } }
}
fn main() {}
