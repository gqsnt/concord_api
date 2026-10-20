#![forbid(unsafe_code)]
//! The canonical §22 Catalog as a first-class example: the §22.3 / §7.3 /
//! §7.4 public signatures (V3-L01), the generated value matrix, the
//! declaration-based Clone rule (V3-L11), and the §22 semantics: `inspect`'s
//! exact replacement versus `item`'s `when_present` fallback, the explicit
//! `"client.tenant"` label and the pinned endpoint IDs (V3-R02).
#![cfg(all(feature = "json", feature = "rate-limit", feature = "default-tls"))]

mod support;

use std::future::IntoFuture;
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::auth::StoreRequest;
use concord::endpoint::{AcceptedBytesRequest, EndpointDescription, RequestPlan, ResponseRequest};
use concord::observation::SanitizedHeaderValue;
use concord::pagination::{CollectAll, CollectPrefix, Pages};
use concord::rate_limit::{
    BoxFuture, Enforcement, LimiterCapability, RateLimitError, RateLimitErrorKind, RateLimitPermit,
    RateLimitRequest, RateLimiter,
};
use concord::{
    ApiError, ByteStream, CallOptions, CancellationToken, ClientBuildError, ControlError,
    ControlSummary, RequestControls, SecretString,
};
use concord_examples::contracts::catalog_v3::catalog_api::regional::{
    ExportRequest, FeedRequest, FindItemRequest, InspectRequest, ItemEndpoint, ItemRequest,
    ItemSummariesRequest, Regional, UpdateItemRequest,
};
use concord_examples::contracts::catalog_v3::catalog_api::{
    AuthenticateRequest, CatalogApiBuilder, Unbound,
};
use concord_examples::contracts::catalog_v3::{
    ApiVersion, CatalogApi, Category, CommandId, Envelope, Item, ItemId, ItemSummary, Language,
    Locale, LoginInput, Region, SessionToken, Tag, TenantId, UpdateInput,
};

fn secret(value: &str) -> SecretString {
    SecretString::try_new(value).expect("bounded fake secret")
}

fn client() -> CatalogApi {
    CatalogApi::builder()
        .set_tenant(support::TENANT_SENTINEL)
        .bootstrap_key(secret(support::BOOTSTRAP_SENTINEL))
        .configure_runtime(|runtime| {
            runtime.rate_limiter(Arc::new(concord::rate_limit::LocalRateLimiter::new()))
        })
        .build()
        .expect("canonical catalog client")
}

fn header(plan: &RequestPlan, name: &str) -> Option<String> {
    plan.public_headers()
        .iter()
        .find(|(header, _)| header.as_str().eq_ignore_ascii_case(name))
        .map(|(_, value)| match value {
            SanitizedHeaderValue::Text(text) => (*text).to_owned(),
            SanitizedHeaderValue::Redacted => "<redacted>".to_owned(),
            SanitizedHeaderValue::NonUtf8 => "<non-utf8>".to_owned(),
        })
}

// ---- §22.3 / §7.3 / §7.4 public signatures (V3-L01) ----

fn assert_output<R, O>()
where
    R: IntoFuture<Output = Result<O, ApiError>> + RequestControls,
{
}

fn assert_clone<T: Clone>() {}
fn assert_send<T: Send>(_: &T) {}

