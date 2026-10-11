//! A kitchen-sink declaration exercising the complete v3 grammar surface:
//! every attachment family and operation, credentials of every source kind,
//! rate registries with formals, encoders and repeated attachment,
//! representation fences, absence, accepted bytes, streaming, records,
//! bodies, pagination with a tuple cursor, nested qualified references, raw
//! identifiers, deprecated no-op bridges and runtime-binding owners.

use std::num::NonZeroU32;
use std::sync::Arc;

use concord::auth::{
    AuthError, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
    SecretCredential,
};
use concord::codec::NotFound;
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError,
};
use concord::rate_limit::{CanonicalKeyWriter, RateKeyEncoder, RateLimitError};
use concord::{
    AtomEncoder, AtomError, Binary, BoundedAtomWriter, ByteStream, Discard, Empty, Json,
    SecretString, Text, api,
};
use serde::{Deserialize, Serialize};

pub struct ProviderToken(SecretString);
impl CredentialMaterial for ProviderToken {}
impl SecretCredential for ProviderToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

pub struct FakeProvider;
impl CredentialProvider for FakeProvider {
    type Material = ProviderToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            Ok(ProviderToken(
                SecretString::try_new("provider-sentinel").expect("secret"),
            ))
        })
    }

    fn recovery_mode(&self) -> concord::auth::RecoveryMode {
        concord::auth::RecoveryMode::Refresh
    }
}

pub struct Upper;
impl AtomEncoder<String> for Upper {
    fn write(value: &String, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(&value.to_ascii_uppercase())
    }
}

pub struct Lowercase;
impl RateKeyEncoder<String> for Lowercase {
    const ENCODING_ID: &'static str = "example.lowercase/1";
    fn write_canonical(value: &String, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("utf8", value.to_ascii_lowercase().as_bytes())
    }
}

#[derive(Serialize, Clone)]
pub struct Search {
    pub term: String,
}

#[derive(Deserialize, Debug)]
pub struct Row {
    pub id: u32,
}

#[derive(Deserialize)]
pub struct PagedRows {
    pub rows: Vec<Row>,
    pub next_page: Option<u32>,
    pub next_token: Option<String>,
}

pub struct TupleInit {
    pub page: u32,
    pub token: Option<String>,
}

pub struct TuplePages {
    page: u32,
    token: Option<String>,
}

impl PaginationController<PagedRows> for TuplePages {
    type Item = Row;
    type Init = TupleInit;
    type Cursor = (u32, Option<String>);
    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        Ok(Self {
            page: init.page,
            token: init.token,
        })
    }

    fn cursor(&self) -> Self::Cursor {
        (self.page, self.token.clone())
    }

    fn advance(
        self,
        response: PageResponse<PagedRows>,
        _limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Row>, PaginationError> {
        let page = response.into_page();
        match page.next_page {
            None => Ok(PageStep::Done { items: page.rows }),
            Some(next) => Ok(PageStep::Continue {
                next: Self {
                    page: next,
                    token: page.next_token,
                },
                items: page.rows,
            }),
        }
    }
}

pub const API_LEVEL: u8 = 7;

