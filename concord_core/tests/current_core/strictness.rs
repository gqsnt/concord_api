use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate has workspace parent")
        .to_path_buf()
}

fn workspace_file(path: &str) -> String {
    let workspace = workspace_root();
    fs::read_to_string(workspace.join(path))
        .unwrap_or_else(|err| panic!("read workspace file {path}: {err}"))
}

fn production_source(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let source =
        fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
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
                // Production guards scan source trees only. Tests intentionally use
                // unwrap/expect as assertions and are covered by normal test runs.
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
fn strict_auth_material_never_enters_logical_request_surfaces() {
    let transport = workspace_file("concord_core/src/transport.rs");
    assert!(
        !transport.contains("#[derive(Clone, Debug)]\npub struct BuiltRequest"),
        "BuiltRequest must keep a custom redacted Debug implementation"
    );
    assert!(
        transport.contains("pending_auth_slots"),
        "BuiltRequest debug URL should render pending auth slots structurally"
    );
    assert!(
        transport.contains("RedactedHeaders(&self.headers)"),
        "BuiltRequest/BuiltResponse/DecodedResponse debug must redact headers"
    );

    let auth_plan = workspace_file("concord_core/src/auth/plan.rs");
    assert!(
        auth_plan.contains("push_pending_slot"),
        "auth helpers should attach typed pending auth slots"
    );
    for forbidden in ["request.headers.insert", "request.url.query_pairs_mut"] {
        assert!(
            !auth_plan.contains(forbidden),
            "auth application helpers must not mutate logical request URL/headers with `{forbidden}`"
        );
    }
}

#[test]
fn strict_auth_hooks_do_not_receive_built_request() {
    let context = workspace_file("concord_core/src/client/context.rs");
    assert!(
        context.contains("AuthApplicationRequest<'_>"),
        "ClientContext auth hooks should use the auth-only application request"
    );
    for forbidden in [
        "prepare_auth_requirement<'a>(\n        _requirement: &'a crate::auth::AuthRequirement,\n        _request: &'a mut BuiltRequest",
        "apply_internal_auth<'a>(\n        _requirement: &'a AuthRequirementId,\n        _request: &'a mut BuiltRequest",
        "&'a mut BuiltRequest",
        "&mut BuiltRequest",
    ] {
        assert!(
            !context.contains(forbidden),
            "auth hooks in ClientContext must not receive BuiltRequest"
        );
    }

    let plan = workspace_file("concord_core/src/auth/plan.rs");
    for helper in [
        "pub fn apply_secret_credential<M: crate::auth::SecretCredential>(\n    request: &mut AuthApplicationRequest<'_>",
        "pub fn apply_basic_credential(\n    request: &mut AuthApplicationRequest<'_>",
        "pub fn apply_certificate_credential(\n    request: &mut AuthApplicationRequest<'_>",
    ] {
        assert!(
            plan.contains(helper),
            "auth helper should use AuthApplicationRequest: {helper}"
        );
    }

    let codegen = workspace_file("concord_macros/src/codegen/client.rs");
    assert!(
        codegen.contains("request: &'a mut ::concord_core::advanced::AuthApplicationRequest<'_>"),
        "generated auth preparation should use AuthApplicationRequest"
    );
    assert!(
        !codegen.contains("request: &'a mut ::concord_core::transport::BuiltRequest"),
        "generated auth preparation must not receive BuiltRequest"
    );
}

#[test]
fn strict_no_unknown_host_rate_limit_fallback() {
    let source = workspace_file("concord_core/src/rate_limit/governor_runtime.rs");
    let forbidden = concat!("unknown", "-", "host");
    assert!(
        !source.contains(forbidden),
        "rate-limit keying must not invent unknown host fallback values"
    );
}

#[test]
fn strict_no_semantic_saturating_arithmetic() {
    for path in [
        "concord_core/src/client/execute.rs",
        "concord_core/src/client/auth_http.rs",
        "concord_core/src/auth/credentials.rs",
        "concord_core/src/rate_limit/governor_runtime.rs",
        "concord_macros/src/sema/cache.rs",
    ] {
        let source = workspace_file(path);
        for forbidden in [
            "saturating_add",
            "saturating_mul",
            "saturating_sub",
            "saturating_div",
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} must not use silent saturating arithmetic for semantic runtime/config values"
            );
        }
    }

    let source = workspace_file("concord_core/src/auth/credentials.rs");
    assert!(
        !source.contains("expect(\"credential generation counter overflowed\")"),
        "credential generation overflow must return a typed AuthError, not panic"
    );
}

