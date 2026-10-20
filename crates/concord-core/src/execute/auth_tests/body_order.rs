//! Body preparation precedes credential work (V3-E05) and head-only absence
//! stays disjoint from authentication challenges (V3-E08).

use super::*;
use crate::endpoint::AbsentStatus;
use crate::response::{ExpectedAbsence, HeadAbsence};

fn rotating_selector(acquires: &Arc<AtomicUsize>) -> Arc<SlotSelector<AccessToken>> {
    SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(acquires),
            refreshes: Arc::new(AtomicUsize::new(0)),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    )
}

#[tokio::test]
async fn v3_e05_invalid_body_length_fails_before_any_provider_acquisition() {
    let executor = DeterministicExecutor::new();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = rotating_selector(&acquires);
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core(&executor, Arc::clone(&hooks));

    // A dishonest structural length is detected during preparation.
    let dishonest = PreparedBody::stream(
        RequestStream::from_bytes(Bytes::from_static(b"four")).exact_length(3),
    );
    let error =
        api_error(request(core.clone(), &selector, ChallengePolicy::Never, dishonest).await);
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(
        error.dispatch_knowledge(),
        crate::error::DispatchKnowledge::NotDispatched
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());

    // A known length above the configured limit fails before credentials.
    let (limited, _) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().request_body_limit(4),
    );
    let error = api_error(
        request(
            limited,
            &selector,
            ChallengePolicy::Never,
            PreparedBody::bytes(Bytes::from_static(b"12345")),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(acquires.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

struct AbsentProtectedEndpoint {
    credential: CredentialRef<AccessToken>,
    challenge: ChallengePolicy,
    absent: StatusCode,
}

impl EndpointSpec for AbsentProtectedEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = Option<Bytes>;
    type Response = HeadAbsence<Binary>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("absent")?;
        builder.push_auth(Auth::bearer(self.credential.clone()).challenge(self.challenge))?;
        builder.finish()
    }

    fn expected_absence(&self) -> Option<ExpectedAbsence<Self::Output, Self::Response>> {
        Some(ExpectedAbsence::new(
            AbsentStatus::codes([self.absent]).expect("absence status"),
        ))
    }

    fn into_body(self) {}
}

#[tokio::test]
async fn v3_e08_absence_overlapping_a_challenge_is_rejected_before_credentials() {
    let executor = DeterministicExecutor::new();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = rotating_selector(&acquires);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let error = api_error(
        Request::new(
            core.clone(),
            AbsentProtectedEndpoint {
                credential: CredentialRef::new(Arc::clone(&selector)),
                challenge: ChallengePolicy::UnauthorizedOrForbidden,
                absent: StatusCode::FORBIDDEN,
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(acquires.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    // A disjoint absence status is resolved after challenge attribution.
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::NOT_FOUND,
            Bytes::from_static(b"irrelevant absent body"),
        ))
        .expect("absent response");
    let value = Request::new(
        core,
        AbsentProtectedEndpoint {
            credential: CredentialRef::new(selector),
            challenge: ChallengePolicy::Unauthorized,
            absent: StatusCode::NOT_FOUND,
        },
    )
    .await
    .expect("declared absence");
    assert_eq!(value, None);
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
}