api! {
    /// Grammar coverage client.
    client Grammar {
        base "https://grammar.example.test/base";
        timeout 45s;
        response Json;

        values {
            #[sensitive] header("X-Tenant") tenant: String;
            query("lvl") level: u8 = API_LEVEL;
            #[default] query("tags") each default_tags?: Vec<String>;
            #[deprecated(note = "no longer sent")] legacy?: String;
            r#type: String = "standard";
        }

        credentials {
            secret user;
            secret pass;
            secret token;
            credential basic_login = basic(secret.user, secret.pass);
            credential static_token = bearer(secret.token);
            credential key = api_key(secret.token);
            credential provided = custom FakeProvider;
            credential session = endpoint accounts::sign_in;
        }

        rate_limits {
            windows standard { allow 100 per 1s; allow 1000 per 1m; }
            policy per_region(region, kind) {
                id "grammar.region";
                key [origin, "region" = region via Lowercase, "kind" = kind];
                cost 2;
                using standard;
            }
            policy unused_policy() {
                id "grammar.unused";
                key [origin];
                allow 5 per 1s;
            }
        }

        path "v{level}";
        headers {
            "X-Kind" = concat["kind-", r#type];
            "X-Flag" = true;
            "X-Char" = 'c';
            "X-Num" = -3;
        }

        scope accounts() {
            auth { basic basic_login; }

            POST sign_in(body credentials: Search via Json) -> ProviderToken via SessionJson {
                id "grammar.accounts.sign_in";
                path "sign-in";
                status [200, 201];
            }
        }

        scope region(name: String, #[default] kind: String) {
            subdomain [name];
            path replace "regions/{name}";
            rate_limit per_region(region = name, kind = kind);
            auth replace { authorization "Token" = key; query "sig" = provided challenge unauthorized_or_forbidden; }
            headers { remove "X-Flag"; append "X-Extra" = "1"; }

            scope zone(zone_id: u32) {
                subdomain replace [concat["z", zone_id], region.name];
                timeout 5s;

                GET locate(query each shards: Vec<u32>) -> Row {
                    id "grammar.zone.locate";
                    path "zones/{zone.zone_id}/{region.name via Upper}";
                    rate_limit per_region(region = region.name, kind = region.kind);
                    headers { replace "X-Kind" = client.r#type; }
                }
            }

            METHOD("PURGE") purge(target: String) -> () via Discard {
                id "grammar.region.purge";
                path "cache/{target}";
                effect idempotent;
                status 202..=204;
            }

            GET maybe(id: u32) -> Option<Row> via NotFound<Json> {
                id "grammar.region.maybe";
                path "rows/{id}";
                status [200, 404];
                expose accepted_bytes;
            }

            GET raw() -> ByteStream via Binary {
                path "raw";
                auth off;
            }

            DELETE r#match(id: u32) -> () via Empty {
                path "match/{id}";
                query off;
                headers off;
            }
        }

        scope bare() {
            subdomain off;
            path off;
            query replace { "only" = "this"; }
            auth { bearer static_token; }
        }

        scope texts() {
            request explicit;
            response Text;

            POST echo(body text?: String via Text, query("mode") mode?: String) -> String {
                path "echo";
                query { when_present "lvl" = mode; }
            }

            POST search(body query: Search via Json, page: u32 = 1, token?: String) -> PagedRows via Json {
                path "search";
                effect safe;
                query {
                    "page" = page;
                    "token" = token;
                }
                paginate via TuplePages {
                    init TupleInit { page: page, token: token };
                    cursor -> (page, token);
                }
            }
        }
    }
}

pub struct SessionJson;
impl concord::response::ResponseRepresentation<ProviderToken> for SessionJson {
    type Driver = concord::response::Buffered;
    const ACCEPT: Option<&'static str> = Some("text/plain");

    fn from_body(
        body: concord::Bytes,
        _head: &concord::response::ResponseHead<'_>,
    ) -> Result<ProviderToken, concord::response::ResponseBodyError> {
        let text = String::from_utf8(body.to_vec()).map_err(|_| {
            concord::response::ResponseBodyError::new(
                concord::response::ResponseBodyErrorKind::Decode,
            )
        })?;
        SecretString::try_new(text).map(ProviderToken).map_err(|_| {
            concord::response::ResponseBodyError::new(
                concord::response::ResponseBodyErrorKind::Decode,
            )
        })
    }
}

fn secret(value: &str) -> SecretString {
    SecretString::try_new(value).expect("secret")
}

fn client() -> Grammar {
    Grammar::builder()
        .set_tenant("t-1")
        .user(secret("alice"))
        .pass(secret("pw"))
        .token(secret("tok"))
        .provided(FakeProvider)
        .configure_runtime(|runtime| {
            runtime.rate_limiter(Arc::new(concord::rate_limit::LocalRateLimiter::new()))
        })
        .build()
        .expect("client")
}

#[test]
fn builder_accumulates_every_missing_input() {
    let error = Grammar::builder().build().expect_err("missing inputs");
    let missing: Vec<&str> = error
        .missing_fields()
        .into_iter()
        .map(|(field, _)| field)
        .collect();
    assert_eq!(missing, ["set_tenant", "user", "pass", "token", "provided"]);
}

#[test]
fn routes_hosts_and_programs_compose_as_declared() {
    let client = client();
    let plan = client
        .region("eu")
        .zone(9)
        .locate(vec![1, 2])
        .plan()
        .expect("plan");
    let url = plan.observed_url();
    assert!(
        url.starts_with("https://z9.eu.grammar.example.test/base/regions/eu/zones/9/EU?"),
        "{url}"
    );
    assert!(url.contains("shards=1&shards=2"), "{url}");
    assert!(url.contains("lvl=7"), "{url}");

    let bare = client.bare().plan_runtime(
        &concord::endpoint::RuntimeEndpoint::builder(
            concord::endpoint::EndpointId::new("grammar.runtime.bare").expect("id"),
            concord::http::Method::GET,
        )
        .path("dynamic")
        .response::<Text, String>()
        .build()
        .expect("runtime endpoint"),
    );
    let bare = bare.expect("runtime plan");
    assert!(
        bare.observed_url()
            .starts_with("https://grammar.example.test/base/dynamic?only=this"),
        "{}",
        bare.observed_url()
    );

    let search = client
        .texts()
        .search(Search {
            term: "x".to_owned(),
        })
        .set_token("abc")
        .plan()
        .expect("plan");
    // `token` matches the mandatory name-based redaction rule (PRIVACY-01).
    assert!(
        search
            .observed_url()
            .contains("page=1&token=%3Credacted%3E"),
        "{}",
        search.observed_url()
    );

    let contract = Grammar::contract();
    assert!(contract.endpoint("grammar.zone.locate").is_some());
    assert_eq!(
        Grammar::unbound()
            .region("eu")
            .zone(1)
            .locate(Vec::new())
            .description()
            .endpoint_id()
            .as_str(),
        "grammar.zone.locate"
    );
    let _ = NonZeroU32::MIN;
}
