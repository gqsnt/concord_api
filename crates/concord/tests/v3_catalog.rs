//! Canonical extended v3 Catalog vertical fixture (§22) compiled against the
//! facade, with the §17.2 downstream cursor-envelope controller.

use std::fmt;
use std::num::NonZeroU32;

use concord::auth::{CredentialMaterial, SecretCredential};
use concord::pagination::{
    OffsetInit, OffsetPagination, PageDecisionLimits, PageResponse, PageStep, PaginationController,
    PaginationDescription, PaginationError, PaginationErrorKind,
};
use concord::{
    AtomError, Binary, BoundedAtomWriter, ByteStream, Json, RequestAtom, SecretString, api,
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
pub struct ItemSummary {
    pub id: String,
}
#[derive(Deserialize)]
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

fn client() -> CatalogApi {
    CatalogApi::builder()
        .set_tenant("acme")
        .bootstrap_key(SecretString::try_new("fake-bootstrap-sentinel").expect("secret"))
        .configure_runtime(|runtime| {
            runtime.rate_limiter(std::sync::Arc::new(
                concord::rate_limit::LocalRateLimiter::new(),
            ))
        })
        .build()
        .expect("client")
}

#[test]
fn catalog_plans_follow_the_selected_meaning() {
    let client = client();
    let regional = client.regional("eu").set_language("fr");
    let plan = regional
        .item_summaries(vec!["repairable".to_owned(), "used".to_owned()])
        .set_category("hardware")
        .plan()
        .expect("plan");
    let url = plan.observed_url().to_owned();
    assert!(
        url.starts_with("https://eu.api.catalog.example.test/root/v2/regions/eu/items?"),
        "{url}"
    );
    assert!(url.contains("category=hardware"), "{url}");
    assert!(url.contains("offset=0"), "{url}");
    assert!(url.contains("count=25"), "{url}");
    assert!(url.contains("tag=repairable&tag=used"), "{url}");
    assert!(
        !url.contains("acme"),
        "sensitive tenant must be withheld: {url}"
    );

    let inspect = client
        .regional("eu")
        .set_language("fr")
        .inspect()
        .plan()
        .expect("plan");
    let url = inspect.observed_url().to_owned();
    assert!(!url.contains("languageEcho"), "{url}");
    let headers = format!("{:?}", inspect.public_headers());
    assert!(
        !headers.to_ascii_lowercase().contains("accept-language: fr"),
        "replace clears inherited: {headers}"
    );

    let contract = CatalogApi::contract();
    assert_eq!(contract.endpoints().len(), 8);
    assert!(contract.endpoint("catalog.item.update").is_some());
}
