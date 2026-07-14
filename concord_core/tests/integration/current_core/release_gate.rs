use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn release_gate_documents_all_required_invariants() {
    let doc = read_repo_file("dev_doc/release_gate.md");
    for anchor in [
        "just release",
        "formatting",
        "Nextest",
        "doctests",
        "no-default",
        "supply-chain",
        "performance-package",
        "benchmark",
        "unversioned private namespace",
        "transport polymorphism",
        "`Retry-After` resend",
        "Hyper/Tower-family",
    ] {
        assert!(
            doc.contains(anchor),
            "release gate doc should contain final validation anchor `{anchor}`"
        );
    }
}

#[test]
fn examples_cover_current_usage_surface() {
    let examples_main = read_repo_file("concord_examples/tests/main.rs");
    for module in [
        "minimal",
        "auth_session",
        "policy_stack",
        "pagination",
        "custom_pagination",
        "custom_codec",
        "custom_cursor_pagination",
        "endpoint_io",
        "riot_large",
    ] {
        assert!(
            examples_main.contains(module),
            "examples test module `{module}` should be registered"
        );
    }

    let endpoint_io = read_repo_file("concord_examples/src/endpoint_io.rs");
    for anchor in [
        "Text<",
        "Text<String>",
        "Stream<OctetStream>",
        "Multipart<",
        "NoContent",
        "BytesResponse",
        "-> Bytes",
        "bytes::Bytes",
        "execute_stream",
        "execute_stream",
        "StreamBody",
        "MultipartBody",
    ] {
        assert!(
            endpoint_io.contains(anchor),
            "endpoint I/O example should contain `{anchor}`"
        );
    }

    let endpoint_docs = read_repo_file("docs/advanced_endpoints.md");
    for anchor in ["ContentType", "Stream<", "Multipart<", "execute_stream"] {
        assert!(
            endpoint_docs.contains(anchor),
            "advanced endpoint docs should contain `{anchor}`"
        );
    }

    let customization = read_repo_file("docs/customization.md");
    for anchor in ["try_content_type", "try_accept", "NoContent"] {
        assert!(
            customization.contains(anchor),
            "customization docs should contain `{anchor}`"
        );
    }

    let minimal = read_repo_file("concord_examples/tests/integration/minimal.rs");
    assert!(minimal.contains(".execute()"));
    assert!(minimal.contains(".response()"));

    let policy = read_repo_file("concord_examples/tests/integration/policy_stack.rs");
    assert!(policy.contains("rate_limiter"));
    assert!(policy.contains("unrated"));

    let pagination = read_repo_file("concord_examples/tests/integration/pagination.rs");
    assert!(pagination.contains(".paginate("));
    assert!(pagination.contains(".collect()"));

    let explicit = read_repo_file("concord_examples/src/explicit_endpoint.rs");
    assert!(explicit.contains("execute_raw_response"));
}

#[test]
fn final_public_extension_boundary_has_no_private_planning_leaks() {
    let fixture = read_repo_file("concord_core/tests/public_extension.rs");
    assert!(!fixture.contains("concord_core::__private"));
    assert!(!fixture.contains("concord_core::__development"));
    assert!(!fixture.contains("crate::"));
    for anchor in [
        "PreparedEndpoint",
        "PreparedStreamEndpoint",
        "PreparedRequestEntity",
        "CredentialProvider",
        "RequestExecutionMeta",
        "AuthPreparationMode::PerExecution",
    ] {
        assert!(
            fixture.contains(anchor),
            "downstream fixture should use `{anchor}`"
        );
    }

    let public_exports = read_repo_file("concord_core/src/lib.rs");
    let advanced = public_exports
        .split("pub mod advanced")
        .nth(1)
        .expect("advanced module")
        .split("pub mod dangerous")
        .next()
        .expect("advanced module body");
    assert!(!advanced.contains("GeneratedRequest"));
    assert!(!advanced.contains("GeneratedResolvedPolicy"));

    let execution_meta = read_repo_file("concord_core/src/execution_meta.rs");
    assert!(execution_meta.contains("pub struct RequestExecutionMeta"));
    let generated_contract = read_repo_file("concord_core/src/__private/mod.rs");
    assert!(generated_contract.contains("GeneratedPreparedCall"));
    assert!(generated_contract.contains("prepare_generated_endpoint"));
}

