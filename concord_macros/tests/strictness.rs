use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn production_source(path: impl AsRef<Path>) -> String {
    let source = read(path);
    source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(&source)
        .to_string()
}

fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in
            fs::read_dir(&path).unwrap_or_else(|err| panic!("read dir {}: {err}", path.display()))
        {
            let entry = entry.expect("read source dir entry");
            let path = entry.path();
            if path.is_dir() {
                // Macro strictness guards scan production code. Trybuild
                // fixtures and test assertions intentionally contain invalid
                // syntax and expected panic/diagnostic text.
                if path.file_name().is_some_and(|name| name == "tests") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn strict_codegen_has_no_validation_dependent_panic_paths() {
    let root = crate_root();
    let files = rust_files_under(&root.join("src/codegen"));

    let forbidden = [
        "unreachable!()",
        "Option::None::<&str>",
        "expect(\"validated",
        "expect(\"valid retry status\")",
        "expect(\"valid OAuth2ClientCredentials token_url\")",
        "compile_error!(\"paginate auth vars",
        "compile_error!(\"paginate auth vars are not supported",
    ];

    for file in files {
        let source = production_source(&file);
        for needle in forbidden {
            assert!(
                !source.contains(needle),
                "{} must not contain `{needle}`",
                file.display()
            );
        }
    }
}

#[test]
fn strict_public_policy_route_pagination_codegen_do_not_render_auth_value_ir() {
    let root = crate_root();
    for path in [
        "src/codegen/policy/ops.rs",
        "src/codegen/policy/route.rs",
        "src/codegen/policy/route_guards.rs",
        "src/codegen/policy/pagination.rs",
        "src/codegen/endpoints/endpoint.rs",
    ] {
        let source = production_source(root.join(path));
        for needle in ["ValueKind::AuthField", "FmtVarSource::Auth"] {
            assert!(
                !source.contains(needle),
                "{path} must not contain `{needle}`"
            );
        }
    }
}

#[test]
fn strict_policy_optional_refs_are_value_typed() {
    let root = crate_root();
    for path in [
        "src/sema/ir.rs",
        "src/sema/policy.rs",
        "src/codegen/policy/ops.rs",
    ] {
        let source = production_source(root.join(path));
        for needle in ["conditional_on_optional_ref", "OptionalRefKind"] {
            assert!(
                !source.contains(needle),
                "{path} must not contain `{needle}`"
            );
        }
    }

    let ir = production_source(root.join("src/sema/ir.rs"));
    assert!(
        ir.contains("pub enum PolicySetValue"),
        "optional policy refs should be represented in the value type"
    );
    assert!(
        ir.contains("OptionalCxField") && ir.contains("OptionalEpField"),
        "optional vars/ep policy values should be explicit IR variants"
    );
}

#[test]
fn strict_pagination_ir_rejects_client_and_secret_refs() {
    let root = crate_root();
    let sema_policy = production_source(root.join("src/sema/policy.rs"));
    for needle in [
        "contains_auth_field(&value) || emit_helpers::contains_cx_field(&value)",
        "source: FmtVarSource::Cx",
    ] {
        assert!(
            sema_policy.contains(needle),
            "pagination sema should guard `{needle}`"
        );
    }

    let sema_policy_with_tests = read(root.join("src/sema/policy.rs"));
    for needle in [
        "pagination_fmt_rejects_client_vars",
        "pagination_fmt_allows_endpoint_vars",
        "pagination_expr_rejects_nested_client_vars",
        "pagination_expr_rejects_nested_auth_vars",
    ] {
        assert!(
            sema_policy_with_tests.contains(needle),
            "pagination sema unit tests should include `{needle}`"
        );
    }

    let trybuild = read(root.join("tests/trybuild_current.rs"));
    assert!(
        trybuild.contains("fail_vars_in_pagination_expr.rs"),
        "trybuild should cover nested vars.* in pagination expressions"
    );
}

#[test]
fn strict_codegen_does_not_parse_validated_oauth_token_url_with_expect() {
    let root = crate_root();
    for file in rust_files_under(&root.join("src/codegen")) {
        let source = production_source(&file);
        assert!(
            !source.contains("expect(\"valid OAuth2ClientCredentials token_url\")"),
            "{} must not parse OAuth token URLs with validation-dependent expect",
            file.display()
        );
    }
}

#[test]
fn strict_codegen_rate_limit_retry_construction_is_fallible() {
    let root = crate_root();
    let rate_limit = production_source(root.join("src/codegen/policy/rate_limit.rs"));
    let retry = production_source(root.join("src/codegen/policy/retry.rs"));

    for needle in [
        "expect(\"validated non-zero rate limit max\")",
        "expect(\"validated non-zero rate limit cost\")",
        "NonZeroU32::new(#max).expect",
        "NonZeroU32::new(#cost).expect",
    ] {
        assert!(
            !rate_limit.contains(needle),
            "rate-limit codegen must not use `{needle}`"
        );
    }
    assert!(
        rate_limit.contains("ok_or_else") || rate_limit.contains("try_"),
        "rate-limit codegen should construct validated numeric values fallibly"
    );

    assert!(
        !retry.contains("expect(\"valid retry status\")"),
        "retry status codegen must not use validation-dependent expect"
    );
    assert!(
        retry.contains("map_err") || retry.contains("ok_or_else"),
        "retry status codegen should construct statuses fallibly"
    );
}

#[test]
fn strict_raw_ast_only_place_auth_field_may_exist() {
    let root = crate_root();
    for path in ["src/codegen", "src/model"] {
        for file in rust_files_under(&root.join(path)) {
            let source = production_source(&file);
            assert!(
                !source.contains("ValueKind::AuthField") && !source.contains("FmtVarSource::Auth"),
                "{} must not carry auth-field public value IR into codegen/model layers",
                file.display()
            );
        }
    }

    let sema_policy = production_source(root.join("src/sema/policy.rs"));
    assert!(
        sema_policy.contains("ValueKind::AuthField(value) => Err"),
        "sema may mention AuthField only to reject it before public resolved IR"
    );
}
