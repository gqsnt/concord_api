//! Unit tests for the effective contract description model and builder.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bytes::Bytes;
use http::StatusCode;

use super::*;
use crate::body::{
    BodyDescription, BodyError, PreparedBody, RequestBodyContext, RequestRepresentation,
};
use crate::codec::Text;
use crate::endpoint::{AcceptedStatus, RequestBodyCategory, TimeoutSelection};
use crate::response::{Buffered, ResponseBodyError, ResponseHead, ResponseRepresentation};

const JSON: Option<&str> = Some("application/json");

fn json_result(output: &'static str) -> ResultSpec {
    ResultSpec::new(output, ResponseAcquisition::Buffered, JSON)
}

fn literal_path(text: &'static str, segments: &[&'static str]) -> PathOperation {
    PathOperation::Append(PathTemplate::new(
        text,
        segments
            .iter()
            .map(|segment| PathSegment::literal(segment))
            .collect(),
    ))
}

fn items_id_path() -> PathOperation {
    PathOperation::Append(PathTemplate::new(
        "items/{id}",
        vec![
            PathSegment::literal("items"),
            PathSegment::hole(ValueRef::local("id")),
        ],
    ))
}

const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(3_600);

/// The canonical §22.2 Catalog declaration expressed through the bridge.
fn catalog_builder() -> ContractBuilder {
    ContractDescription::builder(
        OwnerSpec::client("CatalogApi")
            .docs(&["Typed, tenant-scoped Catalog provider client."])
            .timeout(Duration::from_secs(20))
            .request_default(RepresentationSelection::Type("Json"))
            .response_default(RepresentationSelection::Type("Json"))
            .value(
                ValueSpec::new("tenant", "TenantId")
                    .sensitive()
                    .docs(&["The tenant is sent, but excluded from automatic observation."]),
            )
            .value(ValueSpec::new("locale", "Locale").optional())
            .value(ValueSpec::new("api_version", "ApiVersion").trait_default())
            .subdomain(SubdomainOperation::prepend(vec![AtomExpression::string(
                "api",
            )]))
            .path(PathOperation::Append(PathTemplate::new(
                "{api_version}",
                vec![PathSegment::hole(ValueRef::local("api_version"))],
            )))
            .query(KeyOperation::direct(
                "tenant",
                Emission::value(ValueRef::local("tenant")),
            ))
            .query(KeyOperation::direct(
                "locale",
                Emission::value(ValueRef::local("locale")),
            ))
            .header(KeyOperation::introduce(
                "X-Client-Version",
                Emission::literal("3"),
            ))
            .auth(AuthOperation::append(vec![AuthPlacement::header(
                "X-Bootstrap-Key",
                "application_key",
            )])),
        "https://catalog.example.test/root/",
    )
    .secret(SecretDescription::new("bootstrap_key"))
    .credential(CredentialDescription::api_key(
        "application_key",
        "bootstrap_key",
    ))
    .credential(CredentialDescription::endpoint("session", "authenticate"))
    .policy(
        PolicyDefinition::new("tenant_reads", "catalog.tenant.read")
            .formal("tenant")
            .origin()
            .custom("tenant", "tenant")
            .window(120, MINUTE)
            .window(5_000, HOUR),
    )
    .endpoint(
        EndpointSpec::new("CatalogApi::authenticate", "POST")
            .docs(&["Explicit login. Use store_as_session to request local installation."])
            .body(BodySpec::new(
                "input",
                "LoginInput",
                BodyDescription::buffered(Some("application/json")),
            ))
            .result(json_result("SessionToken"))
            .id("catalog.auth.login")
            .path(literal_path("auth/session", &["auth", "session"]))
            .effect(Effect::NonIdempotent),
    )
    .scope(
        OwnerSpec::scope("CatalogApi::regional")
            .value(ValueSpec::new("region", "Region"))
            .value(ValueSpec::new("language", "Language").optional())
            .header(KeyOperation::direct(
                "Accept-Language",
                Emission::value(ValueRef::local("language")),
            ))
            .subdomain(SubdomainOperation::prepend(vec![AtomExpression::value(
                ValueRef::local("region"),
            )]))
            .path(PathOperation::Append(PathTemplate::new(
                "regions/{region}",
                vec![
                    PathSegment::literal("regions"),
                    PathSegment::hole(ValueRef::local("region")),
                ],
            )))
            .auth(AuthOperation::replace(vec![
                AuthPlacement::bearer("session").challenge(ChallengeRule::Unauthorized),
            ]))
            .rate_limit(RateAttachment::named("tenant_reads").bind_encoded(
                "tenant",
                ValueRef::client("tenant"),
                "concord.utf8/1",
            )),
    )
    .endpoint(
        EndpointSpec::new("CatalogApi::regional::item_summaries", "GET")
            .docs(&["One decoded page; select an explicit traversal terminal for more."])
            .value(ValueSpec::new("category", "Category").optional())
            .value(ValueSpec::new("offset", "u64").stable_literal("0"))
            .value(ValueSpec::new("count", "NonZeroU32").stable_constant("DEFAULT_PAGE_SIZE"))
            .value(ValueSpec::new("tags", "Vec<Tag>"))
            .query(KeyOperation::direct(
                "category",
                Emission::value(ValueRef::local("category")),
            ))
            .query(KeyOperation::direct(
                "offset",
                Emission::value(ValueRef::local("offset")),
            ))
            .query(KeyOperation::direct(
                "count",
                Emission::value(ValueRef::local("count")),
            ))
            .query(KeyOperation::direct(
                "tag",
                Emission::each(ValueRef::local("tags")),
            ))
            .result(json_result("Vec<ItemSummary>"))
            .id("catalog.items.list")
            .path(literal_path("items", &["items"]))
            .pagination(
                PaginationContract::new("OffsetPagination", "OffsetInit")
                    .init_value("offset", ValueRef::local("offset"))
                    .init_value("page_size", ValueRef::local("count"))
                    .cursor(ValueRef::local("offset")),
            ),
    )
    .endpoint(
        EndpointSpec::new("CatalogApi::regional::item", "GET")
            .docs(&["A strict lookup. Its explicit byte terminal does not decode Json."])
            .value(ValueSpec::new("id", "ItemId"))
            .value(ValueSpec::new("locale_override", "Locale").optional())
            .result(json_result("Item"))
            .id("catalog.item.get")
            .path(items_id_path())
            .query(KeyOperation::when_present(
                "locale",
                Emission::value(ValueRef::local("locale_override")),
            ))
            .expose_accepted_bytes(),
    )
    .endpoint(
        EndpointSpec::new("CatalogApi::regional::find_item", "GET")
            .docs(&["A head-only 404 is None; the body of that response is not polled."])
            .value(ValueSpec::new("id", "ItemId"))
            .result(json_result("Option<Item>"))
            .id("catalog.item.find")
            .path(items_id_path())
            .absent_on(&[404]),
    )
    .endpoint(
        EndpointSpec::new("CatalogApi::regional::update_item", "PUT")
            .docs(&["The provider asserts idempotence for this command-bearing update."])
            .value(ValueSpec::new("id", "ItemId"))
            .value(ValueSpec::new("command_id", "CommandId").sensitive())
            .header(KeyOperation::direct(
                "Idempotency-Key",
                Emission::value(ValueRef::local("command_id")),
            ))
            .body(BodySpec::new(
                "input",
                "UpdateInput",
                BodyDescription::buffered(Some("application/json")),
            ))
            .result(json_result("Item"))
            .id("catalog.item.update")
            .path(items_id_path())
            .effect(Effect::Idempotent)
            .rate_limit(
                RateAttachment::inline("catalog.item.write")
                    .origin()
                    .endpoint()
                    .custom_encoded(
                        "client.tenant",
                        ValueRef::client("tenant"),
                        None,
                        Some("concord.utf8/1"),
                    )
                    .cost(5)
                    .window(30, MINUTE),
            ),
    )
    .endpoint(
        EndpointSpec::new("CatalogApi::regional::feed", "GET")
            .docs(&["Cursor-envelope page; the controller owns extraction, not Json."])
            .value(ValueSpec::new("after", "String").optional())
            .value(ValueSpec::new("count", "NonZeroU32").stable_constant("DEFAULT_PAGE_SIZE"))
            .query(KeyOperation::direct(
                "after",
                Emission::value(ValueRef::local("after")),
            ))
            .query(KeyOperation::direct(
                "count",
                Emission::value(ValueRef::local("count")),
            ))
            .result(json_result("Envelope<ItemSummary>"))
            .id("catalog.item.feed")
            .path(literal_path("feed", &["feed"]))
            .pagination(
                PaginationContract::new("EnvelopePages", "CursorStart")
                    .init_value("after", ValueRef::local("after"))
                    .cursor(ValueRef::local("after")),
            ),
    )
    .endpoint(
        EndpointSpec::new("CatalogApi::regional::export", "GET")
            .docs(&["Returns a managed byte stream, not a completed export."])
            .result(
                ResultSpec::new("ByteStream", ResponseAcquisition::Streaming, None).via("Binary"),
            )
            .id("catalog.item.export")
            .path(literal_path("export", &["export"])),
    )
    .endpoint(
        EndpointSpec::new("CatalogApi::regional::inspect", "GET")
            .docs(&["One caller-controlled optional is deliberately used three times."])
            .value(ValueSpec::new("language", "Language").optional())
            .query(KeyOperation::direct(
                "language",
                Emission::value(ValueRef::local("language")),
            ))
            .result(json_result("Item"))
            .id("catalog.item.inspect")
            .path(literal_path("inspect", &["inspect"]))
            .header(KeyOperation::replace(
                "Accept-Language",
                Emission::value(ValueRef::local("language")),
            ))
            .query(KeyOperation::introduce(
                "languageEcho",
                Emission::value(ValueRef::local("language")),
            )),
    )
}