#[test]
fn generated_contract_contains_only_opaque_current_preparation() {
    let private = read_repo_file("concord_core/src/__private/mod.rs");
    for runtime_name in [
        "RequestPlan as",
        "RequestPlanView as",
        "EndpointPlan as",
        "ResolvedRoute as",
        "ResolvedPolicy as",
        "crate::policy::Policy as",
        "PolicyLayer as",
        "PolicySnapshot as",
        "PreparedBody as",
        "PreparedRequestEntity as",
        "RequestEntity as",
        "ResponseEntity as",
        "PaginationRuntime as",
        "PaginationRuntimeAdapter as",
        "RateLimitPlan as",
        "AuthPlan as",
        "AuthRequirement as",
    ] {
        assert!(
            !private.contains(runtime_name),
            "runtime alias remained: {runtime_name}"
        );
    }
    for current in [
        "GeneratedApiDescriptor",
        "GeneratedEndpointDescriptor",
        "GeneratedAuthBuilder",
        "GeneratedRateLimitDescriptor",
        "GeneratedPreparedCall",
        "prepare_generated_request_body",
        "prepare_generated_response",
        "prepare_generated_endpoint",
        "create_generated_client",
    ] {
        assert!(
            private.contains(current),
            "missing narrow adapter: {current}"
        );
    }

    let client = read_repo_file("concord_core/src/client/api.rs");
    assert!(!client.contains(&["with_generated_", "retry_mode"].concat()));
    assert!(!private.contains(&["GENERATED_STATUS_", "RETRY_CAPABILITY"].concat()));
    assert!(!private.contains(&["GeneratedStatus", "RetryCapability"].concat()));
}

#[test]
fn development_boundary_is_explicit_narrow_and_not_generated() {
    let lib = read_repo_file("concord_core/src/lib.rs");
    let declaration = lib
        .split("pub mod __development;")
        .next()
        .expect("development declaration")
        .rsplit_once("#[doc(hidden)]")
        .expect("hidden development declaration")
        .0;
    assert!(declaration.ends_with("#[cfg(any(test, feature = \"dangerous-dev-tools\"))]\n"));
    assert!(!lib.contains("cfg(debug_assertions)"));

    let manifest = read_repo_file("concord_core/Cargo.toml");
    let defaults = manifest
        .lines()
        .find(|line| line.starts_with("default ="))
        .expect("core default feature declaration");
    assert!(!defaults.contains("dangerous-dev-tools"));

    let development = read_repo_file("concord_core/src/__development.rs");
    assert!(!development.contains("install_application_executor"));
    assert!(!development.contains("install_provider_executor"));
    for forbidden in [
        "CredentialSlot",
        "AuthApplicationRequest",
        "AuthAppliedCredential",
        "AuthRejectionAction",
        "GeneratedAuthRequirement",
        "DynBody",
        "LimitedBody",
        "ReqwestError",
        "ReqwestErrorKind",
        "TlsCapability",
        "GeneratedResponseEntity",
        "GeneratedRequestEntity",
    ] {
        assert!(
            !development.contains(forbidden),
            "development module exposed `{forbidden}`"
        );
    }
    assert!(!development.contains("-> u64"));
    assert!(!development.contains("pub struct CredentialGenerationSnapshot("));

    let credentials = read_repo_file("concord_core/src/auth/credentials.rs");
    let lifecycle_event = credentials
        .split("pub enum CredentialLifecycleEvent")
        .nth(1)
        .expect("credential lifecycle event")
        .split("enum SlotAction")
        .next()
        .expect("credential lifecycle event body");
    assert!(lifecycle_event.contains("Option<CredentialGenerationSnapshot>"));
    assert!(!lifecycle_event.contains("u64"));
    assert!(credentials.contains("CredentialGenerationSnapshot(<opaque>)"));

    let execute = read_repo_file("concord_core/src/client/execute.rs");
    assert!(!execute.contains("observe_response_released"));
    assert!(!execute.contains("drop(observed)"));
    assert_eq!(
        execute
            .matches("CredentialLifecycleEvent::ResponseReleased")
            .count(),
        1
    );
    assert!(execute.contains("fn release_challenged_response("));

    let context = read_repo_file("concord_core/src/client/context.rs");
    let observation_target = context
        .split("struct AuthLifecycleObservationTarget")
        .nth(1)
        .expect("auth lifecycle observation target")
        .split("impl AuthLifecycleObservationTarget")
        .next()
        .expect("auth lifecycle observation target fields");
    for identity in ["credential_id", "usage_id", "step_id", "target"] {
        assert!(observation_target.contains(identity));
    }

    for root in ["concord_macros/src", "concord_examples"] {
        for source in read_repo_tree(root) {
            assert!(
                !source.contains("concord_core::__development"),
                "normal generated/example source imported __development"
            );
        }
    }
}

