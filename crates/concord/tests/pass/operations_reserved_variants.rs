concord::api! {
    client Api {
        base "http://127.0.0.1";
        GET self_();
        GET a_l_l();
    }
}
fn main() {
    assert_eq!(__ApiOperation::ALL, &[__ApiOperation::Self_, __ApiOperation::ALL_]);
    assert_eq!(__ApiOperation::from_operation(ApiSelf::operation()), Some(__ApiOperation::Self_));
    let api = Api::new(concord::reqwest::Client::new());
    let _ = api.self_().prepare().unwrap();
    let _ = api.a_l_l().prepare().unwrap();
}