fn catalog() -> ContractDescription {
    catalog_builder()
        .build()
        .expect("canonical Catalog contract")
}

static CATALOG: LazyLock<ContractDescription> = LazyLock::new(catalog);

fn build_error(builder: ContractBuilder) -> ContractBuildError {
    builder
        .build()
        .expect_err("the declaration must be rejected")
}

fn minimal(client: OwnerSpec) -> ContractBuilder {
    ContractDescription::builder(
        client.response_default(RepresentationSelection::Type("Json")),
        "https://api.example.test/",
    )
}

fn get(path: &'static str) -> EndpointSpec {
    EndpointSpec::new(path, "GET").result(json_result("Item"))
}

#[test]
fn catalog_contract_builds_with_every_effective_fact() {
    let contract = &*CATALOG;
    assert_eq!(contract.name(), "CatalogApi");
    assert_eq!(contract.base().host(), "catalog.example.test");
    assert_eq!(contract.base().port(), 443);
    assert_eq!(contract.base().path_prefix(), "/root/");
    assert_eq!(contract.endpoints().len(), 8);
    assert_eq!(
        contract
            .owners()
            .iter()
            .map(OwnerDescription::path)
            .collect::<Vec<_>>()[..3],
        [
            "CatalogApi",
            "CatalogApi::authenticate",
            "CatalogApi::regional"
        ]
    );

    let list = contract
        .endpoint("catalog.items.list")
        .expect("list endpoint");
    assert_eq!(list.identity().source(), IdSource::Explicit);
    assert_eq!(list.method(), "GET");
    assert_eq!(
        contract.url_template(list),
        "https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items"
    );
    let labels = list.route().labels();
    assert_eq!(labels[0].origin(), "CatalogApi::regional");
    assert_eq!(labels[1].origin(), "CatalogApi");

    let query = list.operations(HeadFamily::Query);
    let keys = query
        .iter()
        .map(|operation| operation.operation().key())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        ["tenant", "locale", "category", "offset", "count", "tag"]
    );
    assert!(query[0].sensitive());
    assert_eq!(query[0].origin(), "CatalogApi");
    assert_eq!(query[0].conditional_on(), None);
    assert_eq!(
        query[1].conditional_on(),
        Some(ValueRef::exact("CatalogApi", "locale"))
    );
    assert_eq!(query[2].origin(), "CatalogApi::regional::item_summaries");
    assert!(matches!(
        query[5].operation().emission(),
        Some(Emission::Each { value, .. }) if *value == ValueRef::exact("CatalogApi::regional::item_summaries", "tags")
    ));
    let owner = contract
        .owner("CatalogApi::regional::item_summaries")
        .expect("endpoint owner");
    assert_eq!(
        owner.value("offset").map(ValueDescription::initializer),
        Some(ValueInitializer::StableValue(
            InitializerExpression::Literal("0")
        ))
    );
    assert_eq!(
        owner.value("count").map(ValueDescription::initializer),
        Some(ValueInitializer::StableValue(
            InitializerExpression::Constant("DEFAULT_PAGE_SIZE")
        ))
    );
    assert_eq!(
        owner
            .value("tags")
            .and_then(ValueDescription::constructor_position),
        Some(0)
    );

    let auth = list.auth();
    assert_eq!(auth.placements().len(), 1);
    let placement = auth.placements()[0];
    assert_eq!(placement.placement().target(), PlacementTarget::Bearer);
    assert_eq!(placement.placement().credential(), "session");
    assert_eq!(placement.origin(), "CatalogApi::regional");
    assert_eq!(placement.source_kind(), CredentialSourceKind::Endpoint);
    assert_eq!(
        placement.recovery(),
        ChallengeRecovery::FinalRejectsInstalledVersion
    );
    assert!(!placement.automatic_recovery_available());
    assert_eq!(auth.displaced().len(), 1);
    assert_eq!(auth.displaced()[0].origin(), "CatalogApi");
    assert_eq!(auth.displaced()[0].displaced_by(), "CatalogApi::regional");
    assert_eq!(auth.displaced()[0].mode(), AuthResetMode::Replace);

    let policies = list.policies();
    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0].id(), "catalog.tenant.read");
    assert_eq!(policies[0].origin(), "CatalogApi::regional");
    assert_eq!(
        policies[0]
            .dimensions()
            .iter()
            .map(EffectiveDimension::label)
            .collect::<Vec<_>>(),
        ["origin", "tenant"]
    );
    assert_eq!(policies[0].cost(), 1);
    assert_eq!(policies[0].cost_source(), CostSource::Default);
    assert_eq!(
        policies[0].canonical_windows(),
        [
            WindowDescription::new(120, MINUTE),
            WindowDescription::new(5_000, HOUR)
        ]
    );

    let result = list.result();
    assert_eq!(result.output_type(), "Vec<ItemSummary>");
    assert_eq!(result.representation().type_name(), "Json");
    assert_eq!(
        result.representation().source(),
        RepresentationSource::Inherited("CatalogApi")
    );
    let pagination = list.pagination().expect("pagination");
    assert_eq!(pagination.controller(), "OffsetPagination");
    assert_eq!(pagination.init().len(), 2);
    assert_eq!(pagination.init()[1].field(), "page_size");
    assert_eq!(
        pagination.cursor_destinations(),
        [ValueRef::exact(
            "CatalogApi::regional::item_summaries",
            "offset"
        )]
    );
    assert!(list.terminals().pagination());
    assert_eq!(list.timeout().duration(), Duration::from_secs(20));
    assert_eq!(
        list.timeout().source(),
        TimeoutSource::Declared("CatalogApi")
    );

    let coverage = contract.policy("catalog.tenant.read").expect("coverage");
    assert_eq!(coverage.covered().len(), 7);
    assert_eq!(coverage.sharing().len(), 1);
    let condition = coverage.sharing()[0].condition();
    assert!(condition.same_limiter() && condition.same_policy_id() && condition.same_origin());
    assert!(!condition.same_endpoint());
    assert_eq!(condition.labels()[0].label(), "tenant");
    assert!(
        condition.labels()[0]
            .bindings()
            .iter()
            .all(|binding| binding.value() == ValueRef::exact("CatalogApi", "tenant"))
    );

    let write = contract
        .policy("catalog.item.write")
        .expect("write coverage");
    assert_eq!(write.covered().len(), 1);
    assert!(write.sharing()[0].condition().same_endpoint());
    assert_eq!(write.covered()[0].cost(), 5);

    let session = contract.credential("session").expect("session credential");
    assert_eq!(
        session.resolved_source_endpoint(),
        Some("CatalogApi::authenticate")
    );
    let login = contract.endpoint("catalog.auth.login").expect("login");
    assert_eq!(login.terminals().store_as(), ["session"]);
    assert_eq!(
        login.body().map(BodyContract::constructor_position),
        Some(Some(0))
    );
    assert!(contract.diagnostics().is_empty());
}

