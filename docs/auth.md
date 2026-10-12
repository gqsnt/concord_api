# Authentication and credential storage

Credentials are private. A `credentials` block declares **sources** (where
material comes from) and `auth` blocks declare **placements** (where it goes on
a request). Private values are applied to the native request only after public
shaping, never appear in plans, errors or observers, and are sent only over
HTTPS (§11).

## Sources

Proactive refresh uses a 60-second skew only when a refresh provider exists.
Valid installed material without a provider remains usable until actual expiry.
A provider result already acquired inside that skew carries its checked version
through admission and bounded freshness restarts; a quota wait still rechecks
actual expiry, rejection, replacement and an already-started proactive refresh.
It does not trigger a new acquisition merely because its valid lifetime is
shorter than the skew.

<!-- concord-source: crates/concord-examples/src/authentication/api.rs#credential_declarations -->
```concord
credentials {
    secret api_key;
    secret bearer_token;
    secret username;
    secret password;
    credential application_key = api_key(secret.api_key);
    credential static_bearer = bearer(secret.bearer_token);
    credential login = basic(secret.username, secret.password);
    credential oauth = custom OAuth2ClientCredentials;
    credential acquired = endpoint login;
    credential custom = custom ExampleProvider;
}
```

| Source | Material | Recovery |
|---|---|---|
| `api_key(secret.x)` | the secret as an API key | none: static material does not refresh |
| `bearer(secret.x)` | the secret as a bearer access token | none |
| `basic(secret.user, secret.pass)` | a Basic username/password pair | none |
| `custom ProviderType` | whatever the `CredentialProvider` acquires | the provider's `RecoveryMode` |
| `endpoint path` | the credential material returned by the named source endpoint | none: never logs in automatically |

- Each `secret NAME;` is a **required** builder input named after the secret and
  taking `SecretString`. Optional static secrets are not supported; use an
  endpoint-backed source (initially empty) or a custom provider that may report
  unavailable.
- A custom provider is a required builder input named after the credential,
  taking the declared provider type.
- An endpoint-backed source starts empty. It names its source endpoint by a
  root-relative path (`endpoint login`, `endpoint accounts::sign_in`), forward
  references included. Missing paths and cycles are rejected. The source
  endpoint must return complete material, not a stream, an `absent on` option or
  a traversal, and it is never called implicitly from another endpoint.

## Placements

<!-- concord-source: crates/concord-examples/src/authentication/api.rs#static_authentication_endpoints -->
```concord
GET login() -> ExampleToken via ExampleTokenCodec;
GET api_key() -> () via Empty { auth { header "X-Api-Key" = application_key; } }
GET token_authorization() -> () via Empty {
    auth { authorization "Token" = application_key; }
}
GET bearer() -> () via Empty { auth { bearer static_bearer; } }
GET basic() -> () via Empty { auth { basic login; } }
```

<!-- concord-source: crates/concord-examples/src/authentication/api.rs#provider_authentication_endpoints -->
```concord
GET o_auth() -> () via Empty { auth { bearer oauth challenge unauthorized; } }
GET endpoint_backed() -> () via Empty { auth { query "session" = acquired; } }
GET provider_backed() -> () via Empty { auth { bearer custom; } }
```

| Placement | Target |
|---|---|
| `bearer name` | `Authorization: Bearer ...` |
| `basic name` | `Authorization: Basic ...` |
| `authorization = name` | the exact `Authorization` value |
| `authorization "Scheme" = name` | `Authorization: Scheme ...` (a validated HTTP token) |
| `header "Name" = name` | a private header |
| `query "name" = name` | a private query parameter |

- `auth { ... }` appends ordered placements to the inherited ones;
  `auth replace { ... }` replaces them; `auth off;` removes them (AUTH-01).
- At most 64 effective placements, with exclusive targets. A private target
  can never also be a public header or query key.
- Roles are type-checked: `api_key`/`header`/`query` placements need
  `SecretCredential` material, `basic` needs the Basic capability, and
  `bearer`/`authorization` need an access-token-like secret.
- Missing required material fails before dispatch; a declared placement is
  never silently omitted.
- A request carrying any placement requires an `https` URL and TLS; otherwise it
  fails before dispatch with a configuration error (`InvalidAuthentication`).

## Challenges

