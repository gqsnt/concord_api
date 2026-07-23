# Frozen target public contract

This Phase 0 design fixture records the compile targets for the Concord refactor.
[`CONCORD_REFACTOR_REFERENCE.md`](../../../CONCORD_REFACTOR_REFERENCE.md), especially
Sections 2, 5, 6, and 17, is authoritative if this derived fixture ever disagrees.

These snippets are intentionally not compiled by the starting implementation. They
must become ordinary façade compile/runtime tests during Phases 6–9. Keeping them
beside the compiler tests prevents intermediate implementation details from replacing
the selected public contract.

## Minimal

```rust,ignore
use concord::{api, Json};

api! {
    client UsersApi {
        base "https://api.example.com/v1/";
    }

    GET get_user(id: UserId) -> Json<User> {
        id "users.get";
        path ["users", ep.id];
    }
}

let api = UsersApi::new()?;
let user: User = api.get_user(UserId(42)).await?;
let response = api
    .get_user(UserId(43))
    .timeout(Duration::from_secs(2))
    .response()
    .await?;

let _: users_api::GetUserRequest = api.get_user(UserId(44));
```

## Product-scale scopes

```rust,ignore
use concord::{api, Json};

api! {
    client RiotApi {
        base "https://riotgames.com/";
        vars {
            platform_route: PlatformRoute = PlatformRoute::Euw1;
            regional_route: RegionalRoute = RegionalRoute::Europe;
        }
        secrets {
            api_key;
        }
        credentials {
            credential riot_key = api_key(secret.api_key);
        }
        auth header "X-Riot-Token" = riot_key challenge never;
        timeout 20s;
    }

    scope platform() {
        host [vars.platform_route, "api"];
        path ["lol"];
        rate_limit {
            20 / 1s;
            100 / 2m;
        }

        scope league_v4() {
            path ["league", "v4"];

            GET league_entries(
                queue: LeagueQueue,
                tier: LeagueTier,
                division: LeagueDivision,
                page: u32 = 1,
            ) -> Json<Vec<LeagueEntryDto>> {
                id "riot.league_v4.league_entries";
                query { "page" = ep.page; }
                path ["entries", ep.queue, ep.tier, ep.division];
                rate_limit { 50 / 10s; }
            }
        }
    }
}

let riot = RiotApi::builder()
    .platform_route(PlatformRoute::Euw1)
    .regional_route(RegionalRoute::Europe)
    .api_key(api_key)
    .build()?;

let request: riot_api::platform::league_v4::LeagueEntriesRequest = riot
    .platform()
    .league_v4()
    .league_entries(queue, tier, division)
    .page(2);
```

## Authentication and provider construction

```rust,ignore
use concord::{api, Json};
use concord::auth::OAuth2ClientCredentials;

api! {
    client AuthApi {
        base "https://service.example.com/";
        secrets {
            api_key;
        }
        credentials {
            credential application_key = api_key(secret.api_key);
            credential session = endpoint login;
            credential oauth = custom OAuth2ClientCredentials;
        }
    }

    POST login(#[body] input: Json<LoginRequest>) -> SessionTokenCodec;

    GET profile() -> Json<Profile> {
        auth bearer session challenge unauthorized;
    }

    GET oauth_profile() -> Json<Profile> {
        auth bearer oauth challenge unauthorized;
    }
}

let oauth = OAuth2ClientCredentials::builder(token_url)
    .client_id(client_id)
    .client_secret(client_secret)
    .scope("profile.read")
    .build()?;

let api = AuthApi::builder()
    .api_key(api_key)
    .oauth_provider(oauth)
    .build()?;

api.login(LoginRequest { username, password })
    .store_as_session()
    .await?;
```

## Pagination capability

```rust,ignore
use concord::{api, Json, PageLimit};
use concord::pagination::OffsetPagination;

api! {
    client DirectoryApi {
        base "https://directory.example.com/v1/";
    }

    GET list_users(offset: u64, limit: NonZeroU32) -> Json<Vec<User>> {
        path ["users"];
        query {
            "offset" = ep.offset;
            "limit" = ep.limit;
        }
        paginate OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }

    GET get_user(id: UserId) -> Json<User> {
        path ["users", ep.id];
    }
}

let users = api
    .list_users(0, NonZeroU32::new(100).unwrap())
    .paginate(PageLimit::hard_cap(NonZeroU32::new(20).unwrap()))
    .await?;

// Compile-fail target: GetUserRequest has no `paginate`.
api.get_user(UserId(42)).paginate(PageLimit::take(NonZeroU32::new(2).unwrap()));
```

## Custom semantic specifications

```rust,ignore
use concord::body::{
    BodyError, RequestBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
};
use concord::response::{
    Buffered, ResponseBodyError, ResponseBodySpec, ResponseHead,
};

pub struct AuditBody;

impl RequestBodySpec for AuditBody {
    type Input = AuditEnvelope;

    fn metadata(
        input: &Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        todo!()
    }

    fn prepare(
        input: Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        todo!()
    }
}

pub struct AuditResponse;

impl ResponseBodySpec for AuditResponse {
    type Output = AuditResult;
    type Mode = Buffered;

    fn from_body(
        body: concord::Bytes,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        todo!()
    }
}

api! {
    client AuditApi {
        base "https://audit.example.com/";
    }

    POST submit_audit(#[body] envelope: AuditBody) -> AuditResponse {
        path ["audits"];
    }
}

let result: AuditResult = api.submit_audit(envelope).await?;
```

## Runtime endpoint

```rust,ignore
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

let endpoint = RuntimeEndpoint::builder(
    EndpointId::new("catalog.search")?,
    Method::GET,
)
.path_segment("search")
.query("q", query)
.query_optional("cursor", cursor)
.response::<Json<SearchResults>>()
.build()?;

let results: SearchResults = api.request(endpoint).await?;
```

The timeout representation must preserve all three states:

```text
InheritClient
Explicit(Duration)
Disabled
```

## Explicit endpoint and generic composition

```rust,ignore
use users_api::GetUser;

let endpoint = GetUser::new(UserId(42)).expand(Expand::Teams);
assert_eq!(endpoint.description().id(), "users.get");

let plan = api.plan(&endpoint)?;
assert_eq!(
    plan.safe_url(),
    "https://api.example.com/v1/users/<redacted>",
);

let first: User = api.request(endpoint.clone()).await?;
let second = api.request(endpoint).response().await?;

fn bind_endpoint<E>(api: &UsersApi, endpoint: E) -> E::Request
where
    E: users_api::Endpoint,
{
    api.request(endpoint)
}
```

Planning must not invoke hooks, credentials, limiter work, body metadata or
materialization, body factories, stream polling, native construction, transport, or
completion.