#[test]
fn inspect_endpoint_uses_one_optional_three_times_and_replaces_inherited_header() {
    let inspect = CATALOG.endpoint("catalog.item.inspect").expect("inspect");
    let headers = inspect.operations(HeadFamily::Header);
    let accept_language = headers
        .iter()
        .find(|operation| operation.operation().key() == "Accept-Language")
        .expect("accept-language");
    assert_eq!(
        accept_language.operation().kind(),
        KeyOperationKind::Replace
    );
    assert_eq!(accept_language.origin(), "CatalogApi::regional::inspect");
    assert_eq!(headers.len(), 2);
    let query = inspect.operations(HeadFamily::Query);
    assert_eq!(
        query
            .iter()
            .map(|operation| operation.operation().key())
            .collect::<Vec<_>>(),
        ["tenant", "locale", "language", "languageEcho"]
    );

    let item = CATALOG.endpoint("catalog.item.get").expect("item");
    let query = item.operations(HeadFamily::Query);
    assert_eq!(query[1].operation().key(), "locale");
    assert_eq!(
        query[1].superseded_when_present(),
        [ValueRef::exact(
            "CatalogApi::regional::item",
            "locale_override"
        )]
    );
    assert_eq!(query[2].operation().kind(), KeyOperationKind::WhenPresent);
    assert!(item.terminals().accepted_bytes());
}

#[test]
fn endpoint_description_derives_runtime_facts_from_contract() {
    let description = EndpointDescription::from_contract(&CATALOG, "catalog.item.find")
        .expect("find description");
    assert_eq!(description.id(), "catalog.item.find");
    assert_eq!(description.method(), &http::Method::GET);
    assert_eq!(description.source_path(), "CatalogApi::regional::find_item");
    assert_eq!(
        description.route_shape(),
        "/{client.api_version}/regions/{regional.region}/items/{endpoint.id}"
    );
    assert_eq!(description.body_category(), RequestBodyCategory::Empty);
    assert_eq!(description.response_accept(), JSON);
    assert_eq!(
        description.response_acquisition(),
        ResponseAcquisition::Buffered
    );
    assert_eq!(
        description.timeout(),
        TimeoutSelection::Explicit(Duration::from_secs(20))
    );
    assert_eq!(description.accepted_status(), &AcceptedStatus::Success);
    assert!(
        description
            .absent_status()
            .is_some_and(|absent| absent.contains(StatusCode::NOT_FOUND))
    );
    assert!(description.capabilities().authentication());
    assert!(description.capabilities().rate_limits());
    assert!(!description.capabilities().pagination());
    let record = description.contract().expect("record");
    assert_eq!(record.path(), "CatalogApi::regional::find_item");
    // V3-O07: the identity borrows the `'static` contract text, so copies in
    // request metadata and plans need no reference-count update; it is equal
    // to the contract's own identity.
    assert!(description.endpoint_id().is_static());
    assert_eq!(description.endpoint_id(), record.identity().id());

    let update = EndpointDescription::from_contract(&CATALOG, "catalog.item.update")
        .expect("update description");
    assert_eq!(update.effect(), Effect::Idempotent);
    assert_eq!(update.effect_source(), EffectSource::Explicit);
    assert_eq!(update.body_specification(), Some("Json"));
    assert_eq!(update.body_category(), RequestBodyCategory::Specification);

    let login = EndpointDescription::from_contract(&CATALOG, "catalog.auth.login").expect("login");
    assert!(login.capabilities().credential_storage());
    let error = EndpointDescription::from_contract(&CATALOG, "catalog.missing")
        .expect_err("unknown endpoint");
    assert_eq!(error.kind(), ContractBuildErrorKind::UnknownEndpoint);
}

static CONVERTER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct CountingResponse;

impl ResponseRepresentation<String> for CountingResponse {
    type Driver = Buffered;
    const ACCEPT: Option<&'static str> = Some("application/vnd.counting+json");

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        CONVERTER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn from_body(_body: Bytes, _head: &ResponseHead<'_>) -> Result<String, ResponseBodyError> {
        CONVERTER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(String::new())
    }
}

struct CountingRequest;

impl RequestRepresentation<String> for CountingRequest {
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(Some("text/counting"));

    fn prepare(
        _input: String,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        CONVERTER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(PreparedBody::empty())
    }
}

