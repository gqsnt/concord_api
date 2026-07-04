use super::helpers::{analyze_err, assert_error_contains};

#[test]
fn generated_public_name_collisions_are_rejected() {
    let err = analyze_err(
        r#"
        client Collision {
            base "https://example.com"
            var debug_level: u8
        }

        GET Ping
            path ["ping"]
            -> Json<String>
        "#,
    );

    assert_error_contains(&err, "set_debug_level");
    assert_error_contains(&err, "generated client method");
}