#[test]
fn public_signatures_match_section_22_3() {
    // Builder (§22.3).
    let _: fn(CatalogApiBuilder, TenantId) -> CatalogApiBuilder = CatalogApiBuilder::set_tenant;
    let _: fn(CatalogApiBuilder, &'static str) -> CatalogApiBuilder = CatalogApiBuilder::set_tenant;
    let _: fn(CatalogApiBuilder, Locale) -> CatalogApiBuilder = CatalogApiBuilder::set_locale;
    let _: fn(CatalogApiBuilder, Option<Locale>) -> CatalogApiBuilder =
        CatalogApiBuilder::replace_locale;
    let _: fn(CatalogApiBuilder, Option<Locale>) -> CatalogApiBuilder =
        CatalogApiBuilder::set_locale_if_some;
    let _: fn(CatalogApiBuilder) -> CatalogApiBuilder = CatalogApiBuilder::omit_locale;
    let _: fn(CatalogApiBuilder, ApiVersion) -> CatalogApiBuilder =
        CatalogApiBuilder::set_api_version;
    let _: fn(CatalogApiBuilder) -> CatalogApiBuilder = CatalogApiBuilder::reset_api_version;
    let _: fn(CatalogApiBuilder, SecretString) -> CatalogApiBuilder =
        CatalogApiBuilder::bootstrap_key;

    // Client, credential source and scope navigation (§22.3).
    let _: fn(&CatalogApi, LoginInput) -> AuthenticateRequest = CatalogApi::authenticate;
    let _: fn(AuthenticateRequest) -> StoreRequest = AuthenticateRequest::store_as_session;
    let _: fn(&CatalogApi, Region) -> Regional = CatalogApi::regional;
    let _: fn(&CatalogApi, &'static str) -> Regional = CatalogApi::regional;
    let _: fn(Regional, Language) -> Regional = Regional::set_language;
    let _: fn(Regional, Option<Language>) -> Regional = Regional::replace_language;

    // Requests and their value controls (§22.3).
    let _: fn(&Regional, Vec<Tag>) -> ItemSummariesRequest = Regional::item_summaries;
    let _: fn(ItemSummariesRequest, Category) -> ItemSummariesRequest =
        ItemSummariesRequest::set_category;
    let _: fn(ItemSummariesRequest, Option<Category>) -> ItemSummariesRequest =
        ItemSummariesRequest::replace_category;
    let _: fn(ItemSummariesRequest, Option<NonZeroU32>) -> ItemSummariesRequest =
        ItemSummariesRequest::set_count_if_some;
    let _: fn(ItemSummariesRequest) -> ItemSummariesRequest = ItemSummariesRequest::reset_offset;
    let _: fn(&Regional, ItemId) -> ItemRequest = Regional::item;
    let _: fn(&Regional, ItemId) -> FindItemRequest = Regional::find_item;
    let _: fn(&Regional, ItemId, CommandId, UpdateInput) -> UpdateItemRequest =
        Regional::update_item;
    let _: fn(&Regional, &'static str, &'static str, UpdateInput) -> UpdateItemRequest =
        Regional::update_item;
    let _: fn(&Regional) -> FeedRequest = Regional::feed;
    let _: fn(&Regional) -> ExportRequest = Regional::export;

    // Construction and binding (§7.3).
    let _: fn() -> CatalogApiBuilder = CatalogApi::builder;
    let _: fn(CatalogApiBuilder) -> Result<CatalogApi, ClientBuildError> = CatalogApiBuilder::build;
    let _: fn() -> Unbound = CatalogApi::unbound;
    let _: fn(&CatalogApi, ItemEndpoint) -> ItemRequest = CatalogApi::request::<ItemEndpoint>;
    let _: fn(&CatalogApi, &ItemEndpoint) -> Result<RequestPlan, ApiError> =
        CatalogApi::plan::<ItemEndpoint>;

    // Terminals and output law (§7.4, §10.4, §14.2, §11.4).
    let _: fn(&ItemRequest) -> &'static EndpointDescription = ItemRequest::description;
    let _: fn(&ItemRequest) -> Result<RequestPlan, ApiError> = ItemRequest::plan;
    let _: fn(ItemRequest) -> ResponseRequest<Item> = ItemRequest::response;
    let _: fn(ItemRequest) -> AcceptedBytesRequest = ItemRequest::accepted_bytes;
    let _: fn(ItemSummariesRequest, u32) -> Pages<ItemSummary> = ItemSummariesRequest::pages;
    let _: fn(ItemSummariesRequest, u32) -> CollectPrefix<ItemSummary> =
        ItemSummariesRequest::collect_prefix;
    let _: fn(ItemSummariesRequest, NonZeroU32) -> CollectAll<ItemSummary> =
        ItemSummariesRequest::collect_all;
    let _: fn(FeedRequest, u32) -> Pages<ItemSummary> = FeedRequest::pages;
    assert_output::<ItemRequest, Item>();
    assert_output::<FindItemRequest, Option<Item>>();
    assert_output::<ItemSummariesRequest, Vec<ItemSummary>>();
    assert_output::<FeedRequest, Envelope<ItemSummary>>();
    assert_output::<ExportRequest, ByteStream>();
    assert_output::<InspectRequest, Item>();
    assert_output::<UpdateItemRequest, Item>();
    assert_output::<AuthenticateRequest, SessionToken>();
    assert_output::<StoreRequest, concord::StoreOutcome>();
    assert_output::<ResponseRequest<Item>, concord::ApiResponse<Item>>();
    assert_output::<CollectAll<ItemSummary>, Vec<ItemSummary>>();

    // Request controls (§8.1) are inherent and also the open trait.
    let _: fn(ItemRequest, Duration) -> Result<ItemRequest, ControlError> = ItemRequest::timeout;
    let _: fn(ItemRequest) -> ItemRequest = ItemRequest::reset_timeout;
    let _: fn(ItemRequest, CancellationToken) -> Result<ItemRequest, ControlError> =
        ItemRequest::cancel_with;
    let _: fn(ItemRequest, &CallOptions) -> ItemRequest = ItemRequest::with_options;
    let _: fn(&ItemRequest) -> ControlSummary = ItemRequest::control_summary;

    // Holding a request does not contact the server and its future is Send
    // without borrowing a temporary scope.
    let future = client().regional("eu").item("42").into_future();
    assert_send(&future);
    drop(future);
}

// ---- The generated value matrix and the Clone rule (§4.3, §4.5) ----

/// Marker returned by the fallback methods below: a call resolving to one of
/// them proves that no inherent method of that name was generated (an
/// inherent method takes priority and would not return `Absent`, so a
/// regression fails to compile).
#[derive(Debug, Eq, PartialEq)]
struct Absent;

trait AbsentControls: Sized {
    fn omit_count(self) -> Absent {
        Absent
    }
    fn reset_category(self) -> Absent {
        Absent
    }
    fn set_tags(self, _tags: Vec<String>) -> Absent {
        Absent
    }
    fn omit_offset(self) -> Absent {
        Absent
    }
    fn replace_offset(self, _offset: Option<u64>) -> Absent {
        Absent
    }
    fn set_region(self, _region: String) -> Absent {
        Absent
    }
    fn reset_language(self) -> Absent {
        Absent
    }
    fn omit_tenant(self) -> Absent {
        Absent
    }
    fn reset_locale(self) -> Absent {
        Absent
    }
    fn omit_api_version(self) -> Absent {
        Absent
    }
    fn set_input(self, _input: UpdateInput) -> Absent {
        Absent
    }
    fn set_id(self, _id: String) -> Absent {
        Absent
    }
    fn accepted_bytes(self) -> Absent {
        Absent
    }
    fn pages(self, _maximum_pages: u32) -> Absent {
        Absent
    }
    fn store_as_session(self) -> Absent {
        Absent
    }
}

impl AbsentControls for ItemSummariesRequest {}
impl AbsentControls for Regional {}
impl AbsentControls for CatalogApiBuilder {}
impl AbsentControls for UpdateItemRequest {}
impl AbsentControls for FindItemRequest {}
impl AbsentControls for InspectRequest {}

struct CloneProbe<T>(PhantomData<T>);

trait NotClone {
    fn is_clone(&self) -> bool {
        false
    }
}

impl<T> NotClone for CloneProbe<T> {}

impl<T: Clone> CloneProbe<T> {
    fn is_clone(&self) -> bool {
        true
    }
}

macro_rules! is_clone {
    ($type:ty) => {
        CloneProbe::<$type>(PhantomData).is_clone()
    };
}

#[test]
fn value_matrix_generates_only_permitted_controls() {
    let client = client();
    let regional = client.regional("eu");
    // §22.3: no omit_count, reset_category or set_tags on the plain required
    // tags input; an initialized required input has no omit/replace.
    assert_eq!(regional.item_summaries(Vec::new()).omit_count(), Absent);
    assert_eq!(regional.item_summaries(Vec::new()).reset_category(), Absent);
    assert_eq!(
        regional.item_summaries(Vec::new()).set_tags(Vec::new()),
        Absent
    );
    assert_eq!(regional.item_summaries(Vec::new()).omit_offset(), Absent);
    assert_eq!(
        regional.item_summaries(Vec::new()).replace_offset(None),
        Absent
    );
    // Required positional scope input has no setter; an uninitialized
    // optional has no reset.
    assert_eq!(client.regional("eu").set_region("us".to_owned()), Absent);
    assert_eq!(client.regional("eu").reset_language(), Absent);
    // Root: required value has no omit, uninitialized optional no reset,
    // initialized required no omit.
    assert_eq!(CatalogApi::builder().omit_tenant(), Absent);
    assert_eq!(CatalogApi::builder().reset_locale(), Absent);
    assert_eq!(CatalogApi::builder().omit_api_version(), Absent);
    // A required body and required ordinary inputs have no controls.
    let update = || {
        regional.update_item(
            "1",
            "cmd",
            UpdateInput {
                title: "t".to_owned(),
            },
        )
    };
    assert_eq!(
        update().set_input(UpdateInput {
            title: "u".to_owned()
        }),
        Absent
    );
    assert_eq!(update().set_id("2".to_owned()), Absent);
    // CALL-03: capability methods exist only where declared.
    assert_eq!(regional.find_item("1").accepted_bytes(), Absent);
    assert_eq!(regional.find_item("1").pages(1), Absent);
    assert_eq!(regional.inspect().store_as_session(), Absent);
    assert_eq!(update().pages(1), Absent);

    // V3-L11: body-free requests, intents, scopes and clients are Clone
    // without their outputs being Clone; body-bearing requests are not.
    assert_clone::<CatalogApi>();
    assert_clone::<Regional>();
    assert_clone::<ItemRequest>();
    assert_clone::<FindItemRequest>();
    assert_clone::<ItemSummariesRequest>();
    assert_clone::<FeedRequest>();
    assert_clone::<ExportRequest>();
    assert_clone::<InspectRequest>();
    assert_clone::<ItemEndpoint>();
    assert!(!is_clone!(Item), "outputs need not be Clone");
    assert!(!is_clone!(ByteStream), "streams are never Clone");
    assert!(!is_clone!(SessionToken));
    assert!(is_clone!(ItemRequest));
    assert!(!is_clone!(UpdateItemRequest), "a body slot removes Clone");
    assert!(!is_clone!(AuthenticateRequest), "a body slot removes Clone");
}

// ---- §22 semantics ----

#[test]
fn item_summaries_plan_composes_root_scope_and_endpoint_programs() {
    let client = client();
    let plan = client
        .regional("eu")
        .set_language("fr")
        .item_summaries(vec!["repairable".to_owned(), "used".to_owned()])
        .set_category("hardware")
        .plan()
        .expect("plan");
    // Fixed base prefix, generated labels prepended under the fixed suffix,
    // provider `v2` API version segment, root-then-endpoint query order,
    // repeated tags in order, sensitive tenant withheld from observation.
    assert_eq!(
        plan.observed_url(),
        "https://eu.api.catalog.example.test/root/v2/regions/eu/items?tenant=%3Credacted%3E&category=hardware&offset=0&count=25&tag=repairable&tag=used"
    );
    assert!(!plan.observed_url().contains(support::TENANT_SENTINEL));
    assert_eq!(header(&plan, "x-client-version").as_deref(), Some("3"));
    assert_eq!(header(&plan, "accept-language").as_deref(), Some("fr"));
    assert_eq!(plan.endpoint_id(), "catalog.items.list");
    assert_eq!(plan.resolved_timeout(), Some(Duration::from_secs(20)));

    // An empty present repetition emits zero occurrences; omission removes
    // the category key entirely; replace/reset restore exact states.
    let plan = client
        .regional("eu")
        .item_summaries(Vec::new())
        .set_category("hardware")
        .replace_category(None)
        .set_offset(50)
        .reset_offset()
        .plan()
        .expect("plan");
    assert_eq!(
        plan.observed_url(),
        "https://eu.api.catalog.example.test/root/v2/regions/eu/items?tenant=%3Credacted%3E&offset=0&count=25"
    );
    assert_eq!(header(&plan, "accept-language"), None);
}

#[test]
fn inspect_replacement_differs_from_the_item_when_present_fallback() {
    let with_locale = CatalogApi::builder()
        .set_tenant("acme")
        .set_locale("fr")
        .bootstrap_key(secret(support::BOOTSTRAP_SENTINEL))
        .configure_runtime(|runtime| {
            runtime.rate_limiter(Arc::new(concord::rate_limit::LocalRateLimiter::new()))
        })
        .build()
        .expect("client with root locale");
    let regional = with_locale.regional("eu").set_language("en");

    // `item`: when_present keeps the inherited root locale when the override
    // is absent, and replaces it (at the endpoint position) when present.
    let kept = regional.item("42").plan().expect("plan");
    assert!(
        kept.observed_url()
            .ends_with("/items/42?tenant=%3Credacted%3E&locale=fr"),
        "{}",
        kept.observed_url()
    );
    let replaced = regional
        .item("42")
        .set_locale_override("de")
        .plan()
        .expect("plan");
    assert!(
        replaced
            .observed_url()
            .ends_with("/items/42?tenant=%3Credacted%3E&locale=de"),
        "{}",
        replaced.observed_url()
    );
    assert!(!replaced.observed_url().contains("locale=fr"));
    let explicit_none = regional
        .item("42")
        .replace_locale_override(None)
        .plan()
        .expect("plan");
    assert!(
        explicit_none.observed_url().contains("locale=fr"),
        "absent override keeps the fallback"
    );

    // `inspect`: replace clears the inherited Accept-Language when omitted,
    // and both local query emissions are absent.
    let omitted = regional.inspect().plan().expect("plan");
    assert_eq!(
        header(&omitted, "accept-language"),
        None,
        "replace with absent clears the scope header"
    );
    assert!(
        omitted
            .observed_url()
            .ends_with("/inspect?tenant=%3Credacted%3E&locale=fr"),
        "{}",
        omitted.observed_url()
    );
    assert!(!omitted.observed_url().contains("language"));
    let set = regional.inspect().set_language("de").plan().expect("plan");
    assert_eq!(header(&set, "accept-language").as_deref(), Some("de"));
    assert!(
        set.observed_url()
            .ends_with("/inspect?tenant=%3Credacted%3E&locale=fr&language=de&languageEcho=de"),
        "{}",
        set.observed_url()
    );
    // The scope header is inherited unchanged by endpoints that do not
    // mention it.
    assert_eq!(header(&kept, "accept-language").as_deref(), Some("en"));
}

/// One recorded key part: label, encoding contract name, value bytes as hex.
type RecordedPart = (String, &'static str, String);

/// One recorded policy: id, cost and key parts in declaration order.
type RecordedPolicy = (String, u32, Vec<RecordedPart>);

#[derive(Default)]
struct KeyRecorder {
    requests: Mutex<Vec<Vec<RecordedPolicy>>>,
}

/// Records the generated protected key plan and refuses admission, so the
/// test observes actual generated key bytes without any transport.
struct RecordingLimiter(Arc<KeyRecorder>);

impl RateLimiter for RecordingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord-examples.catalog-recorder", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let policies = request
            .plan
            .policies()
            .iter()
            .map(|policy| {
                let parts = policy
                    .key()
                    .parts()
                    .iter()
                    .map(|part| {
                        let hex = part
                            .value()
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect();
                        (part.name().to_owned(), part.encoding_id(), hex)
                    })
                    .collect();
                (policy.id().as_str().to_owned(), policy.cost().get(), parts)
            })
            .collect();
        self.0.requests.lock().expect("recorder").push(policies);
        Box::pin(async { Err(RateLimitError::new(RateLimitErrorKind::Acquire)) })
    }
}

fn utf8_frame(value: &str) -> String {
    let mut bytes = vec![4_u8];
    bytes.extend_from_slice(b"utf8");
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[tokio::test]
async fn write_policy_keeps_the_client_tenant_label_and_pinned_endpoint_id() {
    // Captured V3 canonical bytes for tenant "tenant-a" in region "eu"
    // (fixtures/contracts/v3_catalog_recording.json).
    const BASELINE_TENANT_A: &str = "0475746638000000000000000874656e616e742d61";
    const BASELINE_ENDPOINT: &str =
        "04757466380000000000000013636174616c6f672e6974656d2e757064617465";
    const BASELINE_ORIGIN: &str = "0475746638000000000000002768747470733a2f2f65752e6170692e636174616c6f672e6578616d706c652e746573743a343433";

    let recorder = Arc::new(KeyRecorder::default());
    let limiter = RecordingLimiter(Arc::clone(&recorder));
    let client = CatalogApi::builder()
        .set_tenant("tenant-a")
        .bootstrap_key(secret(support::BOOTSTRAP_SENTINEL))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()
        .expect("recording client");
    // Credential preflight precedes admission; install a fake session locally.
    client
        .credentials()
        .replace_session(SessionToken::new(secret("session-sentinel")))
        .expect("local session install");

    let error = client
        .regional("eu")
        .update_item(
            "item-1",
            "cmd-1",
            UpdateInput {
                title: "t".to_owned(),
            },
        )
        .await
        .expect_err("the recorder refuses admission");
    assert_eq!(
        error.dispatch_knowledge(),
        concord::DispatchKnowledge::NotDispatched
    );

    let recorded = recorder.requests.lock().expect("recorder").clone();
    assert_eq!(recorded.len(), 1);
    let policies = &recorded[0];
    assert_eq!(policies.len(), 2);
    let (read_id, read_cost, read_key) = &policies[0];
    assert_eq!((read_id.as_str(), *read_cost), ("catalog.tenant.read", 1));
    assert_eq!(
        read_key,
        &vec![
            (
                "origin".to_owned(),
                "concord.utf8/1",
                BASELINE_ORIGIN.to_owned()
            ),
            (
                "tenant".to_owned(),
                "concord.utf8/1",
                BASELINE_TENANT_A.to_owned()
            ),
        ]
    );
    let (write_id, write_cost, write_key) = &policies[1];
    assert_eq!((write_id.as_str(), *write_cost), ("catalog.item.write", 5));
    assert_eq!(
        write_key,
        &vec![
            (
                "origin".to_owned(),
                "concord.utf8/1",
                BASELINE_ORIGIN.to_owned()
            ),
            (
                "endpoint".to_owned(),
                "concord.utf8/1",
                BASELINE_ENDPOINT.to_owned()
            ),
            (
                "client.tenant".to_owned(),
                "concord.utf8/1",
                BASELINE_TENANT_A.to_owned()
            ),
        ]
    );
    assert_eq!(BASELINE_ENDPOINT, utf8_frame("catalog.item.update"));
    assert_eq!(BASELINE_TENANT_A, utf8_frame("tenant-a"));
    // Same value, different labels: the two policies are distinct schemas.
    assert_ne!(read_key[1].0, write_key[2].0);
    assert_eq!(read_key[1].2, write_key[2].2);

    // The value-free plan summary exposes the same labels without values.
    let plan = client
        .regional("eu")
        .update_item(
            "item-1",
            "cmd-1",
            UpdateInput {
                title: "t".to_owned(),
            },
        )
        .plan()
        .expect("plan");
    assert_eq!(plan.rate_limit_policy_count(), 2);
    let summary = format!("{:?}", plan.rate_limit());
    assert!(summary.contains("client.tenant"), "{summary}");
    assert!(summary.contains("catalog.item.write"), "{summary}");
    assert!(
        !summary.contains("tenant-a"),
        "labels only, never values: {summary}"
    );
}

#[test]
fn every_endpoint_identity_is_pinned_and_described() {
    let contract = CatalogApi::contract();
    assert_eq!(contract.endpoints().len(), 8);
    for id in [
        "catalog.auth.login",
        "catalog.items.list",
        "catalog.item.get",
        "catalog.item.find",
        "catalog.item.update",
        "catalog.item.feed",
        "catalog.item.export",
        "catalog.item.inspect",
    ] {
        assert!(contract.endpoint(id).is_some(), "{id} is pinned");
    }
    let unbound = CatalogApi::unbound().regional("eu");
    assert_eq!(
        unbound
            .update_item(
                "1",
                "c",
                UpdateInput {
                    title: "t".to_owned()
                }
            )
            .description()
            .endpoint_id()
            .as_str(),
        "catalog.item.update"
    );
    assert_eq!(
        unbound.feed().description().endpoint_id().as_str(),
        "catalog.item.feed"
    );
    let find = client().regional("eu").find_item("1").plan().expect("plan");
    assert!(
        find.absent_status().is_some(),
        "head-only 404 absence is declared"
    );
}

#[test]
fn unbound_intents_and_direct_requests_plan_identically() {
    let client = client();
    let direct = client
        .regional("eu")
        .set_language("fr")
        .item("9")
        .set_locale_override("it")
        .plan()
        .expect("direct");
    let intent = CatalogApi::unbound()
        .regional("eu")
        .set_language("fr")
        .item("9")
        .set_locale_override("it");
    let planned = client.plan(&intent).expect("unbound plan");
    assert_eq!(direct.observed_url(), planned.observed_url());
    assert_eq!(
        header(&direct, "accept-language"),
        header(&planned, "accept-language")
    );
    let bound = client.request(intent).plan().expect("bound plan");
    assert_eq!(direct.observed_url(), bound.observed_url());
}