#[test]
fn static_accept_and_body_facts_are_read_without_converter_calls() {
    let contract = minimal(OwnerSpec::client("Counting"))
        .endpoint(
            EndpointSpec::new("Counting::submit", "POST")
                .body(
                    BodySpec::from_representation::<CountingRequest, String>("input", "String")
                        .via("CountingRequest"),
                )
                .result(
                    ResultSpec::from_representation::<CountingResponse, String>("String")
                        .via("CountingResponse"),
                ),
        )
        .endpoint(
            EndpointSpec::new("Counting::page", "GET")
                .value(ValueSpec::new("after", "String").optional())
                .query(KeyOperation::direct(
                    "after",
                    Emission::value(ValueRef::local("after")),
                ))
                .body(
                    BodySpec::from_representation::<Text, String>("filter", "String")
                        .via("Text")
                        .page_capable_for::<Text, String>(),
                )
                .result(
                    ResultSpec::from_representation::<CountingResponse, String>("String")
                        .via("CountingResponse"),
                )
                .pagination(
                    PaginationContract::new("Cursor", "CursorInit")
                        .init_value("after", ValueRef::local("after"))
                        .cursor(ValueRef::local("after")),
                ),
        )
        .build()
        .expect("counting contract");
    let submit = contract.endpoint("counting.submit").expect("submit");
    assert_eq!(
        submit.result().accept(),
        Some("application/vnd.counting+json")
    );
    assert_eq!(submit.result().driver(), ResponseAcquisition::Buffered);
    assert_eq!(
        submit.body().map(|body| body.description().media_hint()),
        Some(Some("text/counting"))
    );
    assert_eq!(
        submit.result().representation().shadowed_default(),
        Some(("Counting", "Json"))
    );
    let page = contract.endpoint("counting.page").expect("page");
    assert!(page.body().is_some_and(BodyContract::page_capable));
    assert_eq!(CONVERTER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn when_present_replace_remove_and_family_modes_compose_in_order() {
    let contract = minimal(
        OwnerSpec::client("Shape")
            .value(ValueSpec::new("a", "String"))
            .value(ValueSpec::new("z", "String"))
            .query(KeyOperation::direct(
                "a",
                Emission::value(ValueRef::local("a")),
            ))
            .query(KeyOperation::introduce("locale", Emission::literal("fr")))
            .query(KeyOperation::introduce(
                "z",
                Emission::value(ValueRef::local("z")),
            ))
            .header(KeyOperation::introduce("X-Trace", Emission::literal("on"))),
    )
    .endpoint(
        get("Shape::when_present")
            .value(ValueSpec::new("locale", "String").optional())
            .query(KeyOperation::when_present(
                "locale",
                Emission::value(ValueRef::local("locale")),
            )),
    )
    .endpoint(
        get("Shape::replaced")
            .value(ValueSpec::new("locale", "String").optional())
            .query(KeyOperation::replace(
                "locale",
                Emission::value(ValueRef::local("locale")),
            )),
    )
    .endpoint(get("Shape::removed").query(KeyOperation::remove("locale")))
    .endpoint(
        get("Shape::appended")
            .value(ValueSpec::new("locale", "String").optional())
            .query(KeyOperation::append(
                "locale",
                Emission::value(ValueRef::local("locale")),
            )),
    )
    .endpoint(
        get("Shape::fenced")
            .query_mode(FamilyMode::Replace)
            .header_mode(FamilyMode::Off)
            .value(ValueSpec::new("only", "String"))
            .query(KeyOperation::direct(
                "only",
                Emission::value(ValueRef::local("only")),
            )),
    )
    .build()
    .expect("shape contract");
    let keys = |id: &str| {
        contract
            .endpoint(id)
            .expect("endpoint")
            .operations(HeadFamily::Query)
            .iter()
            .map(|operation| (operation.operation().key(), operation.operation().kind()))
            .collect::<Vec<_>>()
    };
    use KeyOperationKind::{Append, Introduce, Replace, WhenPresent};
    assert_eq!(
        keys("shape.when_present"),
        [
            ("a", Introduce),
            ("locale", Introduce),
            ("z", Introduce),
            ("locale", WhenPresent)
        ]
    );
    assert_eq!(
        keys("shape.replaced"),
        [("a", Introduce), ("z", Introduce), ("locale", Replace)]
    );
    assert_eq!(keys("shape.removed"), [("a", Introduce), ("z", Introduce)]);
    assert_eq!(
        keys("shape.appended"),
        [
            ("a", Introduce),
            ("locale", Introduce),
            ("z", Introduce),
            ("locale", Append)
        ]
    );
    assert_eq!(keys("shape.fenced"), [("only", Introduce)]);
    assert!(
        contract
            .endpoint("shape.fenced")
            .expect("fenced")
            .operations(HeadFamily::Header)
            .is_empty()
    );
    let fallback = &contract
        .endpoint("shape.when_present")
        .expect("wp")
        .operations(HeadFamily::Query)[1];
    assert_eq!(
        fallback.superseded_when_present(),
        [ValueRef::exact("Shape::when_present", "locale")]
    );
}

#[test]
fn key_program_violations_are_rejected_with_structural_subjects() {
    let base = || {
        minimal(
            OwnerSpec::client("Keys")
                .value(ValueSpec::new("locale", "String").optional())
                .value(ValueSpec::new("tenant", "String"))
                .query(KeyOperation::direct(
                    "locale",
                    Emission::value(ValueRef::local("locale")),
                ))
                .query(KeyOperation::direct(
                    "tenant",
                    Emission::value(ValueRef::local("tenant")),
                )),
        )
    };
    let conflict = build_error(base().endpoint(
        get("Keys::x").query(KeyOperation::introduce("locale", Emission::literal("en"))),
    ));
    assert_eq!(conflict.kind(), ContractBuildErrorKind::IntroduceConflict);
    assert_eq!(conflict.owner(), Some("Keys::x"));
    assert_eq!(conflict.subject(), Some("locale"));

    let missing =
        build_error(base().endpoint(
            get("Keys::x").query(KeyOperation::replace("lokale", Emission::literal("en"))),
        ));
    assert_eq!(
        missing.kind(),
        ContractBuildErrorKind::MissingOverrideTarget
    );

    let required = build_error(
        base().endpoint(get("Keys::x").query(KeyOperation::when_present(
            "locale",
            Emission::value(ValueRef::client("tenant")),
        ))),
    );
    assert_eq!(
        required.kind(),
        ContractBuildErrorKind::WhenPresentRequiresOptional
    );

    let off = build_error(
        base().endpoint(
            get("Keys::x")
                .query_mode(FamilyMode::Off)
                .query(KeyOperation::introduce("x", Emission::literal("1"))),
        ),
    );
    assert_eq!(off.kind(), ContractBuildErrorKind::FamilyOffWithOperations);

    let order = build_error(
        base().endpoint(
            get("Keys::x")
                .value(ValueSpec::new("v", "String"))
                .query(KeyOperation::introduce("x", Emission::literal("1")))
                .query(KeyOperation::direct(
                    "v",
                    Emission::value(ValueRef::local("v")),
                )),
        ),
    );
    assert_eq!(order.kind(), ContractBuildErrorKind::InvalidPlacementOrder);

    let header_case = build_error(
        minimal(
            OwnerSpec::client("Keys")
                .header(KeyOperation::introduce("X-Mode", Emission::literal("a"))),
        )
        .endpoint(get("Keys::x").header(KeyOperation::introduce("x-mode", Emission::literal("b")))),
    );
    assert_eq!(
        header_case.kind(),
        ContractBuildErrorKind::IntroduceConflict
    );

    let forbidden = build_error(minimal(OwnerSpec::client("Keys").header(
        KeyOperation::introduce("Content-Type", Emission::literal("x")),
    )));
    assert_eq!(forbidden.kind(), ContractBuildErrorKind::ForbiddenHeader);

    let singleton = build_error(minimal(
        OwnerSpec::client("Keys")
            .header(KeyOperation::append("User-Agent", Emission::literal("x"))),
    ));
    assert_eq!(
        singleton.kind(),
        ContractBuildErrorKind::SingletonHeaderRepetition
    );

    let auth_collision = build_error(
        minimal(
            OwnerSpec::client("Keys")
                .header(KeyOperation::introduce(
                    "X-Key",
                    Emission::literal("public"),
                ))
                .auth(AuthOperation::append(vec![AuthPlacement::header(
                    "x-key", "key",
                )])),
        )
        .secret(SecretDescription::new("raw"))
        .credential(CredentialDescription::api_key("key", "raw"))
        .endpoint(get("Keys::x")),
    );
    assert_eq!(
        auth_collision.kind(),
        ContractBuildErrorKind::ForbiddenHeader
    );
}

/// HEAD-01 / V3-S08 (static contract validation, the generated-code bridge):
/// a public writer claims its target independently of presence, so an
/// outer-optional (absent-capable) source never licenses a body-media,
/// negotiation or transport field, nor an effective private-auth target.
#[test]
fn optional_public_sources_cannot_license_reserved_or_private_targets() {
    let hint = || Emission::value(ValueRef::local("hint"));
    let hints = || Emission::each(ValueRef::local("hints"));
    for (subject, operation) in [
        // Body media.
        ("Content-Type", KeyOperation::direct("Content-Type", hint())),
        (
            "content-length",
            KeyOperation::append("content-length", hint()),
        ),
        // Negotiation.
        ("Accept", KeyOperation::introduce("Accept", hint())),
        (
            "Accept-Encoding",
            KeyOperation::append("Accept-Encoding", hints()),
        ),
        // Transport.
        ("Host", KeyOperation::direct("Host", hint())),
        (
            "Transfer-Encoding",
            KeyOperation::introduce("Transfer-Encoding", hints()),
        ),
    ] {
        let error = build_error(minimal(
            OwnerSpec::client("Reserved")
                .value(ValueSpec::new("hint", "String").optional())
                .value(ValueSpec::new("hints", "Vec<String>").optional())
                .header(operation),
        ));
        assert_eq!(
            error.kind(),
            ContractBuildErrorKind::ForbiddenHeader,
            "{subject}"
        );
        assert_eq!(error.subject(), Some(subject));
    }

    // Private auth: the only public writer is an optional direct placement
    // (header) or an optional appended sequence (query); both collide with
    // the effective placements although their sources may be absent.
    let private = |owner: OwnerSpec| {
        minimal(owner)
            .secret(SecretDescription::new("raw"))
            .credential(CredentialDescription::api_key("key", "raw"))
    };
    let header = build_error(
        private(
            OwnerSpec::client("Private")
                .value(ValueSpec::new("key_hint", "String").optional())
                .header(KeyOperation::direct(
                    "X-Key",
                    Emission::value(ValueRef::local("key_hint")),
                ))
                .auth(AuthOperation::append(vec![AuthPlacement::header(
                    "x-key", "key",
                )])),
        )
        .endpoint(get("Private::x")),
    );
    assert_eq!(header.kind(), ContractBuildErrorKind::ForbiddenHeader);
    let query = build_error(
        private(
            OwnerSpec::client("Private").auth(AuthOperation::append(vec![AuthPlacement::query(
                "api_key", "key",
            )])),
        )
        .endpoint(
            get("Private::x")
                .value(ValueSpec::new("keys", "Vec<String>").optional())
                .query(KeyOperation::append(
                    "api_key",
                    Emission::each(ValueRef::local("keys")),
                )),
        ),
    );
    assert_eq!(query.kind(), ContractBuildErrorKind::ForbiddenHeader);
    assert_eq!(query.subject(), Some("api_key"));
}

#[test]
fn sensitivity_is_sticky_and_sensitive_literals_are_never_retained() {
    let contract = minimal(
        OwnerSpec::client("Secretive")
            .value(
                ValueSpec::new("tenant", "String")
                    .sensitive()
                    .stable_literal("\"tenant-sentinel-81\""),
            )
            .value(ValueSpec::new("mode", "String").stable_literal("\"visible-default\""))
            .query(KeyOperation::direct(
                "tenant",
                Emission::value(ValueRef::local("tenant")),
            ))
            .query(KeyOperation::introduce(
                "mode",
                Emission::value(ValueRef::local("mode")),
            ))
            .header(KeyOperation::introduce(
                "X-Api-Token",
                Emission::literal("literal-sentinel-82"),
            ))
            .header(KeyOperation::introduce("X-Version", Emission::literal("7"))),
    )
    .endpoint(get("Secretive::x").query(KeyOperation::replace(
        "tenant",
        Emission::literal("public-override-83"),
    )))
    .build()
    .expect("secretive contract");
    let client = contract.client();
    assert_eq!(
        client.value("tenant").map(ValueDescription::initializer),
        Some(ValueInitializer::StableValue(
            InitializerExpression::Redacted
        ))
    );
    assert_eq!(
        client.value("mode").map(ValueDescription::initializer),
        Some(ValueInitializer::StableValue(
            InitializerExpression::Literal("\"visible-default\"")
        ))
    );
    let token = &client.local().operations(HeadFamily::Header)[0];
    assert!(matches!(
        token.emission(),
        Some(Emission::Scalar(AtomExpression::Literal {
            text: LiteralText::Redacted,
            ..
        }))
    ));
    let version = &client.local().operations(HeadFamily::Header)[1];
    assert!(matches!(
        version.emission(),
        Some(Emission::Scalar(AtomExpression::Literal {
            text: LiteralText::Text("7"),
            ..
        }))
    ));
    let endpoint = contract.endpoint("secretive.x").expect("endpoint");
    let replaced = endpoint
        .operations(HeadFamily::Query)
        .iter()
        .find(|operation| operation.operation().key() == "tenant")
        .expect("tenant");
    assert!(
        replaced.sensitive(),
        "a public override cannot declassify a key"
    );
    assert!(matches!(
        replaced.operation().emission(),
        Some(Emission::Scalar(AtomExpression::Literal {
            text: LiteralText::Redacted,
            ..
        }))
    ));
    let rendered = format!("{contract:?}");
    for sentinel in [
        "tenant-sentinel-81",
        "literal-sentinel-82",
        "public-override-83",
    ] {
        assert!(
            !rendered.contains(sentinel),
            "{sentinel} leaked into the description"
        );
    }
    let redacted = contract
        .dependencies()
        .iter()
        .filter(|dependency| dependency.kind() == DependencyKind::RedactedLiteral)
        .map(OpaqueDependency::reference)
        .collect::<Vec<_>>();
    assert_eq!(
        redacted,
        [
            "Secretive#headers[0].emission",
            "Secretive#values.tenant.initializer",
            "Secretive::x#query[0].emission"
        ]
    );
}

#[test]
fn route_composition_prepends_labels_appends_segments_and_records_resets() {
    let contract = minimal(
        OwnerSpec::client("Routes")
            .value(ValueSpec::new("version", "String"))
            .subdomain(SubdomainOperation::prepend(vec![AtomExpression::string(
                "api",
            )]))
            .path(PathOperation::Append(PathTemplate::new(
                "{version}",
                vec![PathSegment::hole(ValueRef::local("version"))],
            ))),
    )
    .scope(
        OwnerSpec::scope("Routes::legacy")
            .subdomain(SubdomainOperation::replace(vec![AtomExpression::string(
                "old",
            )]))
            .path(PathOperation::Replace(PathTemplate::new(
                "v0/{client.version}",
                vec![
                    PathSegment::literal("v0"),
                    PathSegment::hole(ValueRef::client("version")),
                ],
            ))),
    )
    .endpoint(get("Routes::legacy::root").path(PathOperation::Off))
    .endpoint(get("Routes::legacy::thing").path(literal_path("thing", &["thing"])))
    .build()
    .expect("routes");
    let root = contract.endpoint("routes.legacy.root").expect("root");
    assert!(root.route().segments().is_empty());
    assert_eq!(
        root.route()
            .path_reset()
            .map(|reset| (reset.owner(), reset.is_off())),
        Some(("Routes::legacy::root", true))
    );
    assert_eq!(contract.url_template(root), "https://old.api.example.test/");
    let thing = contract.endpoint("routes.legacy.thing").expect("thing");
    assert_eq!(
        contract.url_template(thing),
        "https://old.api.example.test/v0/{client.version}/thing"
    );
    assert_eq!(
        thing
            .route()
            .subdomain_reset()
            .map(|reset| (reset.owner(), reset.is_off())),
        Some(("Routes::legacy", false))
    );

    for (segment, kind) in [
        ("a/b", ContractBuildErrorKind::InvalidPathSegment),
        ("..", ContractBuildErrorKind::InvalidPathSegment),
        ("q?x", ContractBuildErrorKind::InvalidPathSegment),
    ] {
        let error = build_error(minimal(OwnerSpec::client("Routes")).endpoint(
            get("Routes::x").path(PathOperation::Append(PathTemplate::new(
                segment,
                vec![PathSegment::literal(segment)],
            ))),
        ));
        assert_eq!(error.kind(), kind);
    }
    let optional_hole = build_error(
        minimal(OwnerSpec::client("Routes")).endpoint(
            get("Routes::x")
                .value(ValueSpec::new("id", "String").optional())
                .path(PathOperation::Append(PathTemplate::new(
                    "{id}",
                    vec![PathSegment::hole(ValueRef::local("id"))],
                ))),
        ),
    );
    assert_eq!(
        optional_hole.kind(),
        ContractBuildErrorKind::NonTotalReference
    );
    let bad_label = build_error(minimal(OwnerSpec::client("Routes").subdomain(
        SubdomainOperation::prepend(vec![AtomExpression::string("Bad.Label")]),
    )));
    assert_eq!(bad_label.kind(), ContractBuildErrorKind::InvalidDnsLabel);
    let ip_base = build_error(
        ContractDescription::builder(
            OwnerSpec::client("Routes")
                .response_default(RepresentationSelection::Type("Json"))
                .subdomain(SubdomainOperation::prepend(vec![AtomExpression::string(
                    "api",
                )])),
            "http://127.0.0.1:8080/",
        )
        .endpoint(get("Routes::x")),
    );
    assert_eq!(ip_base.kind(), ContractBuildErrorKind::InvalidDnsLabel);
    let base = build_error(ContractDescription::builder(
        OwnerSpec::client("Routes"),
        "https://user@host.test/",
    ));
    assert_eq!(base.kind(), ContractBuildErrorKind::InvalidBase);
}

#[test]
fn representation_fences_block_inheritance_and_are_named() {
    let builder = || {
        ContractDescription::builder(
            OwnerSpec::client("Reps").response_default(RepresentationSelection::Type("Json")),
            "https://api.example.test",
        )
        .scope(
            OwnerSpec::scope("Reps::raw").response_default(RepresentationSelection::ExplicitFence),
        )
    };
    let missing =
        build_error(
            builder().endpoint(EndpointSpec::new("Reps::raw::blob", "GET").result(
                ResultSpec::new("Bytes", ResponseAcquisition::Buffered, None),
            )),
        );
    assert_eq!(
        missing.kind(),
        ContractBuildErrorKind::MissingRepresentation
    );
    assert_eq!(missing.owner(), Some("Reps::raw"), "the fence is named");

    let contract =
        builder()
            .endpoint(EndpointSpec::new("Reps::raw::blob", "GET").result(
                ResultSpec::new("Bytes", ResponseAcquisition::Buffered, None).via("Binary"),
            ))
            .build()
            .expect("inline representation");
    let blob = contract.endpoint("reps.raw.blob").expect("blob");
    assert_eq!(
        blob.result().representation().source(),
        RepresentationSource::Inline
    );
    assert_eq!(
        blob.result().representation().blocking_fence(),
        Some("Reps::raw")
    );
    assert_eq!(blob.result().representation().shadowed_default(), None);
    assert_eq!(contract.base().path_prefix(), "/");
}

#[test]
fn auth_composition_displacement_and_recovery_availability() {
    let contract = minimal(OwnerSpec::client("Auth").auth(AuthOperation::append(vec![
        AuthPlacement::bearer("provider").challenge(ChallengeRule::Unauthorized),
    ])))
    .credential(CredentialDescription::custom("provider", "TokenProvider"))
    .endpoint(get("Auth::read"))
    .endpoint(EndpointSpec::new("Auth::write", "POST").result(json_result("Item")))
    .scope(OwnerSpec::scope("Auth::public").auth(AuthOperation::Off))
    .endpoint(get("Auth::public::health"))
    .build()
    .expect("auth contract");
    let read = contract.endpoint("auth.read").expect("read");
    assert!(read.auth().automatic_recovery_available());
    assert_eq!(
        read.auth().placements()[0].recovery(),
        ChallengeRecovery::ProviderRecoveryMode
    );
    let write = contract.endpoint("auth.write").expect("write");
    assert!(
        !write.auth().automatic_recovery_available(),
        "non-idempotent never replays"
    );
    let health = contract.endpoint("auth.public.health").expect("health");
    assert!(health.auth().placements().is_empty());
    assert_eq!(health.auth().displaced()[0].mode(), AuthResetMode::Off);
    assert!(
        contract.diagnostics().iter().any(
            |diagnostic| diagnostic.code() == "C3-AUTH" && diagnostic.owner() == "Auth::public"
        )
    );
    let dependency = contract
        .dependencies()
        .iter()
        .find(|dependency| dependency.kind() == DependencyKind::CredentialProvider)
        .expect("provider dependency");
    assert_eq!(dependency.reference(), "TokenProvider");

    let conflict = build_error(
        minimal(OwnerSpec::client("Auth").auth(AuthOperation::append(vec![
            AuthPlacement::bearer("a"),
            AuthPlacement::basic("b"),
        ])))
        .secret(SecretDescription::new("s1"))
        .secret(SecretDescription::new("s2"))
        .secret(SecretDescription::new("s3"))
        .credential(CredentialDescription::bearer("a", "s1"))
        .credential(CredentialDescription::basic("b", "s2", "s3"))
        .endpoint(get("Auth::x")),
    );
    assert_eq!(
        conflict.kind(),
        ContractBuildErrorKind::AuthPlacementConflict
    );

    let overlap = build_error(
        minimal(OwnerSpec::client("Auth").auth(AuthOperation::append(vec![
            AuthPlacement::bearer("a").challenge(ChallengeRule::UnauthorizedOrForbidden),
        ])))
        .secret(SecretDescription::new("s1"))
        .credential(CredentialDescription::bearer("a", "s1"))
        .endpoint(
            get("Auth::x").status(vec![StatusRange::range(200, 299), StatusRange::single(403)]),
        ),
    );
    assert_eq!(
        overlap.kind(),
        ContractBuildErrorKind::ChallengeStatusOverlap
    );
}

#[test]
fn credential_graph_is_validated() {
    let unknown_secret = build_error(
        minimal(OwnerSpec::client("Creds"))
            .credential(CredentialDescription::api_key("key", "missing"))
            .endpoint(get("Creds::x")),
    );
    assert_eq!(
        unknown_secret.kind(),
        ContractBuildErrorKind::UnknownCredential
    );
    let unknown_endpoint = build_error(
        minimal(OwnerSpec::client("Creds"))
            .credential(CredentialDescription::endpoint("session", "login"))
            .endpoint(get("Creds::x")),
    );
    assert_eq!(
        unknown_endpoint.kind(),
        ContractBuildErrorKind::InvalidCredentialSource
    );
    let cycle = build_error(
        minimal(OwnerSpec::client("Creds"))
            .credential(CredentialDescription::endpoint("session", "login"))
            .endpoint(
                EndpointSpec::new("Creds::login", "POST")
                    .result(json_result("Token"))
                    .auth(AuthOperation::append(vec![AuthPlacement::bearer(
                        "session",
                    )])),
            ),
    );
    assert_eq!(cycle.kind(), ContractBuildErrorKind::CredentialCycle);
    let streaming_source = build_error(
        minimal(OwnerSpec::client("Creds"))
            .credential(CredentialDescription::endpoint("session", "login"))
            .endpoint(
                EndpointSpec::new("Creds::login", "POST").result(ResultSpec::new(
                    "ByteStream",
                    ResponseAcquisition::Streaming,
                    None,
                )),
            ),
    );
    assert_eq!(
        streaming_source.kind(),
        ContractBuildErrorKind::InvalidCredentialSource
    );
}

#[test]
fn rate_policies_deduplicate_conflict_and_report_coverage() {
    let base = || {
        minimal(
            OwnerSpec::client("Rates")
                .value(ValueSpec::new("tenant", "String"))
                .value(ValueSpec::new("other", "String"))
                .value(ValueSpec::new("maybe", "String").optional())
                .query(KeyOperation::direct(
                    "tenant",
                    Emission::value(ValueRef::local("tenant")),
                ))
                .query(KeyOperation::direct(
                    "other",
                    Emission::value(ValueRef::local("other")),
                ))
                .query(KeyOperation::direct(
                    "maybe",
                    Emission::value(ValueRef::local("maybe")),
                )),
        )
        .window_set(WindowSetDescription::new(
            "standard",
            vec![WindowDescription::new(20, Duration::from_secs(10))],
        ))
        .policy(
            PolicyDefinition::new("reads", "rates.read")
                .formal("tenant")
                .origin()
                .custom("tenant", "tenant")
                .using("standard"),
        )
    };
    let contract =
        base()
            .scope(OwnerSpec::scope("Rates::a").rate_limit(
                RateAttachment::named("reads").bind("tenant", ValueRef::client("tenant")),
            ))
            .endpoint(get("Rates::a::x").rate_limit(
                RateAttachment::named("reads").bind("tenant", ValueRef::client("tenant")),
            ))
            .endpoint(get("Rates::a::y"))
            .endpoint(get("Rates::z").rate_limit(
                RateAttachment::named("reads").bind("tenant", ValueRef::client("other")),
            ))
            .build()
            .expect("deduplicated repeated attachment");
    let x = contract.endpoint("rates.a.x").expect("x");
    assert_eq!(
        x.policies().len(),
        1,
        "identical repeated attachment deduplicates"
    );
    assert_eq!(x.policies()[0].window_set(), Some("standard"));
    let coverage = contract.policy("rates.read").expect("coverage");
    assert_eq!(
        coverage
            .covered()
            .iter()
            .map(CoveredEndpoint::path)
            .collect::<Vec<_>>(),
        ["Rates::a::x", "Rates::a::y", "Rates::z"]
    );
    let bindings = coverage.sharing()[0].condition().labels()[0]
        .bindings()
        .iter()
        .map(|binding| (binding.endpoint(), binding.value().name()))
        .collect::<Vec<_>>();
    assert_eq!(
        bindings,
        [
            ("Rates::a::x", "tenant"),
            ("Rates::a::y", "tenant"),
            ("Rates::z", "other")
        ]
    );
    assert_eq!(contract.window_sets()[0].used_by(), ["rates.read"]);

    let conflict = build_error(
        base()
            .scope(OwnerSpec::scope("Rates::a").rate_limit(
                RateAttachment::named("reads").bind("tenant", ValueRef::client("tenant")),
            ))
            .endpoint(
                get("Rates::a::x").rate_limit(
                    RateAttachment::named("reads")
                        .bind("tenant", ValueRef::client("tenant"))
                        .cost(2),
                ),
            ),
    );
    assert_eq!(conflict.kind(), ContractBuildErrorKind::ConflictingPolicy);

    let optional =
        build_error(base().endpoint(
            get("Rates::x").rate_limit(
                RateAttachment::named("reads").bind("tenant", ValueRef::client("maybe")),
            ),
        ));
    assert_eq!(optional.kind(), ContractBuildErrorKind::InvalidRateValue);

    let unbound =
        build_error(base().endpoint(get("Rates::x").rate_limit(RateAttachment::named("reads"))));
    assert_eq!(unbound.kind(), ContractBuildErrorKind::InvalidRateBinding);

    let endpoint_dimension = build_error(
        base().endpoint(
            get("Rates::x").rate_limit(
                RateAttachment::inline("rates.x")
                    .endpoint()
                    .window(10, Duration::from_secs(1)),
            ),
        ),
    );
    assert_eq!(
        endpoint_dimension.kind(),
        ContractBuildErrorKind::EndpointDimensionRequiresExplicitId
    );

    let inconsistent = build_error(
        base()
            .endpoint(
                get("Rates::x").id("rates.x").rate_limit(
                    RateAttachment::inline("rates.shared")
                        .origin()
                        .window(10, Duration::from_secs(1)),
                ),
            )
            .endpoint(
                get("Rates::y").id("rates.y").rate_limit(
                    RateAttachment::inline("rates.shared")
                        .origin()
                        .window(11, Duration::from_secs(1)),
                ),
            ),
    );
    assert_eq!(
        inconsistent.kind(),
        ContractBuildErrorKind::InconsistentPolicySchema
    );

    let unused = base()
        .endpoint(get("Rates::x"))
        .build()
        .expect("unused policy is allowed");
    assert!(unused.policy("rates.read").expect("unused").is_unused());
    assert!(
        unused
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == "C3-RATE" && diagnostic.subject() == "reads")
    );
    assert!(
        unused
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.subject() == "standard")
    );
}