#[test]
fn deterministic_native_executor_remains_a_private_feature_gated_reqwest_seam() {
    let lib = read_repo_file("concord_core/src/lib.rs");
    let prelude = lib
        .split("pub mod prelude")
        .nth(1)
        .expect("prelude")
        .split("pub mod advanced")
        .next()
        .expect("prelude body");
    let advanced = lib
        .split("pub mod advanced")
        .nth(1)
        .expect("advanced")
        .split("pub mod dangerous")
        .next()
        .expect("advanced body");
    for exported in [
        "DeterministicNativeExecutor",
        "ScriptedNativeResponse",
        "UnsafeCredentialPlacementExpectations",
        "install_application_executor",
        "install_provider_executor",
    ] {
        assert!(!prelude.contains(exported), "prelude exported {exported}");
        assert!(!advanced.contains(exported), "advanced exported {exported}");
    }

    let manifest = read_repo_file("concord_core/Cargo.toml");
    let defaults = manifest
        .lines()
        .find(|line| line.starts_with("default ="))
        .expect("default features");
    assert!(!defaults.contains("dangerous-dev-tools"));
    for forbidden_dependency in ["hyper =", "hyper-util", "tower ="] {
        assert!(
            !manifest.contains(forbidden_dependency),
            "executor seam added forbidden dependency {forbidden_dependency}"
        );
    }

    let implementation = read_repo_file("concord_core/src/development_executor.rs");
    let capture = implementation
        .split("fn sanitize_capture(")
        .nth(1)
        .expect("sanitized capture")
        .split("fn body_category(")
        .next()
        .expect("sanitized capture body");
    assert!(capture.contains("context.logical_url.clone()"));
    assert!(!capture.contains("request.url()"));
    assert!(!capture.contains("as_bytes()"));
    assert!(!implementation.contains("pub trait"));

    let transport = read_repo_file("concord_core/src/transport.rs");
    assert!(transport.contains("client\n        .execute(request)"));
    assert!(transport.contains("if let Some(executor) = &self.development_executor"));
    assert!(transport.contains("#[cfg(any(test, feature = \"dangerous-dev-tools\"))]"));
    assert_eq!(
        transport.matches("development_executor: Option<").count(),
        2,
        "application and provider must own independent handles"
    );
    assert!(
        transport
            .contains("#[cfg(test)]\n    pub(crate) fn set_application_tls_capability_for_test")
    );
    assert!(
        transport.contains("#[cfg(test)]\n    pub(crate) fn set_provider_tls_capability_for_test")
    );
    assert!(!transport.contains("pub fn set_application_tls_capability"));
    assert!(!transport.contains("pub fn set_provider_tls_capability"));

    let api = read_repo_file("concord_core/src/client/api.rs");
    assert!(!api.contains("pub fn with_executor"));
    assert!(!api.contains("pub fn with_tls_capability"));
    assert!(!api.contains("ApiClient<Cx,"));
    let generated = read_repo_file("concord_core/src/__private/mod.rs");
    assert!(!generated.contains("DeterministicNativeExecutor"));
    assert!(!generated.contains("install_application_executor"));
    assert!(!generated.contains("install_provider_executor"));
    for forbidden in [
        "GeneratedDevelopmentClient",
        "__development_core_client",
        "install_generated_application_executor",
        "install_generated_provider_executor",
    ] {
        assert!(
            !generated.contains(forbidden),
            "generated contract exposed {forbidden}"
        );
    }
    let wrapper = read_repo_file("concord_macros/src/codegen/endpoints/wrapper.rs");
    for forbidden in [
        "GeneratedDevelopmentClient",
        "__development_core_client",
        "DeterministicNativeExecutor",
        "TlsCapability",
    ] {
        assert!(
            !wrapper.contains(forbidden),
            "generated wrapper exposed {forbidden}"
        );
    }
    let support = read_repo_file("concord_test_support/src/deterministic.rs");
    assert!(!support.contains("DeterministicInstallTarget"));
}

fn read_repo_file(path: impl AsRef<Path>) -> String {
    let path = repo_root().join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn read_repo_tree(path: impl AsRef<Path>) -> Vec<String> {
    fn visit(path: &Path, output: &mut Vec<String>) {
        for entry in fs::read_dir(path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
        {
            let path = entry.expect("repository directory entry").path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) != Some("target") {
                    visit(&path, output);
                }
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("rs" | "md")
            ) {
                output.push(
                    fs::read_to_string(&path)
                        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display())),
                );
            }
        }
    }

    let mut output = Vec::new();
    visit(&repo_root().join(path), &mut output);
    output
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("concord_core should have a workspace parent")
        .to_path_buf()
}