`challenge unauthorized` (401) or `challenge unauthorized_or_forbidden`
(401/403) attributes a rejection to that placement; an omitted challenge is
`never`. At most one effective placement attributes a challenge, and challenge
statuses may not overlap the endpoint's accepted statuses.

An attributed challenge rejects exactly the **installed version actually
applied** to that attempt (AUTH-04). A static source's challenge is final. An
endpoint-backed source never triggers its login endpoint. A custom provider may
refresh under its `RecoveryMode`, and the one automatic auth-recovery replay
still needs a replayable body, a `safe`/`idempotent` effect, remaining caps and
deadline, and a new rate admission: a 401 is never proof that a
non-idempotent write had no effect (RETRY-02).

## Building an authenticated client

<!-- concord-source: crates/concord-examples/src/authentication/usage.rs#oauth2_provider -->
```rust
let oauth = OAuth2ClientCredentials::builder("https://localhost/oauth/token")
    .client_id("example-client")
    .client_secret("example-client-secret")
    .scope("records.read")
    .build()?;
```

<!-- concord-source: crates/concord-examples/src/authentication/usage.rs#authentication_client -->
```rust
let client = AuthenticationApi::builder()
    .api_key(SecretString::try_new("example-api-key")?)
    .bearer_token(SecretString::try_new("example-bearer")?)
    .username(SecretString::try_new("example-user")?)
    .password(SecretString::try_new("example-password")?)
    .oauth(oauth)
    .custom(ExampleProvider)
    .build()?;
```

<!-- concord-source: crates/concord-examples/src/authentication/usage.rs#authenticated_requests -->
```rust
let _: authentication_api::ApiKeyRequest = client.api_key();
let _: authentication_api::TokenAuthorizationRequest = client.token_authorization();
let _: authentication_api::BearerRequest = client.bearer();
let _: authentication_api::BasicRequest = client.basic();
let _: authentication_api::OAuthRequest = client.o_auth();
let _: authentication_api::EndpointBackedRequest = client.endpoint_backed();
let _: authentication_api::ProviderBackedRequest = client.provider_backed();
let _acquisition: StoreRequest = client.login().store_as_acquired();
```

`SecretString::try_new` bounds protected strings to 1 MiB; built-in
single-string material is bounded to 65,536 bytes, Basic usernames to 8,192
bytes (without `:`) and passwords to 32,768 bytes. `Debug`/`Display` never
reveal material; no zeroization is promised unless a material type implements
it.

## Explicit credential storage

An endpoint-backed source is filled only by an explicit
`store_as_<credential>()` on its source endpoint. Ordinary `.await` on the source
endpoint returns the material to the caller and stores nothing.

<!-- concord-source: crates/concord-examples/src/application/construction.rs#authenticate -->
```rust
/// Requests explicit local installation of a new session credential.
///
/// # Errors
///
/// The login exchange or local installation failure, with its truthful
/// contact facts.
pub async fn authenticate(client: &CatalogApi, user: String) -> Result<StoreOutcome, ApiError> {
    client
        .authenticate(LoginInput { user })
        .store_as_session()
        .await
}
```

`StoreRequest` resolves to `StoreOutcome`:

- `StoreOutcome::Installed { version }`: this intent was still current when the
  material decoded; it was installed as a new `CredentialVersion`.
- `StoreOutcome::Superseded`: a newer explicit intent (a later
  `store_as_*`, a `clear_x()` or a `replace_x(..)`) superseded this one; the
  produced material was dropped and is not returned.

Applications must examine the outcome. `Superseded` is not a transport failure
and must not trigger an automatic new login that would undo a logout or a newer
sign-in:

<!-- concord-source: crates/concord-examples/src/application/construction.rs#sign_in -->
```rust
/// Signs in once and reports the outcome without ever logging in again.
///
/// A `Superseded` outcome is returned as-is: automatically repeating the
/// login here would undo the logout or the newer sign-in that superseded it.
///
/// # Errors
///
/// The login exchange or local installation failure.
pub async fn sign_in(client: &CatalogApi, user: String) -> Result<SignIn, ApiError> {
    match authenticate(client, user).await? {
        StoreOutcome::Installed { version } => Ok(SignIn::Installed { version }),
        StoreOutcome::Superseded => Ok(SignIn::Superseded),
    }
}

/// Signs out locally: advances the session intent, removes ready material and
/// supersedes any in-flight explicit acquisition. No network work is done and
/// no remote session is revoked.
///
/// # Errors
///
/// A value-free [`AuthError`] when the local store fails closed.
pub fn sign_out(client: &CatalogApi) -> Result<(), AuthError> {
    client.credentials().clear_session()
}
```