#[test]
fn window_and_policy_definitions_are_validated() {
    let with_policy = |policy: PolicyDefinition| {
        minimal(
            OwnerSpec::client("P")
                .value(ValueSpec::new("t", "String"))
                .query(KeyOperation::direct(
                    "t",
                    Emission::value(ValueRef::local("t")),
                )),
        )
        .policy(policy)
        .endpoint(get("P::x"))
    };
    let cases = [
        (
            PolicyDefinition::new("p", "p.id")
                .origin()
                .window(10, MINUTE)
                .window(20, MINUTE),
            ContractBuildErrorKind::InvalidWindow,
        ),
        (
            PolicyDefinition::new("p", "p.id")
                .origin()
                .window(0, MINUTE),
            ContractBuildErrorKind::InvalidWindow,
        ),
        (
            PolicyDefinition::new("p", "p.id")
                .origin()
                .window(10, MINUTE)
                .using("standard"),
            ContractBuildErrorKind::InvalidWindow,
        ),
        (
            PolicyDefinition::new("p", "p.id").origin(),
            ContractBuildErrorKind::InvalidWindow,
        ),
        (
            PolicyDefinition::new("p", "p.id")
                .origin()
                .window(10, MINUTE)
                .cost(11),
            ContractBuildErrorKind::InvalidCost,
        ),
        (
            PolicyDefinition::new("p", "").origin().window(10, MINUTE),
            ContractBuildErrorKind::InvalidPolicyId,
        ),
        (
            PolicyDefinition::new("p", "p.id")
                .formal("t")
                .origin()
                .window(10, MINUTE),
            ContractBuildErrorKind::UnusedFormal,
        ),
        (
            PolicyDefinition::new("p", "p.id")
                .formal("t")
                .custom("origin", "t")
                .window(10, MINUTE),
            ContractBuildErrorKind::InvalidRateKey,
        ),
        (
            PolicyDefinition::new("p", "p.id")
                .origin()
                .origin()
                .window(10, MINUTE),
            ContractBuildErrorKind::InvalidRateKey,
        ),
        (
            PolicyDefinition::new("p", "p.id").origin().using("missing"),
            ContractBuildErrorKind::InvalidWindowSet,
        ),
    ];
    for (policy, kind) in cases {
        assert_eq!(build_error(with_policy(policy)).kind(), kind);
    }
}

