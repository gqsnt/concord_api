//! The current (v3) language surface: exact value controls, nested optional
//! slots, contract reflection, generated names and the §22 Catalog
//! signatures (§4, §7, §18, §22).

use concord::atom::{AtomEncoder, AtomError, BoundedAtomWriter};
use concord::{Binary, Bytes, Text, api};

const DEFAULT_COUNT: u64 = 25;

/// Encodes the inner layer of an `Option` payload: `none` or the value.
///
/// Inner optional encoding is an atom responsibility (VALUE-01), never a
/// generic omission rule.
pub struct InnerOption;

impl AtomEncoder<Option<u8>> for InnerOption {
    fn write(value: &Option<u8>, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        match value {
            Some(value) => out.push_str(&value.to_string()),
            None => out.push_str("none"),
        }
    }
}

impl AtomEncoder<Option<String>> for InnerOption {
    fn write(value: &Option<String>, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(value.as_deref().unwrap_or("none"))
    }
}

api! {
    /// Compile-checked current language surface.
    client CurrentLanguageApi {
        base "https://example.test/root/";
        timeout 20s;

        values {
            /// Required domain payload.
            tenant: String;
            locale?: String;
            #[default]
            retry_hint?: Option<u8>;
        }

        subdomain ["api"];
        path "{tenant}";
        query {
            "locale" = locale;
            "retry_hint" = retry_hint via InnerOption;
        }

        scope regional(region: String, language?: String) {
            subdomain [region];
            headers {
                "X-Language" = language;
            }

            GET list(
                required_option: Option<String>,
                tags: Vec<String>,
                count: u64 = DEFAULT_COUNT,
            ) -> Bytes via Binary {
                id "v2.language.list";
                path "items";
                query {
                    "tag" = each tags;
                    "count" = count;
                    "required_option" = required_option via InnerOption;
                }
                effect idempotent;
                expose accepted_bytes;
            }

            POST update(
                id: String,
                mode?: Option<String>,
                body input: String via Text,
            ) -> String via Text {
                id "v2.language.update";
                path "items/{id}";
                query { "mode" = mode via InnerOption; }
                effect non_idempotent;
            }
        }
    }
}

api! {
    client ReflectedAuthoritiesApi {
        base "https://example.test";
        timeout 30s;

        values {
            tenant: String;
        }

        credentials {
            secret token;
            credential bearer_auth = bearer(secret.token);
        }

        rate_limits {
            policy tenant_reads(tenant) {
                id "v2.reflection.tenant";
                key [origin, "tenant" = tenant];
                cost 2;
                allow 10 per 1m;
            }
        }

        GET fetch() -> Bytes via Binary {
            id "v2.reflection.fetch";
            auth {
                bearer bearer_auth;
            }
            rate_limit tenant_reads(tenant = client.tenant);
        }
    }
}

fn current_client() -> CurrentLanguageApi {
    CurrentLanguageApi::builder()
        .set_tenant("tenant")
        .set_locale("fr")
        .omit_locale()
        .reset_retry_hint()
        .build()
        .expect("v3 client")
}

#[test]
fn exact_value_and_capability_surface_compiles() {
    let client = current_client();

    let regional = client.regional("eu").set_language("French");
    // `None` targets the required `Option<String>` payload through identity
    // conversion; `Vec::new()`/`vec![..]` target `Vec<String>` exactly.
    let list = regional
        .list(None, vec!["one".to_owned(), "two".to_owned()])
        .reset_count();
    let plan: concord::endpoint::RequestPlan = list.plan().expect("plan");
    assert_eq!(
        plan.observed_url(),
        "https://eu.api.example.test/root/tenant/items?retry_hint=none&tag=one&tag=two&count=25&required_option=none"
    );
    let headers = format!("{:?}", plan.public_headers());
    assert!(headers.contains("French"), "{headers}");
    let _bytes: concord::endpoint::AcceptedBytesRequest = list.accepted_bytes();

    let intent = CurrentLanguageApi::unbound()
        .regional("eu")
        .set_language("French")
        .list(None, vec!["one".to_owned()]);
    assert_eq!(intent.description().id(), "v2.language.list");
    let unbound = client.plan(&intent).expect("unbound plan");
    let direct = regional
        .list(None, vec!["one".to_owned()])
        .plan()
        .expect("direct plan");
    assert_eq!(unbound.observed_url(), direct.observed_url());
    let _bound: current_language_api::regional::ListRequest = client.request(intent);

    let update = regional
        .update("id", "body".to_owned())
        .set_mode(None)
        .replace_mode(Some(None))
        .replace_mode(Some(Some("replace".to_owned())))
        .omit_mode();
    assert!(
        !update
            .plan()
            .expect("update plan")
            .observed_url()
            .contains("mode=")
    );
}