### Two counters, two meanings

Each credential slot has a monotonically increasing **installation intent
epoch** and a separate **installed version**; both are checked and never wrap
(AUTH-03).

1. The first poll of `store_as_x()` reserves a new intent epoch before the
   source endpoint executes, superseding every earlier pending intent. An
   unpolled storage request reserves nothing.
2. Reserving an intent does not clear installed material; requests already in
   flight keep their captured version, subject to a final freshness check.
3. On successful decoding, the material is installed atomically only if this
   epoch is still current, as a new installed version.
4. Otherwise the material is dropped and the outcome is `Superseded`.
5. A failed or cancelled acquisition never rolls back the epoch or revives an
   earlier intent.

`client.credentials().clear_x()` advances the intent (even when empty), removes
ready material and supersedes in-flight acquisition.
`client.credentials().replace_x(material)` advances the intent and installs a
new version, returning it. Both are synchronous, local, return `Result` and do
no network work: they do not revoke a remote session or recall dispatched
requests. Cloned clients share credential stores; independently built clients
do not.

| Events | Result |
|---|---|
| Start A; clear; A succeeds | A `Superseded`; slot empty |
| Start A; manual replace B; A succeeds | A `Superseded`; B remains |
| Start A; start B; A succeeds; B succeeds | A `Superseded`; B installs |
| Start A; start B; B fails; A succeeds | A `Superseded`; any pre-existing ready value remains; no revival |
| Old version V rejected while a newer intent is pending | V may be rejected; the newer intent remains eligible |
| Unpolled A created; clear; then A first polled | A is a new post-clear intent |

This is strict latest-started explicit intent, not first-success-wins. Provider
refresh is single-flight: followers join the current acquisition instead of
reserving new intents, a cancelled follower does not cancel the leader, and a
cancelled leader retires its flight so followers never hang. A failed local
installation after a successful remote response keeps the remote contact and
status facts and reports the validated operational endpoint ID.

Local installation is part of the storage operation's result. The request's
timeout, caps and cancellation signals apply through it: they are decided
immediately before installing (after the material's `expires_at()` has been
read), so a delay or cancellation that happens before installation prevents
it with `FailureStage::CredentialInstall`. Because the caps bound the whole
storage operation, a deadline or cancellation decided there is the result even
when the intent was also superseded (neither installs anything). The
operation's single `OperationFinished` observation follows the complete
storage result: it reports `Success` for both `Installed` and `Superseded`
(the returned `StoreOutcome` tells them apart) and the failure's category
otherwise, with the same operation ID, final status and attempt count as the
returned result. A slow final observer delays delivery but cannot turn an
installed, superseded or failed storage result into another one. Credential
material the operation no longer holds (decoded material that is not
installed, installed material that the installation replaced once nothing else
still uses it, and the credential store itself when the storage request holds
its last reference) is dropped only after that observation, so a `Drop`
implementation that panics cannot misreport the result; its panic then reaches
whoever awaits the storage request.

## Custom providers

The first-party OAuth2 client-credentials provider preserves a positive
`Retry-After` on rejected token responses. Core parses delta seconds or an HTTP
date once at head arrival, ignores malformed or duplicate fields, clamps to the
configured authentication failure cooldown, and subtracts body-acquisition time.
The provider passes the remaining hint to `AuthError`; the credential slot
blocks later acquisitions until that bounded cooldown expires. No automatic
OAuth retry loop is added, and provider response text stays out of errors.

```text
pub trait CredentialProvider: Send + Sync + 'static {
    type Material: CredentialMaterial;
    fn acquire<'a>(&'a self, context: CredentialRequest<'a>)
        -> BoxFuture<'a, Result<Self::Material, AuthError>>;
    fn refresh<'a>(&'a self, current: &'a Self::Material, context: CredentialRequest<'a>)
        -> BoxFuture<'a, Result<Self::Material, AuthError>>;   // default: acquire
    fn recovery_mode(&self) -> RecoveryMode;                   // default: Refresh
    fn http_origins(&self) -> &[ProviderOrigin] { &[] }
}
```