#[test]
fn references_follow_local_and_explicit_owner_rules() {
    let base = || {
        minimal(
            OwnerSpec::client("Refs")
                .value(ValueSpec::new("tenant", "String"))
                .query(KeyOperation::direct(
                    "tenant",
                    Emission::value(ValueRef::local("tenant")),
                )),
        )
        .scope(
            OwnerSpec::scope("Refs::regional")
                .value(ValueSpec::new("region", "String"))
                .path(PathOperation::Append(PathTemplate::new(
                    "{region}",
                    vec![PathSegment::hole(ValueRef::local("region"))],
                ))),
        )
    };
    let unbound = build_error(base().endpoint(get("Refs::regional::x").header(
        KeyOperation::introduce("X-T", Emission::value(ValueRef::local("tenant"))),
    )));
    assert_eq!(
        unbound.kind(),
        ContractBuildErrorKind::UnboundReference,
        "a bare name never searches ancestors"
    );
    let redundant = build_error(minimal(
        OwnerSpec::client("Refs")
            .value(ValueSpec::new("tenant", "String"))
            .query(KeyOperation::direct(
                "tenant",
                Emission::value(ValueRef::client("tenant")),
            )),
    ));
    assert_eq!(
        redundant.kind(),
        ContractBuildErrorKind::RedundantQualification
    );
    let contract = base()
        .endpoint(
            get("Refs::regional::x")
                .header(KeyOperation::introduce(
                    "X-T",
                    Emission::value(ValueRef::client("tenant")),
                ))
                .header(KeyOperation::introduce(
                    "X-R",
                    Emission::value(ValueRef::scope("regional", "region")),
                )),
        )
        .build()
        .expect("qualified references");
    let headers = contract
        .endpoint("refs.regional.x")
        .expect("x")
        .operations(HeadFamily::Header);
    assert_eq!(
        headers[0]
            .operation()
            .emission()
            .and_then(Emission::presence_source),
        Some(&ValueRef::exact("Refs", "tenant"))
    );
    assert_eq!(
        headers[1]
            .operation()
            .emission()
            .and_then(Emission::presence_source),
        Some(&ValueRef::exact("Refs::regional", "region"))
    );
    assert_eq!(
        contract.display_ref(&ValueRef::exact("Refs::regional", "region")),
        "regional.region"
    );
    let dead = build_error(minimal(
        OwnerSpec::client("Refs").value(ValueSpec::new("unused", "String")),
    ));
    assert_eq!(dead.kind(), ContractBuildErrorKind::DeadValue);
    let no_op = minimal(
        OwnerSpec::client("Refs").value(
            ValueSpec::new("legacy", "String")
                .optional()
                .deprecated_no_op()
                .deprecated(Deprecation::new().since("3.0.0").note("no longer sent")),
        ),
    )
    .build()
    .expect("deprecated no-op bridge");
    assert!(
        no_op
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == "C3-PLACE" && diagnostic.subject() == "legacy")
    );
    let dead_required = build_error(
        minimal(OwnerSpec::client("Refs")).endpoint(
            get("Refs::x").value(
                ValueSpec::new("id", "String")
                    .deprecated_no_op()
                    .deprecated(Deprecation::new()),
            ),
        ),
    );
    assert_eq!(
        dead_required.kind(),
        ContractBuildErrorKind::InvalidNoOpValue
    );
}

