use super::common::{
    MockResponse, MockTransport, ObservationRuntimeHooks, SafeRecordingDebugSink, TestAuthVars,
    TextEndpoint, auth_policy, client,
};
use super::response_body_limit::ByteBodyEndpoint;
use bytes::Bytes;
use concord_core::advanced::AuthPlacement;
use concord_core::prelude::{ApiClientError, DebugLevel};
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

fn unique_capture_dir(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("test clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "concord-cutover-{name}-{}-{nanos}",
        std::process::id()
    ))
}

fn capture_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut files = std::fs::read_dir(dir)
        .expect("read capture directory")
        .map(|entry| entry.expect("read capture entry").path())
        .collect::<Vec<_>>();
    files.sort();
    files
}

#[tokio::test]
async fn dev_body_capture_disabled_by_default() -> Result<(), ApiClientError> {
    let dir = unique_capture_dir("disabled");
    std::fs::create_dir_all(&dir).expect("create capture directory");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "uncaptured")],
    );
    let client = client(TestAuthVars::default(), transport);

    let response = client.request(TextEndpoint::default()).response().await?;
    assert_eq!(response.value(), "uncaptured");
    assert!(capture_files(&dir).is_empty());
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[cfg(feature = "dangerous-dev-tools")]
#[allow(deprecated)]
#[tokio::test]
async fn dev_body_capture_writes_only_response_to_safe_file() -> Result<(), ApiClientError> {
    const REQUEST: &str = "CAPTURE_REQUEST_SENTINEL_DO_NOT_WRITE";
    const RESPONSE: &str = "CAPTURE_RESPONSE_SENTINEL";
    let dir = unique_capture_dir("enabled");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, RESPONSE)],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.set_debug_sink(Arc::new(SafeRecordingDebugSink::new(events.clone())));
    client.set_debug_level(DebugLevel::VV);
    client.configure(|config| {
        config.dev_body_capture(
            concord_core::dangerous::DevBodyCaptureConfig::response_dir(&dir).max_bytes(1024),
        );
    });

    let response = client
        .request(ByteBodyEndpoint {
            body: Bytes::from_static(REQUEST.as_bytes()),
        })
        .response()
        .await?;
    assert_eq!(response.value(), RESPONSE);

    let files = capture_files(&dir);
    assert_eq!(files.len(), 1);
    let name = files[0]
        .file_name()
        .and_then(|name| name.to_str())
        .expect("capture filename is UTF-8");
    assert!(name.starts_with("ByteBody-POST-200-"));
    assert!(!name.contains(REQUEST));
    assert!(!name.contains(RESPONSE));
    let captured = std::fs::read_to_string(&files[0]).expect("read captured response");
    assert_eq!(captured, RESPONSE);
    assert!(!captured.contains(REQUEST));
    let rendered_events = format!("{:?}", events.lock().await.as_slice());
    assert!(!rendered_events.contains(REQUEST));
    assert!(!rendered_events.contains(RESPONSE));
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[cfg(feature = "dangerous-dev-tools")]
#[allow(deprecated)]
#[tokio::test]
async fn dev_body_capture_skips_oversized_response() -> Result<(), ApiClientError> {
    const RESPONSE: &str = "OVERSIZED_CAPTURE_SENTINEL";
    let dir = unique_capture_dir("oversized");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, RESPONSE)]);
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|config| {
        config.dev_body_capture(
            concord_core::dangerous::DevBodyCaptureConfig::response_dir(&dir).max_bytes(4),
        );
    });

    let response = client.request(TextEndpoint::default()).response().await?;
    assert_eq!(response.value(), RESPONSE);
    assert!(capture_files(&dir).is_empty());
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[cfg(feature = "dangerous-dev-tools")]
#[allow(deprecated)]
#[tokio::test]
async fn dev_body_capture_skips_protected_auth_response() -> Result<(), ApiClientError> {
    const RESPONSE: &str = "PROTECTED_AUTH_RESPONSE_SENTINEL";
    let dir = unique_capture_dir("protected");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, RESPONSE)]);
    let mut client = client(
        TestAuthVars {
            token: Some("secret-token".to_string()),
            identity: "protected",
        },
        transport,
    );
    client.configure(|config| {
        config.dev_body_capture(concord_core::dangerous::DevBodyCaptureConfig::response_dir(
            &dir,
        ));
    });

    let response = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .response()
        .await?;
    assert_eq!(response.value(), RESPONSE);
    assert!(capture_files(&dir).is_empty());
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[cfg(feature = "dangerous-dev-tools")]
#[allow(deprecated)]
#[tokio::test]
async fn dev_body_capture_keeps_hooks_and_debug_body_free() -> Result<(), ApiClientError> {
    const REQUEST: &str = "CAPTURE_HOOK_REQUEST_SENTINEL";
    const RESPONSE: &str = "CAPTURE_HOOK_RESPONSE_SENTINEL";
    let dir = unique_capture_dir("observers");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, RESPONSE)],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    client.set_debug_sink(Arc::new(SafeRecordingDebugSink::new(events.clone())));
    client.set_debug_level(DebugLevel::VV);
    client.configure(|config| {
        config.dev_body_capture(
            concord_core::dangerous::DevBodyCaptureConfig::response_dir(&dir).max_bytes(1024),
        );
    });

    let response = client
        .request(ByteBodyEndpoint {
            body: Bytes::from_static(REQUEST.as_bytes()),
        })
        .response()
        .await?;
    assert_eq!(response.value(), RESPONSE);

    let rendered_events = format!("{:?}", events.lock().await.as_slice());
    assert!(rendered_events.contains("pre_send"));
    assert!(rendered_events.contains("hook_status:200 OK"));
    assert!(!rendered_events.contains(REQUEST));
    assert!(!rendered_events.contains(RESPONSE));
    let files = capture_files(&dir);
    assert_eq!(files.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&files[0]).expect("read captured response"),
        RESPONSE
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