#[test]
fn strict_no_runtime_lock_panics_in_request_paths() {
    for path in [
        "concord_core/src/client/api.rs",
        "concord_core/src/client/context.rs",
        "concord_core/src/client/execute.rs",
        "concord_core/src/auth/credentials.rs",
        "concord_core/src/cache.rs",
        "concord_core/src/rate_limit/governor_runtime.rs",
        "concord_macros/src/codegen/client.rs",
        "concord_macros/src/codegen/endpoints/wrapper.rs",
        "concord_macros/src/codegen/policy/context.rs",
    ] {
        let source = workspace_file(path);
        for forbidden in [
            ".lock().expect(\"cache index lock\")",
            ".lock().expect(\"rate limit window lock\")",
            ".lock().expect(\"rate limit cooldown lock\")",
            ".read().expect(\"auth_state lock poisoned\")",
            ".read().unwrap",
            ".write().expect(\"auth_state lock poisoned\")",
            ".write().unwrap",
            "host cooldown checked by caller",
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} must not use runtime lock panic pattern `{forbidden}`"
            );
        }
    }
}

#[test]
fn strict_no_unbounded_body_reads() {
    for path in [
        "concord_core/src/client/send_flow.rs",
        "concord_core/src/client/auth_http.rs",
    ] {
        let source = workspace_file(path);
        assert!(
            !source.contains("read_body_all("),
            "{path} must not call an unbounded full-body read helper"
        );
        assert!(
            source.contains("read_body_all_limited("),
            "{path} should route full-body reads through the limited helper"
        );
    }
}

#[test]
fn strict_response_body_limits_are_configured_by_runtime_not_cache() {
    let send_flow = workspace_file("concord_core/src/client/send_flow.rs");
    assert!(
        send_flow.contains("self.runtime_state.max_response_body_bytes()"),
        "endpoint response reads must use RuntimeConfig response limits"
    );
    assert!(
        !send_flow.contains("max_body"),
        "cache max_body must not be reused as the endpoint read/decode limit"
    );

    let auth_http = workspace_file("concord_core/src/client/auth_http.rs");
    assert!(
        auth_http.contains("Some(policy.max_body_bytes)"),
        "auth HTTP reads must use the auth policy body limit"
    );
}

#[test]
fn strict_transport_request_is_the_only_auth_materialization_boundary() {
    let root = workspace_root();
    let core_src = root.join("concord_core/src");
    let mut raw_materialization_sites = Vec::new();

    for file in rust_files_under(&core_src) {
        let source = production_source(&file);
        let contains_raw_materialization = source.contains(".expose()")
            || source.contains("query_pairs_mut().append_pair(name, secret.expose())");
        if contains_raw_materialization {
            raw_materialization_sites.push(
                file.strip_prefix(&root)
                    .unwrap()
                    .display()
                    .to_string()
                    .replace('\\', "/"),
            );
        }
    }

    raw_materialization_sites.sort();
    assert_eq!(
        raw_materialization_sites,
        vec![
            "concord_core/src/auth/materials.rs",
            "concord_core/src/auth/providers.rs",
            "concord_core/src/transport.rs",
        ],
        "raw auth exposure should be limited to material identity/provider construction and TransportRequest materialization"
    );
    let transport = workspace_file("concord_core/src/transport.rs");
    assert!(
        transport.contains("pub(crate) fn materialize_transport_request"),
        "TransportRequest materialization should have one explicit boundary"
    );
}
