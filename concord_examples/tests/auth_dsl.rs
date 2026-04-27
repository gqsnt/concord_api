use concord_examples::auth_session::SessionLoginRequest;

#[test]
fn auth_session_request_type_is_public() {
    let req = SessionLoginRequest {
        username: "u".to_string(),
        password: "p".to_string(),
    };
    assert_eq!(req.username, "u");
}