/// V3-L06: every operation on a nested optional slot, including `Some(None)`;
/// exact `None` clears a prior explicit or default value while the conditional
/// `None` preserves it (VALUE-03).
#[test]
fn nested_optional_slot_operations_follow_the_exact_transitions() {
    let client = current_client();
    let regional = client.regional("eu");
    let mode = |request: current_language_api::regional::UpdateRequest| {
        let url = request
            .plan()
            .expect("update plan")
            .observed_url()
            .to_owned();
        url.split_once('?')
            .map(|(_, query)| {
                query
                    .split('&')
                    .filter(|pair| pair.starts_with("mode="))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let update = || regional.update("id", "body".to_owned());

    assert!(
        mode(update()).is_empty(),
        "uninitialized optional starts absent"
    );
    assert_eq!(
        mode(update().set_mode(None)),
        ["mode=none"],
        "present inner None"
    );
    assert_eq!(
        mode(update().set_mode(Some("x".to_owned()))),
        ["mode=x"],
        "set takes the exact payload"
    );
    assert!(
        mode(update().set_mode(None).replace_mode(None)).is_empty(),
        "exact None clears the outer slot"
    );
    assert_eq!(
        mode(update().replace_mode(Some(None))),
        ["mode=none"],
        "exact Some(None) is present inner None"
    );
    assert_eq!(
        mode(update().replace_mode(Some(Some("replace".to_owned())))),
        ["mode=replace"]
    );
    assert_eq!(
        mode(
            update()
                .set_mode(Some("kept".to_owned()))
                .set_mode_if_some(None)
        ),
        ["mode=kept"],
        "conditional None keeps the current value"
    );
    assert!(
        mode(update().set_mode_if_some(None)).is_empty(),
        "conditional None keeps absence"
    );
    assert_eq!(
        mode(update().set_mode_if_some(Some(None))),
        ["mode=none"],
        "conditional Some(None) selects present inner None"
    );
    assert!(mode(update().set_mode(None).omit_mode()).is_empty());

    // Initialized optional root slot: `#[default] retry_hint?: Option<u8>`
    // starts present with inner `None`.
    let hint = |builder: current_language_api::CurrentLanguageApiBuilder| {
        let client = builder.set_tenant("tenant").build().expect("client");
        let url = client
            .regional("eu")
            .update("id", "body".to_owned())
            .plan()
            .expect("plan")
            .observed_url()
            .to_owned();
        url.split_once('?')
            .map(|(_, query)| {
                query
                    .split('&')
                    .filter(|pair| pair.starts_with("retry_hint="))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let builder = CurrentLanguageApi::builder;
    assert_eq!(hint(builder()), ["retry_hint=none"]);
    assert!(
        hint(builder().replace_retry_hint(None)).is_empty(),
        "exact None clears the default"
    );
    assert_eq!(
        hint(builder().set_retry_hint_if_some(None)),
        ["retry_hint=none"],
        "conditional None preserves the default"
    );
    assert_eq!(hint(builder().set_retry_hint(Some(3))), ["retry_hint=3"]);
    assert_eq!(
        hint(
            builder()
                .set_retry_hint(Some(3))
                .set_retry_hint_if_some(None)
        ),
        ["retry_hint=3"],
        "conditional None preserves an explicit value"
    );
    assert!(
        hint(builder().set_retry_hint(Some(3)).replace_retry_hint(None)).is_empty(),
        "exact None clears an explicit value"
    );
    assert_eq!(
        hint(
            builder()
                .set_retry_hint(Some(3))
                .omit_retry_hint()
                .reset_retry_hint()
        ),
        ["retry_hint=none"],
        "reset restores the retained initial present slot"
    );
    assert_eq!(
        hint(builder().replace_retry_hint(Some(Some(9)))),
        ["retry_hint=9"]
    );
}

#[test]
fn contract_reflection_preserves_the_domain_tree_and_exact_semantics() {
    use concord::inspection::{
        HeadFamily, IdSource, KeyOperationKind, OperationSite, OwnerKind, ValueInitializer,
        ValuePresence, ValueRole,
    };

    let contract = CurrentLanguageApi::contract();
    assert_eq!(contract.name(), "CurrentLanguageApi");
    assert_eq!(
        contract.client().docs(),
        ["Compile-checked current language surface."]
    );
    assert_eq!(contract.base().origin(), "https://example.test:443");
    assert_eq!(contract.base().path_prefix(), "/root/");
    assert_eq!(
        contract.root_timeout().duration(),
        std::time::Duration::from_secs(20)
    );
    let values = contract.client().values();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0].owner_kind(), OwnerKind::Client);
    assert_eq!(values[0].name(), "tenant");
    assert_eq!(values[0].docs(), ["Required domain payload."]);
    assert_eq!(values[0].presence(), ValuePresence::Required);
    assert_eq!(values[0].role(), ValueRole::BuilderRequired);
    assert_eq!(values[1].presence(), ValuePresence::Optional);
    assert_eq!(values[1].initializer(), ValueInitializer::None);
    assert_eq!(values[2].initializer(), ValueInitializer::TraitDefault);
    assert_eq!(values[2].presence(), ValuePresence::Optional);
    assert!(values[2].payload_type().contains("Option"));

    let client_view = contract
        .client()
        .effective()
        .expect("client effective view");
    let client_query = client_view.operations(HeadFamily::Query);
    assert_eq!(
        client_query
            .iter()
            .map(|operation| operation.operation().key())
            .collect::<Vec<_>>(),
        ["locale", "retry_hint"]
    );
    assert!(client_query.iter().all(|operation| {
        operation.operation().kind() == KeyOperationKind::Introduce
            && operation.operation().site() == OperationSite::Block
    }));

    let regional = contract
        .owner("CurrentLanguageApi::regional")
        .expect("regional owner");
    assert_eq!(regional.kind(), OwnerKind::Scope);
    assert_eq!(regional.name(), "regional");
    assert_eq!(regional.values()[0].owner_kind(), OwnerKind::Scope);
    assert_eq!(regional.values()[0].role(), ValueRole::Constructor);
    assert_eq!(regional.children().len(), 2);
    let regional_view = regional.effective().expect("regional effective view");
    let labels = regional_view.route().labels();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].origin(), "CurrentLanguageApi::regional");
    assert_eq!(labels[1].origin(), "CurrentLanguageApi");
    assert_eq!(
        contract.route_template(regional_view.route()),
        "https://{regional.region}.api.example.test/root/{client.tenant}"
    );

    let list = contract
        .endpoint("v2.language.list")
        .expect("reflected list endpoint");
    assert_eq!(list.identity().source(), IdSource::Explicit);
    assert_eq!(list.path(), "CurrentLanguageApi::regional::list");
    let list_owner = contract.owner(list.path()).expect("list owner");
    assert_eq!(list_owner.kind(), OwnerKind::Endpoint);
    assert_eq!(list_owner.values()[0].name(), "required_option");
    assert_eq!(list_owner.values()[0].presence(), ValuePresence::Required);
    assert_eq!(list_owner.values()[0].role(), ValueRole::Constructor);
    assert_eq!(list.result().output_type(), "Bytes");
    assert_eq!(list.result().representation().type_name(), "Binary");
    assert!(list.terminals().accepted_bytes());
    assert!(!list.terminals().pagination());
    assert!(
        list.operations(HeadFamily::Query)
            .iter()
            .any(|operation| operation.operation().key() == "tag"
                && operation.origin() == "CurrentLanguageApi::regional::list")
    );
    assert_eq!(
        contract.url_template(list),
        "https://{regional.region}.api.example.test/root/{client.tenant}/items"
    );
    let update = contract
        .endpoint("v2.language.update")
        .expect("update endpoint");
    let body = update.body().expect("update body");
    assert_eq!(body.name(), "input");
    assert_eq!(body.presence(), ValuePresence::Required);
    assert_eq!(body.representation().type_name(), "Text");
}

#[test]
fn contract_reflection_includes_value_free_authority_and_profile_schemas() {
    use concord::inspection::{CredentialSourceKind, FormalDimension};

    let contract = ReflectedAuthoritiesApi::contract();
    assert_eq!(contract.secrets().len(), 1);
    assert_eq!(contract.secrets()[0].name(), "token");
    // Secrets are protected builder inputs, never ordinary public values.
    assert!(contract.client().value("token").is_none());

    assert_eq!(contract.credentials().len(), 1);
    let credential = &contract.credentials()[0];
    assert_eq!(credential.name(), "bearer_auth");
    assert_eq!(credential.kind(), CredentialSourceKind::Bearer);
    assert_eq!(credential.secrets(), ["token"]);
    assert_eq!(credential.source_endpoint(), None);
    assert_eq!(credential.provider_type(), None);

    assert_eq!(contract.policy_definitions().len(), 1);
    let profile = &contract.policy_definitions()[0];
    assert_eq!(profile.name(), "tenant_reads");
    assert_eq!(profile.id(), "v2.reflection.tenant");
    assert_eq!(profile.formals(), ["tenant"]);
    assert_eq!(
        profile.dimensions(),
        [
            FormalDimension::Origin,
            FormalDimension::Custom {
                label: "tenant",
                formal: "tenant",
                encoder: None,
            }
        ]
    );
    assert_eq!(profile.declared_cost(), Some(2));
    assert_eq!(profile.inline_windows().len(), 1);

    let fetch = contract
        .endpoint("v2.reflection.fetch")
        .expect("reflected fetch endpoint");
    let policy = &fetch.policies()[0];
    assert_eq!(policy.id(), "v2.reflection.tenant");
    assert_eq!(
        policy
            .dimensions()
            .iter()
            .map(|dimension| dimension.label())
            .collect::<Vec<_>>(),
        ["origin", "tenant"]
    );
    assert_eq!(policy.cost(), 2);
    assert_eq!(fetch.auth().placements().len(), 1);
    assert_eq!(
        fetch.auth().placements()[0].placement().credential(),
        "bearer_auth"
    );
}

/// V3-L01: the complete §22 Catalog declaration compiles and its generated
/// public signatures match §7.3/§7.4/§22.3 exactly; removed capabilities are
/// absent (checked through inherent-method precedence, see `assert_absent!`).
mod catalog {
    use std::fmt;
    use std::future::IntoFuture;
    use std::num::NonZeroU32;
    use std::sync::Arc;
    use std::time::Duration;

    use concord::auth::{CredentialMaterial, SecretCredential, StoreRequest};
    use concord::endpoint::{
        AcceptedBytesRequest, EndpointDescription, RequestPlan, ResponseRequest,
    };
    use concord::pagination::{
        CollectAll, CollectPrefix, OffsetInit, OffsetPagination, PageDecisionLimits, PageResponse,
        PageStep, Pages, PaginationController, PaginationDescription, PaginationError,
        PaginationErrorKind,
    };
    use concord::{
        ApiError, AtomError, Binary, BoundedAtomWriter, ByteStream, CallOptions, CancellationToken,
        ClientBuildError, ControlError, ControlSummary, Json, RequestAtom, RequestControls,
        SecretString, api,
    };
    use serde::{Deserialize, Serialize};

    pub type TenantId = String;
    pub type Locale = String;
    pub type Region = String;
    pub type Language = String;
    pub type Category = String;
    pub type Tag = String;
    pub type ItemId = String;
    pub type CommandId = String;

    pub const DEFAULT_PAGE_SIZE: NonZeroU32 = match NonZeroU32::new(25) {
        Some(n) => n,
        None => panic!("25 is nonzero"),
    };

    pub struct ApiVersion(&'static str);
    impl Default for ApiVersion {
        fn default() -> Self {
            Self("v2")
        }
    }
    impl RequestAtom for ApiVersion {
        fn write_atom(&self, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
            out.push_str(self.0)
        }
    }

    #[derive(Serialize)]
    pub struct LoginInput {
        pub user: String,
    }
    #[derive(Serialize)]
    pub struct UpdateInput {
        pub title: String,
    }
    #[derive(Deserialize)]
    #[allow(dead_code, reason = "decoded DTO of the canonical fixture")]
    pub struct ItemSummary {
        pub id: String,
    }
    #[derive(Deserialize)]
    #[allow(dead_code, reason = "decoded DTO of the canonical fixture")]
    pub struct Item {
        pub id: String,
        pub title: String,
    }

    pub struct SessionToken(SecretString);
    impl CredentialMaterial for SessionToken {}
    impl SecretCredential for SessionToken {
        fn expose_secret(&self) -> &str {
            self.0.expose_secret()
        }
    }
    impl fmt::Debug for SessionToken {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("SessionToken(<redacted>)")
        }
    }
    impl<'de> Deserialize<'de> for SessionToken {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let text = String::deserialize(d)?;
            SecretString::try_new(text)
                .map(Self)
                .map_err(serde::de::Error::custom)
        }
    }

    #[derive(Deserialize)]
    pub struct Envelope<T> {
        pub data: Vec<T>,
        pub next: Option<String>,
    }

    pub struct CursorStart {
        pub after: Option<String>,
    }

    pub struct EnvelopePages {
        after: Option<String>,
    }

    impl<T: Send + 'static> PaginationController<Envelope<T>> for EnvelopePages {
        type Item = T;
        type Init = CursorStart;
        type Cursor = Option<String>;
        const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

        fn start(init: Self::Init) -> Result<Self, PaginationError> {
            Ok(Self { after: init.after })
        }

        fn cursor(&self) -> Self::Cursor {
            self.after.clone()
        }

        fn advance(
            self,
            response: PageResponse<Envelope<T>>,
            limits: PageDecisionLimits,
        ) -> Result<PageStep<Self, T>, PaginationError> {
            let Envelope { data, next } = response.into_page();
            if data.len() > limits.remaining_items() {
                return Err(PaginationError::new(
                    PaginationErrorKind::CollectedItemCeilingExceeded,
                ));
            }
            match next {
                None => Ok(PageStep::Done { items: data }),
                Some(cursor) => {
                    if cursor.is_empty() || self.after.as_ref() == Some(&cursor) {
                        return Err(PaginationError::new(
                            PaginationErrorKind::InvalidContinuation,
                        ));
                    }
                    Ok(PageStep::Continue {
                        next: Self {
                            after: Some(cursor),
                        },
                        items: data,
                    })
                }
            }
        }
    }

    api! {
        /// Typed, tenant-scoped Catalog provider client.
        client CatalogApi {
            base "https://catalog.example.test/root/";
            timeout 20s;
            request Json;
            response Json;

            values {
                /// The tenant is sent, but excluded from automatic observation.
                #[sensitive] query tenant: TenantId;
                query locale?: Locale;
                #[default] api_version: ApiVersion;
            }

            credentials {
                secret bootstrap_key;
                credential application_key = api_key(secret.bootstrap_key);
                credential session = endpoint authenticate;
            }

            rate_limits {
                policy tenant_reads(tenant) {
                    id "catalog.tenant.read";
                    key [origin, "tenant" = tenant];
                    allow 120 per 1m;
                    allow 5000 per 1h;
                }
            }

            subdomain ["api"];
            path "{api_version}";
            headers { "X-Client-Version" = "3"; }
            auth { header "X-Bootstrap-Key" = application_key; }

            /// Explicit login. Use store_as_session to request local installation.
            POST authenticate(body input: LoginInput) -> SessionToken {
                id "catalog.auth.login";
                path "auth/session";
                effect non_idempotent;
            }

            scope regional(
                region: Region,
                header("Accept-Language") language?: Language,
            ) {
                subdomain [region];
                path "regions/{region}";
                auth replace { bearer session challenge unauthorized; }
                rate_limit tenant_reads(tenant = client.tenant);

                /// One decoded page; select an explicit traversal terminal for more.
                GET item_summaries(
                    query category?: Category,
                    query offset: u64 = 0,
                    query count: NonZeroU32 = DEFAULT_PAGE_SIZE,
                    query("tag") each tags: Vec<Tag>,
                ) -> Vec<ItemSummary> {
                    id "catalog.items.list";
                    path "items";
                    paginate via OffsetPagination {
                        init OffsetInit { offset: offset, page_size: count };
                        cursor -> offset;
                    }
                }

                /// A strict lookup. Its explicit byte terminal does not decode Json.
                GET item(id: ItemId, locale_override?: Locale) -> Item {
                    id "catalog.item.get";
                    path "items/{id}";
                    query { when_present "locale" = locale_override; }
                    expose accepted_bytes;
                }

                /// A head-only 404 is None; the body of that response is not polled.
                GET find_item(id: ItemId) -> Option<Item> {
                    id "catalog.item.find";
                    path "items/{id}";
                    absent on 404;
                }

                /// The provider asserts idempotence for this command-bearing update.
                PUT update_item(
                    id: ItemId,
                    #[sensitive] header("Idempotency-Key") command_id: CommandId,
                    body input: UpdateInput,
                ) -> Item {
                    id "catalog.item.update";
                    path "items/{id}";
                    effect idempotent;
                    rate_limit {
                        id "catalog.item.write";
                        key [origin, endpoint, "client.tenant" = client.tenant];
                        cost 5;
                        allow 30 per 1m;
                    }
                }

                /// Cursor-envelope page; the controller owns extraction, not Json.
                GET feed(query after?: String, query count: NonZeroU32 = DEFAULT_PAGE_SIZE)
                    -> Envelope<ItemSummary>
                {
                    id "catalog.item.feed";
                    path "feed";
                    paginate via EnvelopePages {
                        init CursorStart { after: after };
                        cursor -> after;
                    }
                }

                /// Returns a managed byte stream, not a completed export.
                GET export() -> ByteStream via Binary {
                    id "catalog.item.export";
                    path "export";
                }

                /// One caller-controlled optional is deliberately used three times.
                GET inspect(query language?: Language) -> Item {
                    id "catalog.item.inspect";
                    path "inspect";
                    headers { replace "Accept-Language" = language; }
                    query { "languageEcho" = language; }
                }
            }
        }
    }

    use catalog_api::regional::{
        ExportRequest, FeedRequest, FindItemRequest, InspectRequest, ItemEndpoint, ItemRequest,
        ItemSummariesRequest, Regional, UpdateItemEndpoint, UpdateItemRequest,
    };
    use catalog_api::{AuthenticateRequest, CatalogApiBuilder};

    /// Proves a method is **absent** from a value's inherent surface: an
    /// inherent method always wins over a same-named blanket trait method, so
    /// the probe only type-checks when no inherent method exists.
    macro_rules! assert_absent {
        ($value:expr => $method:ident()) => {{
            struct Absent;
            trait Probe: Sized {
                fn $method(self) -> Absent {
                    Absent
                }
            }
            impl<T> Probe for T {}
            let _: Absent = $value.$method();
        }};
        ($value:expr => $method:ident($argument:expr)) => {{
            struct Absent;
            trait Probe: Sized {
                fn $method<A>(self, _argument: A) -> Absent {
                    Absent
                }
            }
            impl<T> Probe for T {}
            let _: Absent = $value.$method($argument);
        }};
    }

    crate::assert_not_impl!(UpdateItemRequest: Clone);
    crate::assert_not_impl!(UpdateItemEndpoint: Clone);
    crate::assert_not_impl!(AuthenticateRequest: Clone);

    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    fn assert_controls<T: RequestControls>() {}
    fn assert_output<O, R: IntoFuture<Output = Result<O, ApiError>>>() {}

    fn client() -> CatalogApi {
        CatalogApi::builder()
            .set_tenant("acme-tenant-sentinel")
            .bootstrap_key(SecretString::try_new("fake-bootstrap-sentinel").expect("secret"))
            .configure_runtime(|runtime| {
                runtime.rate_limiter(Arc::new(concord::rate_limit::LocalRateLimiter::new()))
            })
            .build()
            .expect("catalog client")
    }

    #[test]
    fn catalog_signatures_match_the_public_contract() {
        // §7.3 construction and binding.
        let _: fn() -> CatalogApiBuilder = CatalogApi::builder;
        let _: fn(CatalogApiBuilder) -> Result<CatalogApi, ClientBuildError> =
            CatalogApiBuilder::build;
        let _: fn() -> catalog_api::Unbound = CatalogApi::unbound;
        let _: fn(&CatalogApi, ItemEndpoint) -> ItemRequest = CatalogApi::request::<ItemEndpoint>;
        let _: fn(&CatalogApi, &ItemEndpoint) -> Result<RequestPlan, ApiError> =
            CatalogApi::plan::<ItemEndpoint>;

        // §22.3 builder controls. `impl IntoValue<T>` arguments accept the
        // closed conversion set: `T`, `&str` and `&String` for `String`.
        let _: fn(CatalogApiBuilder, TenantId) -> CatalogApiBuilder = |b, v| b.set_tenant(v);
        let _: fn(CatalogApiBuilder, &str) -> CatalogApiBuilder = |b, v| b.set_tenant(v);
        let _: fn(CatalogApiBuilder, &String) -> CatalogApiBuilder = |b, v| b.set_tenant(v);
        let _: fn(CatalogApiBuilder, Locale) -> CatalogApiBuilder = |b, v| b.set_locale(v);
        let _: fn(CatalogApiBuilder, Option<Locale>) -> CatalogApiBuilder =
            CatalogApiBuilder::replace_locale;
        let _: fn(CatalogApiBuilder, Option<Locale>) -> CatalogApiBuilder =
            CatalogApiBuilder::set_locale_if_some;
        let _: fn(CatalogApiBuilder) -> CatalogApiBuilder = CatalogApiBuilder::omit_locale;
        let _: fn(CatalogApiBuilder, ApiVersion) -> CatalogApiBuilder = |b, v| b.set_api_version(v);
        let _: fn(CatalogApiBuilder) -> CatalogApiBuilder = CatalogApiBuilder::reset_api_version;
        let _: fn(CatalogApiBuilder, SecretString) -> CatalogApiBuilder =
            CatalogApiBuilder::bootstrap_key;

        // §22.3 navigation, constructors and value controls.
        let _: fn(&CatalogApi, LoginInput) -> AuthenticateRequest = CatalogApi::authenticate;
        let _: fn(AuthenticateRequest) -> StoreRequest = AuthenticateRequest::store_as_session;
        let _: fn(&CatalogApi, Region) -> Regional = |c, r| c.regional(r);
        let _: fn(&CatalogApi, &str) -> Regional = |c, r| c.regional(r);
        let _: fn(Regional, Language) -> Regional = |s, v| s.set_language(v);
        let _: fn(Regional, Option<Language>) -> Regional = Regional::replace_language;
        let _: fn(&Regional, Vec<Tag>) -> ItemSummariesRequest = |s, t| s.item_summaries(t);
        let _: fn(ItemSummariesRequest, Category) -> ItemSummariesRequest =
            |r, c| r.set_category(c);
        let _: fn(ItemSummariesRequest, Option<Category>) -> ItemSummariesRequest =
            ItemSummariesRequest::replace_category;
        let _: fn(ItemSummariesRequest, Option<NonZeroU32>) -> ItemSummariesRequest =
            ItemSummariesRequest::set_count_if_some;
        let _: fn(ItemSummariesRequest) -> ItemSummariesRequest =
            ItemSummariesRequest::reset_offset;
        let _: fn(&Regional, ItemId) -> ItemRequest = |s, id| s.item(id);
        let _: fn(&Regional, ItemId) -> FindItemRequest = |s, id| s.find_item(id);
        let _: fn(&Regional, ItemId, CommandId, UpdateInput) -> UpdateItemRequest =
            |s, id, command, input| s.update_item(id, command, input);
        let _: fn(&Regional) -> FeedRequest = Regional::feed;
        let _: fn(&Regional) -> ExportRequest = Regional::export;
        let _: fn(&Regional) -> InspectRequest = Regional::inspect;

        // §7.4 terminals and §8.1 controls on a generated named request.
        let _: fn(&ItemRequest) -> &'static EndpointDescription = ItemRequest::description;
        let _: fn(&ItemRequest) -> Result<RequestPlan, ApiError> = ItemRequest::plan;
        let _: fn(ItemRequest) -> ResponseRequest<Item> = ItemRequest::response;
        let _: fn(ItemRequest) -> AcceptedBytesRequest = ItemRequest::accepted_bytes;
        let _: fn(ItemRequest, Duration) -> Result<ItemRequest, ControlError> =
            ItemRequest::timeout;
        let _: fn(ItemRequest) -> ItemRequest = ItemRequest::reset_timeout;
        let _: fn(ItemRequest, CancellationToken) -> Result<ItemRequest, ControlError> =
            ItemRequest::cancel_with;
        let _: fn(ItemRequest, &CallOptions) -> ItemRequest = ItemRequest::with_options;
        let _: fn(&ItemRequest) -> ControlSummary = ItemRequest::control_summary;
        let _: fn(FindItemRequest) -> ResponseRequest<Option<Item>> = FindItemRequest::response;
        let _: fn(ItemSummariesRequest, u32) -> Pages<ItemSummary> = ItemSummariesRequest::pages;
        let _: fn(ItemSummariesRequest, u32) -> CollectPrefix<ItemSummary> =
            ItemSummariesRequest::collect_prefix;
        let _: fn(ItemSummariesRequest, NonZeroU32) -> CollectAll<ItemSummary> =
            ItemSummariesRequest::collect_all;
        let _: fn(FeedRequest, u32) -> Pages<ItemSummary> = FeedRequest::pages;

        assert_output::<Vec<ItemSummary>, ItemSummariesRequest>();
        assert_output::<Item, ItemRequest>();
        assert_output::<Option<Item>, FindItemRequest>();
        assert_output::<Item, UpdateItemRequest>();
        assert_output::<Envelope<ItemSummary>, FeedRequest>();
        assert_output::<ByteStream, ExportRequest>();
        assert_output::<SessionToken, AuthenticateRequest>();
        assert_controls::<ItemSummariesRequest>();
        assert_controls::<UpdateItemRequest>();
        assert_controls::<AuthenticateRequest>();
        assert_clone::<CatalogApi>();
        assert_clone::<Regional>();
        assert_clone::<ItemRequest>();
        assert_clone::<ItemEndpoint>();
        assert_clone::<ExportRequest>();
        assert_send::<UpdateItemRequest>();
        assert_send::<<ItemRequest as IntoFuture>::IntoFuture>();

        // The value matrix does not generate these (§4.3, §22.3, §7.4).
        let client = client();
        let regional = client.regional("eu");
        assert_absent!(regional.item_summaries(Vec::new()) => omit_count());
        assert_absent!(regional.item_summaries(Vec::new()) => omit_offset());
        assert_absent!(regional.item_summaries(Vec::new()) => reset_category());
        assert_absent!(regional.item_summaries(Vec::new()) => set_tags(Vec::<Tag>::new()));
        assert_absent!(regional.item_summaries(Vec::new()) => replace_tags(None::<Vec<Tag>>));
        assert_absent!(regional.item_summaries(Vec::new()) => accepted_bytes());
        assert_absent!(regional.item("id") => set_id("other"));
        assert_absent!(regional.item("id") => reset_locale_override());
        assert_absent!(regional.item("id") => pages(1_u32));
        assert_absent!(regional.item("id") => store_as_session());
        assert_absent!(regional.find_item("id") => accepted_bytes());
        assert_absent!(regional.find_item("id") => pages(1_u32));
        assert_absent!(regional.export() => accepted_bytes());
        assert_absent!(regional.update_item("id", "command", UpdateInput { title: "t".to_owned() })
            => set_input(UpdateInput { title: "t".to_owned() }));
        assert_absent!(regional.update_item("id", "command", UpdateInput { title: "t".to_owned() })
            => set_command_id("other"));
        assert_absent!(client.regional("eu") => set_region("us"));
        assert_absent!(client.regional("eu") => reset_language());
        assert_absent!(CatalogApi::builder() => omit_tenant());
        assert_absent!(CatalogApi::builder() => reset_tenant());
        assert_absent!(CatalogApi::builder() => omit_api_version());
    }

    /// V3-L03 on the canonical fixture: `inspect` introduces the optional
    /// `language` directly in the query, uses it explicitly under
    /// `languageEcho` and replaces the inherited scoped header.
    #[test]
    fn one_optional_value_drives_every_intended_target() {
        let client = client();
        let header_value = |plan: &RequestPlan| {
            plan.public_headers()
                .iter()
                .filter(|(name, _)| name.as_str().eq_ignore_ascii_case("accept-language"))
                .map(|(_, value)| format!("{value:?}"))
                .collect::<Vec<_>>()
        };

        let scoped = client.regional("eu").set_language("fr");
        let present = scoped
            .inspect()
            .set_language("de")
            .plan()
            .expect("present plan");
        assert_eq!(
            present.observed_url(),
            "https://eu.api.catalog.example.test/root/v2/regions/eu/inspect?tenant=%3Credacted%3E&language=de&languageEcho=de"
        );
        let present_headers = header_value(&present);
        assert_eq!(present_headers.len(), 1, "{present_headers:?}");
        assert!(present_headers[0].contains("de"), "{present_headers:?}");

        let absent = scoped.inspect().plan().expect("absent plan");
        assert_eq!(
            absent.observed_url(),
            "https://eu.api.catalog.example.test/root/v2/regions/eu/inspect?tenant=%3Credacted%3E"
        );
        assert!(
            header_value(&absent).is_empty(),
            "exact replacement with an absent source clears the inherited header"
        );

        // The lookup's `when_present` fallback is intentionally different:
        // absence keeps the inherited scoped header (§22.2).
        let item = scoped.item("42").plan().expect("item plan");
        let item_headers = header_value(&item);
        assert_eq!(item_headers.len(), 1);
        assert!(item_headers[0].contains("fr"), "{item_headers:?}");
        assert!(!item.observed_url().contains("locale="));
        let overridden = scoped
            .item("42")
            .set_locale_override("it")
            .plan()
            .expect("override plan");
        assert!(
            overridden
                .observed_url()
                .ends_with("items/42?tenant=%3Credacted%3E&locale=it"),
            "{}",
            overridden.observed_url()
        );

        for plan in [&present, &absent, &item, &overridden] {
            let rendered = format!("{plan:?} {:?}", plan.public_headers());
            assert!(!rendered.contains("acme-tenant-sentinel"), "{rendered}");
            assert!(!rendered.contains("fake-bootstrap-sentinel"), "{rendered}");
        }
    }
}

