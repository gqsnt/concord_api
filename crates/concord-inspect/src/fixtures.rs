//! Hand-built Catalog-like contract fixtures and their mutation corpus.
//!
//! The baseline is the canonical §22.2 Catalog declaration expressed through
//! the core generated-code bridge, extended with an `archive` scope that
//! declares a response fence, an `admin` scope whose endpoints share a named
//! policy, a secondary partner credential and a `ping` endpoint with a
//! navigation-derived ID. Each [`CatalogMutation`] is one targeted
//! declaration edit used by the V3-O02/V3-O03 mutation corpus and the §24.2
//! edit battery. These fixtures are construction-only: they describe a test
//! host and run no network work.

use std::sync::LazyLock;
use std::time::Duration;

use concord_core::body::BodyDescription;
use concord_core::inspection::{
    AtomExpression, AuthOperation, AuthPlacement, ChallengeRule, ContractBuilder,
    ContractDescription, CredentialDescription, Effect, Emission, EndpointSpec, KeyOperation,
    OwnerSpec, PaginationContract, PathOperation, PathSegment, PathTemplate, PolicyDefinition,
    RateAttachment, RepresentationSelection, ResponseAcquisition, ResultSpec, SecretDescription,
    SubdomainOperation, ValueRef, ValueSpec,
};

/// One targeted declaration edit applied to the Catalog-like baseline.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogMutation {
    /// The unmodified baseline.
    Baseline,
    /// The root `X-Bootstrap-Key` placement uses `partner_key` instead of
    /// `application_key` (same header target).
    SwitchCredentialAtSameHeader,
    /// `"X-Client-Version" = "3"` becomes `"4"` (same key).
    ChangeLiteralKeepKey,
    /// `inspect`'s `"languageEcho" = language` becomes `= regional.language`.
    RemapValue,
    /// The `archive` scope drops its `response explicit;` fence.
    RemoveRepresentationFence,
    /// `item_summaries` count default `DEFAULT_PAGE_SIZE` becomes
    /// `MAX_PAGE_SIZE`.
    ChangeDefaultExpression,
    /// The write policy label `"client.tenant"` becomes `"tenant"`.
    RenameRateLabel,
    /// `admin::audit` moves from shared `admin_reads` to its own inline
    /// policy with equal numbers.
    SplitSharing,
    /// The write policy cost 5 becomes 3.
    ChangeCost,
    /// `tenant_reads` encodes its tenant through `via TenantKeyV2` with a new
    /// encoding ID.
    ChangeKeyCodec,
    /// `regional::ping` (derived ID) moves to the client root.
    MoveEndpoint,
    /// `admin::stats` `period` gains the default `= "day"`.
    AddDefault,
    /// `admin_reads` is hoisted from `stats`/`audit` to the `admin` scope,
    /// newly covering `admin::health`.
    HoistQuota,
    /// The `admin` scope is renamed `operations`.
    RenameScope,
    /// The client gains an optional value named `id` (placed in a new
    /// header); endpoint-local `id` references stay local.
    InsertSameNamedAncestorValue,
    /// The equal-number `admin_reads` definition is merged into
    /// `tenant_reads`.
    MergeEqualDefinitions,
    /// `tenant_reads` narrows its per-minute window from 120 to 100 requests
    /// (window edit only: same ID, labels, cost and coverage).
    ChangeWindow,
    /// Adds a sensitive default and a name-sensitive literal header whose
    /// hidden text is the given sentinel.
    SensitiveLiterals(&'static str),
}

impl CatalogMutation {
    /// Every mutation other than the baseline and the parameterized
    /// sensitive-literal variant.
    pub const CORPUS: [Self; 16] = [
        Self::SwitchCredentialAtSameHeader,
        Self::ChangeLiteralKeepKey,
        Self::RemapValue,
        Self::RemoveRepresentationFence,
        Self::ChangeDefaultExpression,
        Self::RenameRateLabel,
        Self::SplitSharing,
        Self::ChangeCost,
        Self::ChangeKeyCodec,
        Self::MoveEndpoint,
        Self::AddDefault,
        Self::HoistQuota,
        Self::RenameScope,
        Self::InsertSameNamedAncestorValue,
        Self::MergeEqualDefinitions,
        Self::ChangeWindow,
    ];
}

const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(3_600);
const JSON: Option<&str> = Some("application/json");

fn json_result(output: &'static str) -> ResultSpec {
    ResultSpec::new(output, ResponseAcquisition::Buffered, JSON)
}

