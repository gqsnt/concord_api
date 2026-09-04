// Scope-façade value mutation has no legacy bare-method alias.
use concord::api;

api! {
    client ScopeSetterApi {
        timeout 30s;
        base "http://scope-setter.example.test";


    scope control_scope(label: Option<String>) {
        GET list() -> Bytes via Binary;
    }
}}

fn main() {
    let client = ScopeSetterApi::new().unwrap();
    let _ = client.control_scope().label("selected".to_owned());
}