/// V3-L16: acronym/digit/raw identifier transitions derive predictable
/// generated modules, handles and request types without truncation or
/// hidden suffixes.
mod names {
    use concord::{Binary, Bytes, api};

    api! {
        client XMLHttpAPIClient2 {
            base "https://names.example.test/";

            scope oauth2_tokens() {
                GET fetch_v2_items2() -> Bytes via Binary;
                GET r#type() -> Bytes via Binary;
            }

            GET a_long_endpoint_name_that_stays_within_the_operational_limit() -> Bytes via Binary;

            // The derived segment would exceed 63 bytes, so the ID must be
            // explicit ([C3-NAME] otherwise); the generated Rust names are
            // never truncated.
            GET an_overlong_endpoint_name_that_exceeds_the_sixty_three_byte_segment() -> Bytes via Binary {
                id "names.overlong";
            }
        }
    }

    #[test]
    fn acronym_digit_and_raw_identifiers_follow_the_canonical_conversion() {
        let _: fn() -> xml_http_api_client2::XMLHttpAPIClient2Builder = XMLHttpAPIClient2::builder;
        let client = XMLHttpAPIClient2::new().expect("names client");
        let scope: xml_http_api_client2::oauth2_tokens::Oauth2Tokens = client.oauth2_tokens();
        let fetch: xml_http_api_client2::oauth2_tokens::FetchV2Items2Request =
            scope.fetch_v2_items2();
        let raw: xml_http_api_client2::oauth2_tokens::TypeRequest = scope.r#type();
        let long: xml_http_api_client2::ALongEndpointNameThatStaysWithinTheOperationalLimitRequest =
            client.a_long_endpoint_name_that_stays_within_the_operational_limit();
        let overlong: xml_http_api_client2::AnOverlongEndpointNameThatExceedsTheSixtyThreeByteSegmentRequest =
            client.an_overlong_endpoint_name_that_exceeds_the_sixty_three_byte_segment();
        assert_eq!(overlong.description().id(), "names.overlong");
        assert_eq!(
            overlong.description().source_path(),
            "XMLHttpAPIClient2::an_overlong_endpoint_name_that_exceeds_the_sixty_three_byte_segment"
        );
        assert_eq!(
            fetch.description().id(),
            "xml_http_api_client2.oauth2_tokens.fetch_v2_items2"
        );
        assert_eq!(
            raw.description().id(),
            "xml_http_api_client2.oauth2_tokens.type"
        );
        assert_eq!(
            raw.description().source_path(),
            "XMLHttpAPIClient2::oauth2_tokens::type"
        );
        assert_eq!(
            long.description().id(),
            "xml_http_api_client2.a_long_endpoint_name_that_stays_within_the_operational_limit"
        );
        let _: xml_http_api_client2::oauth2_tokens::TypeEndpoint =
            XMLHttpAPIClient2::unbound().oauth2_tokens().r#type();
    }
}