fn json_body(name: &'static str, input: &'static str) -> concord_core::inspection::BodySpec {
    concord_core::inspection::BodySpec::new(
        name,
        input,
        BodyDescription::buffered(Some("application/json")),
    )
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

/// Builds the Catalog-like contract with one mutation applied.
///
/// # Panics
///
/// Panics if the fixture declaration is invalid, which is a fixture bug
/// covered by this crate's tests.
#[must_use]
pub fn catalog(mutation: CatalogMutation) -> ContractDescription {
    catalog_builder(mutation)
        .build()
        .expect("Catalog-like fixture declaration is valid")
}

/// The baseline fixture as a `'static` description, for registries that
/// hand out `&'static ContractDescription` like generated `contract()`.
#[must_use]
pub fn catalog_static() -> &'static ContractDescription {
    static BASELINE: LazyLock<ContractDescription> =
        LazyLock::new(|| catalog(CatalogMutation::Baseline));
    &BASELINE
}

#[allow(
    clippy::too_many_lines,
    reason = "one fixture declaration mirrors one complete api! invocation"
)]
fn catalog_builder(mutation: CatalogMutation) -> ContractBuilder {
    use CatalogMutation as M;
    let admin_path: &'static str = if mutation == M::RenameScope {
        "CatalogApi::operations"
    } else {
        "CatalogApi::admin"
    };
    let admin_child = |name: &'static str| -> &'static str {
        match (mutation == M::RenameScope, name) {
            (true, "stats") => "CatalogApi::operations::stats",
            (true, "audit") => "CatalogApi::operations::audit",
            (true, _) => "CatalogApi::operations::health",
            (false, "stats") => "CatalogApi::admin::stats",
            (false, "audit") => "CatalogApi::admin::audit",
            (false, _) => "CatalogApi::admin::health",
        }
    };

    let mut client = OwnerSpec::client("CatalogApi")
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
        .value(ValueSpec::new("api_version", "ApiVersion").trait_default());
    if mutation == M::InsertSameNamedAncestorValue {
        client = client.value(ValueSpec::new("id", "ItemId").optional());
    }
    if let M::SensitiveLiterals(sentinel) = mutation {
        client = client.value(
            ValueSpec::new("audit_tag", "String")
                .sensitive()
                .stable_literal(sentinel),
        );
    }
    client = client
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
        ));
    if let M::SensitiveLiterals(_) = mutation {
        client = client.header(KeyOperation::direct(
            "X-Audit-Tag",
            Emission::value(ValueRef::local("audit_tag")),
        ));
    }
    client = client.header(KeyOperation::introduce(
        "X-Client-Version",
        Emission::literal(if mutation == M::ChangeLiteralKeepKey {
            "4"
        } else {
            "3"
        }),
    ));
    if mutation == M::InsertSameNamedAncestorValue {
        client = client.header(KeyOperation::introduce(
            "X-Default-Item",
            Emission::value(ValueRef::local("id")),
        ));
    }
    if let M::SensitiveLiterals(sentinel) = mutation {
        client = client.header(KeyOperation::introduce(
            "X-Partner-Token",
            Emission::literal(sentinel),
        ));
    }
    client = client.auth(AuthOperation::append(vec![AuthPlacement::header(
        "X-Bootstrap-Key",
        if mutation == M::SwitchCredentialAtSameHeader {
            "partner_key"
        } else {
            "application_key"
        },
    )]));

    let tenant_reads = PolicyDefinition::new("tenant_reads", "catalog.tenant.read")
        .formal("tenant")
        .origin();
    let tenant_reads = if mutation == M::ChangeKeyCodec {
        tenant_reads.custom_via("tenant", "tenant", "TenantKeyV2")
    } else {
        tenant_reads.custom("tenant", "tenant")
    };
    let tenant_encoding = if mutation == M::ChangeKeyCodec {
        "catalog.tenant-key/2"
    } else {
        "concord.utf8/1"
    };
    let admin_policy = if mutation == M::MergeEqualDefinitions {
        "tenant_reads"
    } else {
        "admin_reads"
    };
    let admin_reads = || {
        RateAttachment::named(admin_policy).bind_encoded(
            "tenant",
            ValueRef::client("tenant"),
            if admin_policy == "tenant_reads" {
                tenant_encoding
            } else {
                "concord.utf8/1"
            },
        )
    };

    let mut builder = ContractDescription::builder(client, "https://catalog.example.test/root/")
        .secret(SecretDescription::new("bootstrap_key"))
        .secret(SecretDescription::new("partner_secret"))
        .credential(CredentialDescription::api_key(
            "application_key",
            "bootstrap_key",
        ))
        .credential(CredentialDescription::api_key(
            "partner_key",
            "partner_secret",
        ))
        .credential(CredentialDescription::endpoint("session", "authenticate"))
        .policy(
            tenant_reads
                .window(
                    if mutation == M::ChangeWindow {
                        100
                    } else {
                        120
                    },
                    MINUTE,
                )
                .window(5_000, HOUR),
        );
    if mutation != M::MergeEqualDefinitions {
        builder = builder.policy(
            PolicyDefinition::new("admin_reads", "catalog.admin.read")
                .formal("tenant")
                .origin()
                .custom("tenant", "tenant")
                .window(120, MINUTE)
                .window(5_000, HOUR),
        );
    }

    builder = builder.endpoint(
        EndpointSpec::new("CatalogApi::authenticate", "POST")
            .docs(&["Explicit login. Use store_as_session to request local installation."])
            .body(json_body("input", "LoginInput"))
            .result(json_result("SessionToken"))
            .id("catalog.auth.login")
            .path(literal_path("auth/session", &["auth", "session"]))
            .effect(Effect::NonIdempotent),
    );
    if mutation == M::MoveEndpoint {
        builder = builder.endpoint(ping("CatalogApi::ping"));
    }
    builder = builder.scope(
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
                tenant_encoding,
            )),
    );
    builder = builder
        .endpoint(
            EndpointSpec::new("CatalogApi::regional::item_summaries", "GET")
                .docs(&["One decoded page; select an explicit traversal terminal for more."])
                .value(ValueSpec::new("category", "Category").optional())
                .value(ValueSpec::new("offset", "u64").stable_literal("0"))
                .value(ValueSpec::new("count", "NonZeroU32").stable_constant(
                    if mutation == M::ChangeDefaultExpression {
                        "MAX_PAGE_SIZE"
                    } else {
                        "DEFAULT_PAGE_SIZE"
                    },
                ))
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
                .body(json_body("input", "UpdateInput"))
                .result(json_result("Item"))
                .id("catalog.item.update")
                .path(items_id_path())
                .effect(Effect::Idempotent)
                .rate_limit(
                    RateAttachment::inline("catalog.item.write")
                        .origin()
                        .endpoint()
                        .custom_encoded(
                            if mutation == M::RenameRateLabel {
                                "tenant"
                            } else {
                                "client.tenant"
                            },
                            ValueRef::client("tenant"),
                            None,
                            Some("concord.utf8/1"),
                        )
                        .cost(if mutation == M::ChangeCost { 3 } else { 5 })
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
                    ResultSpec::new("ByteStream", ResponseAcquisition::Streaming, None)
                        .via("Binary"),
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
                    Emission::value(if mutation == M::RemapValue {
                        ValueRef::scope("regional", "language")
                    } else {
                        ValueRef::local("language")
                    }),
                )),
        );
    if mutation != M::MoveEndpoint {
        builder = builder.endpoint(ping("CatalogApi::regional::ping"));
    }
    let archive = OwnerSpec::scope("CatalogApi::regional::archive")
        .path(literal_path("archive", &["archive"]));
    let archive = if mutation == M::RemoveRepresentationFence {
        archive
    } else {
        archive.response_default(RepresentationSelection::ExplicitFence)
    };
    builder = builder.scope(archive).endpoint(
        EndpointSpec::new("CatalogApi::regional::archive::download", "GET")
            .docs(&["Raw archive bytes; the scope fences the inherited Json default."])
            .result(ResultSpec::new("Bytes", ResponseAcquisition::Buffered, None).via("Binary"))
            .id("catalog.archive.download")
            .path(literal_path("download", &["download"])),
    );

    let mut admin_scope = OwnerSpec::scope(admin_path)
        .docs(&["Tenant administration views sharing one read budget."])
        .path(literal_path("admin", &["admin"]));
    if mutation == M::HoistQuota {
        admin_scope = admin_scope.rate_limit(admin_reads());
    }
    let mut period = ValueSpec::new("period", "Period");
    if mutation == M::AddDefault {
        period = period.stable_literal("\"day\"");
    }
    let mut stats = EndpointSpec::new(admin_child("stats"), "GET")
        .value(period)
        .result(json_result("Stats"))
        .id("catalog.admin.stats")
        .path(PathOperation::Append(PathTemplate::new(
            "stats/{period}",
            vec![
                PathSegment::literal("stats"),
                PathSegment::hole(ValueRef::local("period")),
            ],
        )));
    let mut audit = EndpointSpec::new(admin_child("audit"), "GET")
        .result(json_result("Audit"))
        .id("catalog.admin.audit")
        .path(literal_path("audit", &["audit"]));
    if mutation != M::HoistQuota {
        stats = stats.rate_limit(admin_reads());
        audit = audit.rate_limit(if mutation == M::SplitSharing {
            RateAttachment::inline("catalog.admin.audit")
                .origin()
                .custom_encoded(
                    "tenant",
                    ValueRef::client("tenant"),
                    None,
                    Some("concord.utf8/1"),
                )
                .window(120, MINUTE)
                .window(5_000, HOUR)
        } else {
            admin_reads()
        });
    }
    builder
        .scope(admin_scope)
        .endpoint(stats)
        .endpoint(audit)
        .endpoint(
            EndpointSpec::new(admin_child("health"), "GET")
                .result(json_result("Health"))
                .path(literal_path("health", &["health"])),
        )
}

fn ping(path: &'static str) -> EndpointSpec {
    EndpointSpec::new(path, "GET")
        .docs(&["Liveness probe with a navigation-derived ID."])
        .result(json_result("Pong"))
        .path(literal_path("ping", &["ping"]))
}