#[test]
fn value_roles_controls_and_constructor_order_follow_the_matrix() {
    let update = CATALOG
        .owner("CatalogApi::regional::update_item")
        .expect("update");
    assert_eq!(
        update
            .value("id")
            .and_then(ValueDescription::constructor_position),
        Some(0)
    );
    assert_eq!(
        update
            .value("command_id")
            .and_then(ValueDescription::constructor_position),
        Some(1)
    );
    let body = CATALOG
        .endpoint("catalog.item.update")
        .and_then(EndpointContract::body)
        .expect("body");
    assert_eq!(body.constructor_position(), Some(2));
    assert_eq!(body.declaration_index(), 2);
    let root = CATALOG.client();
    assert_eq!(
        root.value("tenant").map(ValueDescription::role),
        Some(ValueRole::BuilderRequired)
    );
    assert_eq!(
        root.value("tenant").map(ValueDescription::controls),
        Some(&[ValueControl::Set, ValueControl::SetIfSome][..])
    );
    assert_eq!(
        root.value("locale").map(ValueDescription::controls),
        Some(
            &[
                ValueControl::Set,
                ValueControl::Replace,
                ValueControl::SetIfSome,
                ValueControl::Omit
            ][..]
        )
    );
    assert_eq!(
        root.value("api_version").map(ValueDescription::controls),
        Some(
            &[
                ValueControl::Set,
                ValueControl::SetIfSome,
                ValueControl::Reset
            ][..]
        )
    );
    let list = CATALOG
        .owner("CatalogApi::regional::item_summaries")
        .expect("list");
    assert_eq!(
        list.value("tags").map(ValueDescription::controls),
        Some(&[][..])
    );
    assert_eq!(
        list.value("tags").map(ValueDescription::role),
        Some(ValueRole::Constructor)
    );
}

