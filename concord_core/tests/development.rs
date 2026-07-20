use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/development_features/Cargo.toml")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should be inside the workspace")
        .to_owned()
}

fn check_fixture(bin: &str, features: Option<&str>) -> Output {
    let mut arguments = vec!["--bin", bin];
    if let Some(features) = features {
        arguments.extend(["--features", features]);
    }
    check_manifest(
        &workspace_root(),
        &format!("development-features/{bin}-{}", features.unwrap_or("base")),
        &fixture_manifest(),
        &arguments,
    )
}

#[test]
fn development_module_is_downstream_accessible_only_with_its_feature() {
    let present = check_fixture("presence", Some("dangerous-dev-tools"));
    assert!(
        present.status.success(),
        "development presence fixture failed\n{}",
        diagnostics(&present)
    );

    let absent = check_fixture("absence", None);
    let absent_diagnostics = diagnostics(&absent);
    assert!(
        !absent.status.success(),
        "development absence fixture unexpectedly compiled\n{absent_diagnostics}"
    );
    assert!(
        absent_diagnostics.contains("__development"),
        "absence diagnostic omitted the gated module\n{absent_diagnostics}"
    );
}

#[test]
fn generated_clients_still_compile_when_development_support_is_enabled() {
    let normal = check_fixture("generated_presence", Some("dangerous-dev-tools"));
    assert!(
        normal.status.success(),
        "normal generated client failed with development tools\n{}",
        diagnostics(&normal)
    );
}

#[cfg(feature = "dangerous-dev-tools")]
mod benchmark_operations {
    use concord_core::__development::{
        BenchmarkOperation, benchmark_public_head, benchmark_ready_credentials,
    };
    use concord_macros::api;

    api! {
        client DevelopmentBenchmarkApi {
            base "https://development-benchmark.example.test/";
            credentials {
                secret application_secret: String;
                credential application_key = bearer(secret.application_secret);
            }
            GET PublicHead(value: String) -> Bytes {
                path ["public", ep.value];
                auth off;
                rate_limit off;
            }
            GET CredentialReady() -> Bytes {
                auth bearer application_key;
                rate_limit off;
            }
        }
    }

    #[tokio::test]
    async fn benchmark_seams_execute_head_preparation_and_ready_credential_finalization() {
        let client = DevelopmentBenchmarkApi::builder()
            .application_secret("development-ready-credential")
            .build()
            .expect("development benchmark client");

        let head = benchmark_public_head(client.public_head("segment".to_owned()))
            .await
            .expect("public head preparation");
        assert_eq!(head.operation(), BenchmarkOperation::PublicHeadPreparation);
        assert!(head.work_units() >= 3);

        let credential = benchmark_ready_credentials(client.credential_ready())
            .await
            .expect("ready credential preparation and finalization");
        assert_eq!(
            credential.operation(),
            BenchmarkOperation::CredentialReadyPreparation
        );
        assert_eq!(credential.work_units(), 1);
    }
}
