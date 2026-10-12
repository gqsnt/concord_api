use super::*;
use crate::error::{DispatchKnowledge, FailureStage};
use crate::limits::MAX_REQUEST_URL_BYTES;
use url::Url;

const BASE: &str = "https://query.example.test/";
const ENDPOINT_ID: &str = "s1.private_query";

struct QueryEndpoint {
    left: String,
    right: String,
    placements: Vec<Auth>,
}
impl EndpointSpec for QueryEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = Bytes;
    type Response = Binary;
    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT_ID, Method::GET, Some(Duration::from_secs(30)))
    }
    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("protected")?;
        builder.push_query_value("left", &self.left)?;
        builder.push_query_value("right", &self.right)?;
        for placement in &self.placements {
            builder.push_auth(placement.clone())?;
        }
        builder.finish()
    }
    fn into_body(self) {}
}

fn public_url(length: usize) -> (String, String, Url) {
    let mut url = Url::parse(&format!("{BASE}protected")).unwrap();
    url.query_pairs_mut()
        .append_pair("left", "")
        .append_pair("right", "");
    let payload = length.checked_sub(url.as_str().len()).unwrap();
    let left = "p".repeat(payload.min(65_536));
    let right = "q".repeat(payload - left.len());
    assert!(right.len() <= 65_536);
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("left", &left)
        .append_pair("right", &right);
    assert_eq!(url.as_str().len(), length);
    (left, right, url)
}

async fn measure(
    name: &str,
    public_length: usize,
    secrets: Vec<String>,
    public_reject: bool,
    private_reject: bool,
) {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(AuthHooks::default());
    let core = ClientCore::build_with_development(
        BASE,
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().operation_observer(hooks.clone()),
        &executor,
    )
    .unwrap();
    let (left, right, public) = public_url(public_length);
    let mut native = public.clone();
    let mut selectors = Vec::new();
    let mut versions = Vec::new();
    let mut placements = Vec::new();
    for (index, secret) in secrets.iter().enumerate() {
        let selector =
            SlotSelector::static_value(AccessToken::try_new(secret).unwrap(), Duration::ZERO);
        let version = selector.installed_version().unwrap();
        let key = format!("private{index}");
        native.query_pairs_mut().append_pair(&key, secret);
        placements.push(Auth::query(&key, CredentialRef::new(selector.clone())).unwrap());
        selectors.push(selector);
        versions.push(version);
    }
    if !public_reject && !private_reject {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new().native_url(native.as_str()),
                ScriptedResponse::success(Bytes::from_static(b"ok")),
            )
            .unwrap();
    }
    let result = Request::new(
        core,
        QueryEndpoint {
            left,
            right,
            placements,
        },
    )
    .response()
    .await;
    let count = executor.application().request_count().unwrap();
    let category = result.as_ref().err().map(ApiError::category);
    let stage = result.as_ref().err().map(ApiError::failure_stage);
    println!(
        "S1_QUERY case={name} public_bytes={} final_native_bytes={} placements={} selected_each_raw_bytes={:?} ok={} category={category:?} stage={stage:?} native_handoffs={count}",
        public.as_str().len(),
        native.as_str().len(),
        secrets.len(),
        secrets.iter().map(String::len).collect::<Vec<_>>(),
        result.is_ok()
    );
    if public_reject || private_reject {
        let error = api_error(result);
        assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
        assert_eq!(count, 0);
        if public_reject {
            assert_eq!(error.category(), ErrorCategory::Configuration);
            assert_eq!(error.failure_stage(), FailureStage::Preparation);
        } else {
            assert_eq!(error.auth_kind(), Some(AuthErrorKind::MaterialTooLarge));
            assert_eq!(error.failure_stage(), FailureStage::BeforeDispatch);
            assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
        }
    } else {
        let response = result.expect("exact native URL oracle must succeed");
        assert_eq!(response.value(), &Bytes::from_static(b"ok"));
        assert_eq!(
            response.url().as_str(),
            public.as_str(),
            "credential-sanitized response URL"
        );
        assert_eq!(count, 1);
        assert_eq!(executor.application().remaining_scripts().unwrap(), 0);
    }
    for (selector, version) in selectors.iter().zip(versions) {
        assert_eq!(selector.installed_version(), Some(version));
        assert_eq!(selector.counters_for_test(), (0, 1));
    }
    for capture in executor.application().captures().unwrap() {
        let observed = capture.observed_url().unwrap();
        assert!(!observed.contains("private0="));
        assert!(!format!("{capture:?}").contains("private0="));
    }
}

#[tokio::test]
async fn s1_public_url_exact_and_first_excess_control() {
    measure("public_exact", MAX_REQUEST_URL_BYTES, vec![], false, false).await;
    measure(
        "public_first_excess",
        MAX_REQUEST_URL_BYTES + 1,
        vec![],
        true,
        false,
    )
    .await;
}

#[tokio::test]
async fn s1_private_query_final_url_exact_and_first_excess_are_observed() {
    let secret = "fixture-private-query".to_owned();
    let pair_bytes = "&private0=".len() + secret.len();
    measure(
        "native_exact",
        MAX_REQUEST_URL_BYTES - pair_bytes,
        vec![secret.clone()],
        false,
        false,
    )
    .await;
    measure(
        "native_first_excess",
        MAX_REQUEST_URL_BYTES - pair_bytes + 1,
        vec![secret],
        false,
        false,
    )
    .await;
}

#[tokio::test]
async fn s1_multiple_individually_valid_private_values_exceed_public_url_limit() {
    measure(
        "three_exact_materials",
        64,
        vec!["a".repeat(65_536), "b".repeat(65_536), "c".repeat(65_536)],
        false,
        false,
    )
    .await;
}

#[tokio::test]
async fn s1_private_encoded_value_exact_and_first_excess_control() {
    let exact = format!("{}a", "%".repeat(21_845));
    measure("encoded_exact", 64, vec![exact], false, false).await;
    measure(
        "encoded_first_excess",
        64,
        vec!["%".repeat(21_846)],
        false,
        true,
    )
    .await;
}