#[test]
fn endpoint_identity_and_status_rules_are_enforced() {
    let contract = minimal(OwnerSpec::client("HTTPServerApi"))
        .scope(OwnerSpec::scope("HTTPServerApi::match_v5"))
        .endpoint(get("HTTPServerApi::match_v5::by_id"))
        .build()
        .expect("derived id");
    let endpoint = &contract.endpoints()[0];
    assert_eq!(endpoint.id(), "http_server_api.match_v5.by_id");
    assert_eq!(endpoint.identity().source(), IdSource::Derived);
    assert_eq!(
        endpoint
            .identity()
            .derivation_inputs()
            .iter()
            .map(|input| &**input)
            .collect::<Vec<_>>(),
        ["http_server_api", "match_v5", "by_id"]
    );
    let invalid = build_error(minimal(OwnerSpec::client("Api")).endpoint(get("Api::Upper")));
    assert_eq!(
        invalid.kind(),
        ContractBuildErrorKind::InvalidDerivedEndpointId
    );
    let duplicate = build_error(
        minimal(OwnerSpec::client("Api"))
            .endpoint(get("Api::a").id("api.same"))
            .endpoint(get("Api::b").id("api.same")),
    );
    assert_eq!(
        duplicate.kind(),
        ContractBuildErrorKind::DuplicateEndpointId
    );
    for (endpoint, kind) in [
        (
            get("Api::x").status(vec![StatusRange::single(101)]),
            ContractBuildErrorKind::InvalidStatus,
        ),
        (
            get("Api::x").status(vec![StatusRange::range(200, 210), StatusRange::single(205)]),
            ContractBuildErrorKind::InvalidStatus,
        ),
        (
            get("Api::x").absent_on(&[204]),
            ContractBuildErrorKind::InvalidStatus,
        ),
        (
            get("Api::x").absent_on(&[404]).expose_accepted_bytes(),
            ContractBuildErrorKind::InvalidAbsence,
        ),
        (
            EndpointSpec::new("Api::x", "GET")
                .result(ResultSpec::new(
                    "ByteStream",
                    ResponseAcquisition::Streaming,
                    None,
                ))
                .expose_accepted_bytes(),
            ContractBuildErrorKind::InvalidAcceptedBytes,
        ),
        (
            EndpointSpec::new("Api::x", "TRACE").result(json_result("Item")),
            ContractBuildErrorKind::InvalidMethod,
        ),
        (
            EndpointSpec::new("Api::x", "GET"),
            ContractBuildErrorKind::MissingResult,
        ),
        (
            get("Api::x").id("Bad.ID"),
            ContractBuildErrorKind::InvalidEndpointId,
        ),
        (
            get("Api::x").id("a").id("b"),
            ContractBuildErrorKind::DuplicateClause,
        ),
        (
            EndpointSpec::new("Api::x", "POST")
                .result(json_result("Item"))
                .pagination(PaginationContract::new("C", "I")),
            ContractBuildErrorKind::InvalidPagination,
        ),
    ] {
        assert_eq!(
            build_error(minimal(OwnerSpec::client("Api")).endpoint(endpoint)).kind(),
            kind
        );
    }
    let parentless =
        build_error(minimal(OwnerSpec::client("Api")).endpoint(get("Api::missing::x")));
    assert_eq!(parentless.kind(), ContractBuildErrorKind::InvalidParent);
    let reserved =
        build_error(minimal(OwnerSpec::client("Api")).scope(OwnerSpec::scope("Api::client")));
    assert_eq!(reserved.kind(), ContractBuildErrorKind::InvalidScopeName);
}

#[test]
fn dependencies_and_diagnostics_are_sorted_and_complete() {
    let kinds = CATALOG
        .dependencies()
        .iter()
        .map(|dependency| (dependency.kind(), dependency.reference()))
        .collect::<Vec<_>>();
    let mut sorted = kinds.clone();
    sorted.sort();
    assert_eq!(kinds, sorted);
    for expected in [
        (DependencyKind::PayloadType, "TenantId"),
        (DependencyKind::Constant, "DEFAULT_PAGE_SIZE"),
        (DependencyKind::PaginationController, "OffsetPagination"),
        (DependencyKind::PaginationInit, "OffsetInit"),
        (DependencyKind::Representation, "Json"),
        (DependencyKind::Representation, "Binary"),
        (DependencyKind::OutputType, "Vec<ItemSummary>"),
        (DependencyKind::BodyInputType, "UpdateInput"),
    ] {
        assert!(kinds.contains(&expected), "missing {expected:?}");
    }
    let constant = CATALOG
        .dependencies()
        .iter()
        .find(|dependency| dependency.reference() == "DEFAULT_PAGE_SIZE")
        .expect("constant");
    assert_eq!(
        constant
            .sites()
            .iter()
            .map(|site| &**site)
            .collect::<Vec<_>>(),
        [
            "CatalogApi::regional::feed#values.count.initializer",
            "CatalogApi::regional::item_summaries#values.count.initializer"
        ]
    );
    assert_eq!(
        catalog().dependencies(),
        CATALOG.dependencies(),
        "builds are deterministic"
    );
}

#[test]
fn owner_views_describe_scopes_and_runtime_binding_contexts() {
    let regional = CATALOG
        .owner("CatalogApi::regional")
        .and_then(OwnerDescription::effective)
        .expect("regional view");
    assert_eq!(
        CATALOG.route_template(regional.route()),
        "https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}"
    );
    assert_eq!(
        regional.auth().placements()[0].placement().credential(),
        "session"
    );
    assert_eq!(regional.auth().displaced().len(), 1);
    assert_eq!(regional.policies()[0].id(), "catalog.tenant.read");
    assert_eq!(regional.response_default().type_name(), Some("Json"));
    assert_eq!(regional.response_default().owner(), Some("CatalogApi"));
    assert_eq!(
        regional
            .operations(HeadFamily::Header)
            .iter()
            .map(|operation| operation.operation().key())
            .collect::<Vec<_>>(),
        ["X-Client-Version", "Accept-Language"]
    );
    assert!(
        CATALOG
            .owner("CatalogApi::regional::item")
            .is_some_and(|owner| owner.effective().is_none())
    );

    // A scope with no endpoints is a runtime-binding context: its owner-only
    // value is live through the runtime projection and its view is complete.
    let contract = minimal(OwnerSpec::client("Dyn").timeout(Duration::from_secs(5)))
        .policy(
            PolicyDefinition::new("scoped", "dyn.scoped")
                .formal("account")
                .origin()
                .custom("account", "account")
                .window(10, MINUTE),
        )
        .scope(
            OwnerSpec::scope("Dyn::runtime")
                .value(ValueSpec::new("account", "String"))
                .value(
                    ValueSpec::new("trace", "String")
                        .optional()
                        .runtime_projection_only(),
                )
                .response_default(RepresentationSelection::ExplicitFence)
                .rate_limit(
                    RateAttachment::named("scoped").bind("account", ValueRef::local("account")),
                ),
        )
        .build()
        .expect("runtime binding context");
    let view = contract
        .owner("Dyn::runtime")
        .and_then(OwnerDescription::effective)
        .expect("scope view");
    assert!(view.response_default().fenced());
    assert_eq!(view.response_default().owner(), Some("Dyn::runtime"));
    assert_eq!(view.timeout().source(), TimeoutSource::Declared("Dyn"));
    assert_eq!(
        view.policies()[0].dimensions()[1].binding(),
        Some(ValueRef::exact("Dyn::runtime", "account"))
    );
    assert!(
        contract
            .policy("dyn.scoped")
            .is_some_and(PolicyCoverage::is_unused)
    );
}