`CredentialRequest` carries the reason (`Missing`, `Expired`, `ExpiringSoon`,
`Rejected`) and an isolated, bounded `ProviderHttp`, never the application
client or other credential slots. `RecoveryMode` is `None`, `InvalidateOnly` or
`Refresh`.

`ProviderHttp` reaches only the exact origins the provider declares in
`http_origins` (AUTH-02): at most 8 by default (`provider_max_origins`, hard
maximum 32), HTTPS only, validated at build. Any other destination is rejected
before transport. It disables redirects and cookies, rejects application
credential placement, bounds request and response bodies (1 MiB each by
default), sends at most 4 requests per acquisition by default (hard maximum 64)
and runs under the remaining caller deadline and the provider timeout (30 s by
default). `OAuth2ClientCredentials` registers its validated token origin. A
provider that owns another HTTP client is trusted application code Concord does
not sandbox. See [Extensions](extensions.md) for a complete provider.

Each provider acquisition or refresh flight is its own operation (EXEC-01): the
leading request allocates a distinct `OperationId` for it and a bounded,
value-free provider journal records its helper sends, last provider status and
its own dispatch knowledge, linked to the parent application operation.
Followers that join the same flight report the same provider identity. The
facts are exposed as `ProviderAcquisitionFacts` through
`ApiError::provider_acquisition()`, `ExecutionMeta::provider_acquisition()` and
the `OperationEvent::ProviderAcquisitionFinished` observer event.

## Recipes

A single private API-key header:

<!-- concord-source: crates/concord-examples/src/recipes/api_key.rs#api_key_recipe -->
```rust
api! {
    client ApiKeyApi {
        base "https://api-key.example.test/";
        timeout 30s;

        credentials {
            secret api_key;
            credential application_key = api_key(secret.api_key);
        }

        GET profile() -> () via Empty {
            auth { header "X-Api-Key" = application_key; }
        }
    }
}

/// Builds one immutable client from bounded protected input.
pub fn build(api_key: impl AsRef<str>) -> Result<ApiKeyApi, Box<dyn std::error::Error>> {
    let client = ApiKeyApi::builder()
        .api_key(SecretString::try_new(api_key)?)
        .build()?;
    Ok(client)
}
```

OAuth2 client credentials with refresh on a 401:

<!-- concord-source: crates/concord-examples/src/recipes/oauth_refresh.rs#oauth_refresh_recipe -->
```rust
api! {
    client OAuthRecordsApi {
        base "https://records.example.test/";
        timeout 30s;

        credentials {
            credential access = custom OAuth2ClientCredentials;
        }

        GET records() -> () via Empty {
            auth { bearer access challenge unauthorized; }
        }
    }
}

/// Builds a client whose clones share one provider-backed credential slot.
pub fn build(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<OAuthRecordsApi, Box<dyn std::error::Error>> {
    let provider = OAuth2ClientCredentials::builder(token_url)
        .client_id(client_id)
        .client_secret(client_secret)
        .scope("records.read")
        .build()?;
    let client = OAuthRecordsApi::builder().access(provider).build()?;
    Ok(client)
}

/// Executes one provider-authenticated operation.
pub async fn fetch(client: &OAuthRecordsApi) -> Result<(), concord::ApiError> {
    client.records().await
}

/// A small application policy decision based only on stable error categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthFailureAction {
    /// Retry later under application-owned finite backoff.
    RetryLater,
    /// Ask the credential owner to repair or rotate configuration.
    RepairCredentials,
    /// Preserve the original failure for ordinary handling.
    Propagate,
}

/// Classifies provider and protected-service authentication failures.
#[must_use]
pub fn failure_action(error: &concord::ApiError) -> OAuthFailureAction {
    match error.category() {
        concord::error::ErrorCategory::AuthenticationProvider
        | concord::error::ErrorCategory::Timeout
        | concord::error::ErrorCategory::Connect => OAuthFailureAction::RetryLater,
        concord::error::ErrorCategory::AuthenticationRejected => {
            OAuthFailureAction::RepairCredentials
        }
        _ => OAuthFailureAction::Propagate,
    }
}
```

The canonical Catalog shows scope-level replacement: the root places the
bootstrap API key, and `scope regional` replaces it with
`auth replace { bearer session challenge unauthorized; }` for the whole scope
([Modeling APIs](modeling_apis.md)).
