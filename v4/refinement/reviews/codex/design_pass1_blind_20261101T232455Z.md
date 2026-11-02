**Reasoning-only report.** Everything below is a design proposal or predicted consequence. I did not compile or execute Rust, run tests, make HTTP requests, collect measurements, or inspect implementation source.

**1. Observed configuration and independence**

I cannot observe the effective backend model identifier or reasoning-effort setting. The visible instructions describe me as Codex, based on GPT-6. They do not independently confirm **GPT-6.1 Sol / xhigh**. The requested configuration is therefore **unverified**; I cannot honestly report either a confirmed match or a confirmed backend mismatch.

I read exactly:

- `01_CHARTER.md`
- `USER_TASKS.md`
- `CONTRACT_CASES.md`

I did not inspect other paths, repositories, implementation files, decision logs, registries, or network sources. I changed nothing.

I have no existing Concord implementation or coordinator-preferred design in context. My knowledge of the existing project is limited to what these materials disclose: an unpublished V4 alpha exists, refinement should continue it, and its syntax and implementation are revisable. The proposed syntax below is independent reasoning, **not a description of that alpha**.

**2. Eight decisions that most determine desirability**

| Decision | Smallest exposing counterexample | Recommendation |
|---|---|---|
| Root inputs are named, not positional. | `new(http, account_id, project_id)` accepts an accidental swap when both are `String`. | One public required-input record per root; explicitly supply the native client. |
| Payload type, placement presence, and inheritance are different concepts. | A required `Option<Patch>` containing `None` may mean JSON `null`; an absent optional body means no body. | Declare optional placement explicitly. Never recognize optionality from a type’s spelling. |
| Defaults have one documented evaluation point and reset meaning. | A counter-valued default changes between two preparations, or “reset” unexpectedly generates another value. | Capture defaults at owner selection; retain the initial value; reset restores it. |
| Conditional override and exact replacement differ. | Parent has `tag=a`; child has an absent optional override versus a present empty tag collection. | Absent conditional override inherits; present empty replacement clears. Provide explicit removal. |
| Representation choice is explicit, with lexical defaults. | `String` could mean JSON `"hello"` or text `hello`; `()` could mean JSON `null` or a drained response. | Separate Rust output type from representation. Let authors establish body/success defaults. |
| Wire formatting is chosen per use. | A timestamp’s human-facing `Display` becomes “Yesterday”; the API needs RFC 3339. | Explicit adapters, including an explicit `display` adapter. No serialization guessing. |
| Snapshot reuse must not require cloning every payload. | A selected endpoint contains a non-`Clone` DTO or caller-owned context; cloning the selection should not clone its contents. | Share retained values; make snapshot and auto-trait consequences explicit. |
| Safe errors must preserve useful ownership without forwarding raw formatting. | A redirect error contains a secret query value; `Error::source()` exposes it despite a redacted outer message. | Safe classification outside; unread response and raw causes behind explicit access. |

The first four determine whether callers can predict the request. The last four determine whether the abstraction remains useful once the endpoint stops being trivial.

**3. Proposed authoring and calling surface**

The basic concepts are:

- `value x: T`: a required payload, unless an initializer supplies its initial value.
- `optional x: T`: an optional placement containing a `T`; storage presence is `Option<T>`.
- Named input records contain only fields callers must supply.
- Consuming setters return a new selection.
- Scope navigation borrows the parent and captures a structural snapshot.
- `prepare()` renders an owned native request without executing it.
- `send()` prepares and executes through the supplied native client.
- Native exit transfers the request **and its decoder**.

No auth, admission, retry, or pagination machinery participates.

The complete proposed regional fixture follows. Identifiers such as `json`, `raw`, `utf8`, and `decimal` are proposed built-ins. `using some_path` can also name an ordinary adapter or representation type.

```rust
use std::sync::Arc;

#[derive(serde::Serialize)]
pub struct SearchRequest {
    pub text: String,
}

#[derive(serde::Deserialize)]
pub struct ProductRecord {
    pub id: String,
    pub name: String,
}

#[derive(serde::Deserialize)]
pub struct SearchPage {
    pub items: Vec<ProductRecord>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ProductPatch {
    pub name: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct ApiProblem {
    pub code: String,
    pub message: String,
}

// Ordinary application data. Concord supplies no lifecycle for it.
pub struct CallContext {
    pub operation_id: String,
}

concord::client! {
    pub client Catalog {
        value region: String;

        #[sensitive]
        optional api_key: String;

        optional locale: String = Some("en".to_owned());

        // Legitimate typed storage, with no fabricated wire placement.
        optional context: Arc<CallContext>;

        base "https://catalog.example.test";
        host prepend root.region using dns_label;
        path "v1";

        header "x-api-key" <- root.api_key using utf8;
        query "locale" <- root.locale using utf8;

        default body using json;
        default response using json;
        error ApiProblem using json;

        pub scope tenant: Tenant {
            value tenant_id: String;

            // Explicitly allow this scope to shadow this inherited value.
            override root.locale;

            path "tenants";
            segment self.tenant_id using utf8;

            pub endpoint search: Search {
                method POST;
                path "search";

                value body: SearchRequest;
                optional category: String;
                value page_size: u16 = 50;
                value tags: Vec<String> = Vec::new();
                optional cursor: String;

                query "category" <- self.category using utf8;
                query "page_size" <- self.page_size using decimal;
                query each "tag" <- self.tags using utf8;
                query "cursor" <- self.cursor using utf8;

                body <- self.body;
                response SearchPage;
            }

            pub scope product: ProductScope {
                value product_id: String;

                path "products";
                segment self.product_id using utf8;

                pub endpoint get: GetProduct {
                    method GET;
                    response ProductRecord;
                }

                pub endpoint update: UpdateProduct {
                    method PATCH;

                    value body: ProductPatch;

                    body <- self.body;
                    response ProductRecord;
                }

                pub endpoint download: Download {
                    method GET;
                    path "download";

                    response reqwest::Response using raw;
                }
            }
        }
    }
}
```

`optional context` is deliberately ordinary typed storage. It does not create a registry, service container, callback system, or automatic context propagation.

The default search request is predicted to target:

```text
POST https://eu.catalog.example.test/v1/tenants/acme/search
     ?locale=en&page_size=50
```

Its body is JSON. Tags repeat in supplied order. An absent category contributes nothing; an empty category contributes `category=`.

The product identifier `sku/42` is one dynamic segment:

```text
/v1/tenants/acme/products/sku%2F42
```

It cannot introduce another path level.

**Generated signature inventory**

These are proposed signatures; semicolons stand for generated implementations.

```rust
pub struct CatalogInputs {
    pub region: String,
}

pub struct TenantInputs {
    pub tenant_id: String,
}

pub struct ProductScopeInputs {
    pub product_id: String,
}

pub struct SearchInputs {
    pub body: SearchRequest,
}

pub struct UpdateProductInputs {
    pub body: ProductPatch,
}
```

Construction captures values and evaluates root defaults. Wire conversion and runtime wire validation happen during preparation.

```rust
impl Catalog {
    pub fn new(
        http: reqwest::Client,
        input: CatalogInputs,
    ) -> Self;

    pub fn region(self, value: String) -> Self;
    pub fn region_value(&self) -> &String;
    pub fn reset_region(self) -> Self;

    pub fn api_key(self, value: String) -> Self;
    pub fn set_api_key(self, value: Option<String>) -> Self;
    pub fn api_key_value(&self) -> Option<&String>;
    pub fn reset_api_key(self) -> Self;

    pub fn locale(self, value: String) -> Self;
    pub fn set_locale(self, value: Option<String>) -> Self;
    pub fn locale_value(&self) -> Option<&String>;
    pub fn reset_locale(self) -> Self;

    pub fn context(self, value: Arc<CallContext>) -> Self;
    pub fn set_context(self, value: Option<Arc<CallContext>>) -> Self;
    pub fn context_value(&self) -> Option<&Arc<CallContext>>;
    pub fn reset_context(self) -> Self;

    // Replaces the declared base and host composition as a whole.
    // All declared path directives, including "v1", still append.
    pub fn with_resolved_base(self, base: reqwest::Url) -> Self;
    pub fn reset_base(self) -> Self;

    pub fn tenant(&self, input: TenantInputs) -> Tenant;
}

impl Tenant {
    pub fn parent(&self) -> &Catalog;

    pub fn tenant_id(self, value: String) -> Self;
    pub fn tenant_id_value(&self) -> &String;
    pub fn reset_tenant_id(self) -> Self;

    // These edit the explicitly declared inherited override.
    pub fn locale(self, value: String) -> Self;
    pub fn set_locale(self, value: Option<String>) -> Self;
    pub fn inherit_locale(self) -> Self;
    pub fn locale_value(&self) -> Option<&String>;

    pub fn search(&self, input: SearchInputs) -> Search;
    pub fn product(&self, input: ProductScopeInputs) -> ProductScope;
}

impl ProductScope {
    pub fn parent(&self) -> &Tenant;

    pub fn product_id(self, value: String) -> Self;
    pub fn product_id_value(&self) -> &String;
    pub fn reset_product_id(self) -> Self;

    pub fn get(&self) -> GetProduct;
    pub fn update(&self, input: UpdateProductInputs) -> UpdateProduct;
    pub fn download(&self) -> Download;
}
```

Only an explicit inherited-override declaration generates inherited editing methods. This avoids giving every endpoint setters for every ancestor field.

```rust
impl Search {
    pub fn parent(&self) -> &Tenant;

    pub fn body(self, value: SearchRequest) -> Self;
    pub fn body_value(&self) -> &SearchRequest;
    pub fn reset_body(self) -> Self;

    pub fn category(self, value: String) -> Self;
    pub fn set_category(self, value: Option<String>) -> Self;
    pub fn category_value(&self) -> Option<&String>;
    pub fn reset_category(self) -> Self;

    pub fn page_size(self, value: u16) -> Self;
    pub fn page_size_value(&self) -> &u16;
    pub fn reset_page_size(self) -> Self;

    pub fn tags(self, value: Vec<String>) -> Self;
    pub fn tags_value(&self) -> &Vec<String>;
    pub fn reset_tags(self) -> Self;

    pub fn cursor(self, value: String) -> Self;
    pub fn set_cursor(self, value: Option<String>) -> Self;
    pub fn cursor_value(&self) -> Option<&String>;
    pub fn reset_cursor(self) -> Self;

    pub fn prepare(
        &self,
    ) -> Result<Prepared<SearchPage, ApiProblem>, BuildError>;

    pub fn send(
        &self,
    ) -> impl std::future::Future<
        Output = Result<SearchPage, CallError<ApiProblem>>,
    > + Send + 'static;
}

impl UpdateProduct {
    pub fn parent(&self) -> &ProductScope;

    pub fn body(self, value: ProductPatch) -> Self;
    pub fn body_value(&self) -> &ProductPatch;
    pub fn reset_body(self) -> Self;

    pub fn prepare(
        &self,
    ) -> Result<Prepared<ProductRecord, ApiProblem>, BuildError>;

    pub fn send(
        &self,
    ) -> impl std::future::Future<
        Output = Result<ProductRecord, CallError<ApiProblem>>,
    > + Send + 'static;
}

impl GetProduct {
    pub fn parent(&self) -> &ProductScope;

    pub fn prepare(
        &self,
    ) -> Result<Prepared<ProductRecord, ApiProblem>, BuildError>;

    pub fn send(
        &self,
    ) -> impl std::future::Future<
        Output = Result<ProductRecord, CallError<ApiProblem>>,
    > + Send + 'static;
}

impl Download {
    pub fn parent(&self) -> &ProductScope;

    pub fn prepare(
        &self,
    ) -> Result<Prepared<reqwest::Response, ApiProblem>, BuildError>;

    pub fn send(
        &self,
    ) -> impl std::future::Future<
        Output = Result<reqwest::Response, CallError<ApiProblem>>,
    > + Send + 'static;
}
```

For this fixture, the returned futures are owned, `Send`, and `'static`. `send()` performs preparation synchronously and returns a future holding the prepared result; it does not borrow the selection through execution. This permits ordinary non-wire data to stay outside the execution future.

That timing is observable: custom encoding can run when `send()` is called, before its future is polled. Concord performs no network I/O during that work.

Every generated selection implements `Clone` by sharing retained values, without requiring these DTOs to implement `Clone`. Prepared requests and native bodies have no unconditional `Clone` promise.

```rust
pub struct Prepared<O, E> {
    /* private */
}

impl<O: 'static, E: 'static> Prepared<O, E> {
    // Explicit access to unprotected native data.
    pub fn request(&self) -> &reqwest::Request;

    pub fn into_parts(
        self,
    ) -> (
        reqwest::Client,
        reqwest::Request,
        ResponseDecoder<O, E>,
    );

    pub fn send(
        self,
    ) -> impl std::future::Future<
        Output = Result<O, CallError<E>>,
    > + Send + 'static;
}

pub struct ResponseDecoder<O, E> {
    /* private */
}

impl<O: 'static, E: 'static> ResponseDecoder<O, E> {
    pub fn decode(
        self,
        response: reqwest::Response,
    ) -> impl std::future::Future<
        Output = Result<O, CallError<E>>,
    > + Send + 'static;

    // Preserves safe error conversion around native execution.
    pub fn finish(
        self,
        result: Result<reqwest::Response, reqwest::Error>,
    ) -> impl std::future::Future<
        Output = Result<O, CallError<E>>,
    > + Send + 'static;
}
```

The error boundary preserves a non-2xx response without reading its body:

```rust
pub enum CallError<E> {
    Build(BuildError),
    Transport(TransportError),
    Status(ErrorResponse<E>),
    Decode(DecodeError),
}

pub struct ErrorResponse<E> {
    /* unread response plus selected error decoder */
}

impl<E: 'static> ErrorResponse<E> {
    pub fn status(&self) -> reqwest::StatusCode;

    // Native data access; values are not redacted.
    pub fn headers(&self) -> &reqwest::header::HeaderMap;

    pub fn decode(
        self,
    ) -> impl std::future::Future<
        Output = Result<E, DecodeError>,
    > + Send + 'static;

    pub fn into_parts(
        self,
    ) -> (reqwest::Response, ErrorDecoder<E>);

    pub fn into_response(self) -> reqwest::Response;
}

pub struct ErrorDecoder<E> {
    /* private */
}

impl<E: 'static> ErrorDecoder<E> {
    pub fn decode(
        self,
        response: reqwest::Response,
    ) -> impl std::future::Future<
        Output = Result<E, DecodeError>,
    > + Send + 'static;
}
```

`BuildError`, `TransportError`, and `DecodeError` expose safe kind/stage information and symbolic declaration sites. Built-in decoders distinguish body-read failure from invalid representation.

All Concord error `Display`, `Debug`, and `Error::source()` behavior stays within that safe boundary. A raw underlying cause is available only through an explicitly documented accessor such as `into_raw_cause()`. It is not silently forwarded through `source()`.

The minimal representation extension is two roles:

```rust
pub type BoxError =
    Box<dyn std::error::Error + Send + Sync + 'static>;

pub struct EncodedBody {
    pub body: reqwest::Body,
    pub content_type: Option<reqwest::header::HeaderValue>,
}

pub trait Encode<T: ?Sized> {
    fn encode(value: &T) -> Result<EncodedBody, BoxError>;
}

pub trait Decode<T> {
    fn decode(
        response: reqwest::Response,
    ) -> impl std::future::Future<
        Output = Result<T, DecodeError>,
    > + Send;
}
```

Custom decoders construct safe `DecodeError` values using read/representation constructors. Custom causes remain opaque in protected formatting.

Representations receive no executor, registry, retry context, or lifecycle hook. A representation that needs to consume an existing move-only native body should use the native request route instead of hiding consumption behind `&T`.

**Caller examples**

These fragments are reasoning examples under the proposed API. An enclosing async function can return `Result<(), Box<dyn std::error::Error>>`.

Shared setup:

```rust
fn search_input() -> SearchInputs {
    SearchInputs {
        body: SearchRequest {
            text: "lamp".to_owned(),
        },
    }
}

let root = Catalog::new(
    http.clone(),
    CatalogInputs {
        region: "eu".to_owned(),
    },
)
.api_key("fixture-secret".to_owned());

let tenant = root.tenant(TenantInputs {
    tenant_id: "acme".to_owned(),
});

let product = tenant.product(ProductScopeInputs {
    product_id: "sku/42".to_owned(),
});
```

**U03 — roots with 0, 1, 3, and 8 required inputs**

Zero domain inputs means no generated input argument. One or more means one named record.

```rust
// Generated constructor shapes:
Zero::new(http: reqwest::Client) -> Zero;

One::new(
    http: reqwest::Client,
    input: OneInputs,
) -> One;

Three::new(
    http: reqwest::Client,
    input: ThreeInputs,
) -> Three;

Eight::new(
    http: reqwest::Client,
    input: EightInputs,
) -> Eight;
```

For this constructor experiment, the records correspond directly to required `value` declarations:

```rust
pub struct OneInputs {
    pub region: String,
}

pub struct ThreeInputs {
    pub region: String,
    pub account_id: String,
    pub project_id: String,
}

pub struct EightInputs {
    pub region: String,
    pub account_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub environment: String,
    pub application_id: String,
    pub organization_id: String,
    pub device_id: String,
}
```

```rust
let zero = Zero::new(http.clone());

let one = One::new(
    http.clone(),
    OneInputs {
        region: "eu".to_owned(),
    },
);

let three = Three::new(
    http.clone(),
    ThreeInputs {
        region: "eu".to_owned(),
        account_id: "acct-7".to_owned(),
        project_id: "proj-9".to_owned(),
    },
);

let eight = Eight::new(
    http.clone(),
    EightInputs {
        region: "eu".to_owned(),
        account_id: "acct-7".to_owned(),
        project_id: "proj-9".to_owned(),
        workspace_id: "work-2".to_owned(),
        environment: "staging".to_owned(),
        application_id: "app-3".to_owned(),
        organization_id: "org-4".to_owned(),
        device_id: "device-5".to_owned(),
    },
);
```

Missing fields fail at the input record. Adding an optional or defaulted field does not enlarge that record. Adding a required field intentionally requires a caller edit.

The zero-input exception avoids an otherwise pointless unit input object. Its cost is a documented constructor-shape distinction at zero versus nonzero required inputs.

**U04 — same-typed identities**

Named fields remove positional ambiguity:

```rust
let input = ThreeInputs {
    region,
    account_id,
    project_id,
};
```

They cannot catch this:

```rust
let input = ThreeInputs {
    region,
    account_id: project_id,
    project_id: account_id,
};
```

Those assignments remain type-correct because the types are identical. No constructor syntax can infer the intended identities.

If compiler rejection is required, the author must distinguish the domain types:

```rust
pub struct AccountId(pub String);
pub struct ProjectId(pub String);

pub struct IdentityInputs {
    pub account_id: AccountId,
    pub project_id: ProjectId,
}
```

This is ordinary Rust, not a Concord identity subsystem. U04 should explicitly record the remaining same-type risk rather than claim naming eliminates it.

**U06 — absent, present, empty, and exact `None`**

```rust
let absent = tenant.search(search_input());

let present = absent.clone()
    .category("lighting".to_owned());

let empty = absent.clone()
    .category(String::new());

let cleared = present.clone()
    .set_category(None);
```

Predicted category contributions:

| Selection | Contribution |
|---|---|
| `absent` | No category pair |
| `present` | `category=lighting` |
| `empty` | `category=` |
| `cleared` | No category pair |

`set_category(None)` has a concrete parameter type, so no generic `None` inference workaround is needed.

**U08 — override, clone, and prepare twice**

```rust
let selected = tenant.search(search_input())
    .page_size(200)
    .tags(vec!["red".to_owned(), "new".to_owned()]);

let copied = selected.clone();

let first = selected.prepare()?;
let second = copied.prepare()?;

let reset = copied.reset_page_size();
```

Predicted behavior:

- The page-size initializer ran once when `search(...)` created the selection.
- It ran even though `.page_size(200)` immediately replaced its value.
- Both prepared requests use `200`.
- Cloning and preparation do not reevaluate the initializer.
- `reset` uses the captured initial value, `50`.
- JSON encoding and live wire formatting run for each preparation; their results are not cached.

**U09 — a child survives a later parent setter**

```rust
let parent = root.clone().locale("en-GB".to_owned());

let old_child = parent.tenant(TenantInputs {
    tenant_id: "acme".to_owned(),
});

let changed_parent = parent.locale("fr-FR".to_owned());

let new_child = changed_parent.tenant(TenantInputs {
    tenant_id: "acme".to_owned(),
});

let old_request = old_child.search(search_input()).prepare()?;
let new_request = new_child.search(search_input()).prepare()?;
```

The old request uses `en-GB`; the new request uses `fr-FR`.

This is a structural snapshot. If a stored payload contains caller-owned interior mutability, both snapshots can still observe mutations inside that payload.

**U10 — inherit, override, remove, and reintroduce**

Starting with the original `tenant`, whose captured parent locale is `en`:

```rust
let inherited = tenant.clone();

let overridden = inherited.clone()
    .locale("fr".to_owned());

let removed = overridden.clone()
    .set_locale(None);

let reintroduced = removed.clone()
    .locale("de".to_owned());

let fallback = reintroduced.clone()
    .inherit_locale();
```

The effective values are respectively `en`, `fr`, absent, `de`, and `en`.

A conditional application override remains ordinary Rust:

```rust
let configured = match application_locale {
    Some(value) => tenant.clone().locale(value),
    None => tenant.clone(),
};
```

Here `None` means “do not change this selection” because the application chose not to call a setter. It does not change the meaning of `set_locale(None)`, which means exact absence.

`inherit_locale()` returns to the **captured parent**, not a subsequently modified root.

**U14 — recover an unread non-2xx response and decode its document**

```rust
let update = product.update(UpdateProductInputs {
    body: ProductPatch {
        name: Some("Desk lamp".to_owned()),
    },
});

match update.send().await {
    Ok(updated) => {
        let _updated = updated;
    }

    Err(CallError::Status(error)) => {
        let (response, decoder) = error.into_parts();

        let status = response.status();
        let request_id = response.headers()
            .get("x-request-id")
            .cloned();

        // No body read has occurred before this explicit decode.
        let problem: ApiProblem = decoder.decode(response).await?;

        let _application_owned_data = (status, request_id, problem);
    }

    Err(other) => return Err(Box::new(other)),
}
```

If the body is not the declared JSON document, decoding fails separately. The original status error must not have consumed it speculatively.

The simpler route is `error.decode().await`; the native ownership route remains available.

**U15 — modify the native request and retain decoding**

```rust
let prepared = tenant.search(search_input()).prepare()?;

let (http, mut request, decoder) = prepared.into_parts();

let inspected_method = request.method().clone();
let inspected_url = request.url().clone();

*request.timeout_mut() =
    Some(std::time::Duration::from_secs(2));

request.headers_mut().insert(
    "x-audit",
    reqwest::header::HeaderValue::from_static("yes"),
);

let page: SearchPage = decoder
    .finish(http.execute(request).await)
    .await?;

let _application_owned_data =
    (inspected_method, inspected_url, page);
```

The decoder is preserved through the native edit and execution. `finish` converts native transport failure into the protected error boundary.

Request inspection exposes native data, including secrets. Concord must document that boundary directly.

**4. Choices, serious alternatives, and semantic tables**

| Area | Proposed choice | Best serious alternative | Trade-off |
|---|---|---|---|
| Root construction | Explicit native client plus named required-input record. | Named fluent builder with a final build step; typestate if compile-time completeness is essential. | Records use ordinary field diagnostics and avoid typestate growth. Builders can configure defaults before evaluating them, but add stages and generated API. |
| Optional controls | `x(T)` means present; `set_x(Option<T>)` means exact presence. Inherited overrides additionally have `inherit_x()`. | One explicit patch argument: `Override<Option<T>>`, with `Inherit` and `Replace(...)`. | Methods read better in ordinary calls; the enum is more explicit when passing policies as data and produces fewer generated methods. |
| Default evaluation/reset | Evaluate once when the owner is selected; reset restores the captured initial choice. | Draft/finalize construction, evaluating only unchosen defaults at finalization. | Chosen semantics are easy to retain and explain. They perform overridden work and retain reset baselines. |
| Inherited override/replacement | Named value override is exact; HTTP conditional override and exact replacement are distinct operators. | Expose one explicit contribution action enum everywhere. | Declarative operators make common composition readable. An action enum centralizes semantics but exposes more machinery to callers. |
| Representations/defaults | Lexical body and success defaults; explicit type at every endpoint; explicit exceptions. | Spell the representation at every body and response. | Defaults help the many-JSON case. Explicit spelling is easier to trace locally and reduces ambient context. |
| Wire formatting | Explicit per-placement adapter; `display` is opt-in. | `Display` by default, with adapters only for exceptions. | Explicit adapters avoid silent wire changes and orphan problems, at the cost of declaration ceremony and formatting allocations. |
| Clone/ownership/Send | Share retained owned values; clone handles; retain normal auto traits. | Own values directly and make selection `Clone` conditional on payload `Clone`; callers explicitly use `Arc<T>`. | Sharing handles non-`Clone` snapshots. Direct ownership makes sharing and destruction more visible and can avoid `Sync` requirements induced by `Arc<T>`. |
| Non-wire storage | Ordinary declared typed values, getters, and setters; no required placement. | Keep context exclusively in an application wrapper around the client. | Storage avoids repetitive wrappers for genuinely client-local values. Wrappers better separate broad application state. Both should remain practical. |
| Error/sensitivity boundary | Safe classification and formatting; unread response ownership; explicit raw access. | Preserve native error chains by default and promise only limited URL/header redaction. | The alternative is familiar and easier to inspect, but cannot support a strong protected-diagnostics promise. |

**Optionality is independent of payload type**

Assume the selected representation formats an inner `None` as JSON `null`:

| Declaration | Input/control | Meaning |
|---|---|---|
| `value payload: Option<T>;` | Required input field contains `None` | Present payload representing `null` |
| `optional payload: T;` | `set_payload(None)` | No placement |
| `optional payload: T;` | `payload(value)` | Present payload |
| `optional payload: Option<T>;` | `set_payload(None)` | Outer absence: no placement |
| `optional payload: Option<T>;` | `payload(None)` | Present inner `None`: `null` |
| `optional payload: Option<T>;` | `set_payload(Some(None))` | Same present `null` |
| `optional payload: Option<T>;` | `payload(Some(value))` | Present non-null payload |

An adapter produces a value, not an omission decision. It cannot silently turn `""` or an inner `None` into absence.

An alias such as `type Nullable<T> = Option<T>` receives identical behavior. No macro type-name heuristics participate.

**Default lifecycle**

Let `D` be the captured initial value and `X` a later override.

| Operation | Current choice afterward | Initializer evaluation |
|---|---|---|
| Select owner with default | `D` | Once |
| Set `X` | `X` | None |
| Clone | Same retained choice | None |
| Prepare | Same retained choice | None |
| Send | Same retained choice | None |
| Reset | Captured `D` | None |
| Create a new owner selection | New initial choice | Once for that new owner |

For `optional category: String = Some("all".to_owned())`:

| Operation | Effective category |
|---|---|
| Select | `"all"` |
| `set_category(None)` | Absent |
| `category(String::new())` | Present empty |
| `reset_category()` | `"all"` |

If several defaults exist, evaluate in declaration order. Any supported dependencies must refer to already initialized inputs; forward references should fail near the reference.

Changing a dependency later does not recompute derived defaults. That limitation must appear in generated documentation.

**Inherited exact controls**

Let `P` be the parent value captured when the child was selected.

| Child command | `P = Some("en")` | `P = None` |
|---|---|---|
| No override | `"en"` | Absent |
| `locale("fr")` | `"fr"` | `"fr"` |
| `set_locale(Some("fr"))` | `"fr"` | `"fr"` |
| `set_locale(None)` | Absent | Absent |
| `inherit_locale()` | `"en"` | Absent |

An inherited override targets a declared value identity. If that value supplies several placements, they all receive the effective replacement.

If only one query occurrence should change, declare a distinct child value and use a query composition operator. Do not silently overload value shadowing to mean HTTP-key editing.

**HTTP composition**

I would support these authoring operations for query/header contributions:

```text
query "k" <- value                  append
override query "k" <- optional      replace iff present
replace query "k" <- optional       exact replacement
remove query "k"                    remove earlier contributions
```

The corresponding repeated form expands an ordered sequence.

| Child contribution | Append | Conditional override | Exact replacement |
|---|---|---|---|
| Absent optional | Preserve ancestors | Preserve ancestors | Remove ancestors |
| Present scalar `x` | Ancestors, then `x` | Only `x` | Only `x` |
| Present empty scalar | Append `k=` | Only `k=` | Only `k=` |
| Present sequence `[x, y]` | Ancestors, then `x, y` | Only `x, y` | Only `x, y` |
| Present empty sequence `[]` | Preserve ancestors | Remove ancestors | Remove ancestors |

Removal is not a permanent prohibition. A later descendant can reintroduce the key.

Resolve contribution liveness **before** formatting. A formatter belonging only to a removed contribution must not run.

Ordering rules:

- Process declaration contributions root to leaf.
- Append at the current declaration position.
- Replacement removes earlier live occurrences and emits its replacements at the replacement position.
- Query order is exact.
- Header names compare case-insensitively.
- Preserve same-name header value order; promise no global header order.

For example:

```text
a=1 & k=old & b=2
```

followed by replacement of `k` becomes:

```text
a=1 & b=2 & k=new
```

Concord removal affects Concord contributions. It must not claim to erase native client default headers. An omitted Concord header can allow a native default to appear.

**Representation details**

The chosen defaults are author declarations, not global library assumptions.

```text
default body using json;
default response using json;
```

Consequences:

- `response String` under the JSON default expects a JSON string.
- Text requires `response String using text`.
- Empty output requires `response () using empty`.
- `json<()>` remains JSON unit semantics, not empty-response semantics.
- No declared body means no body.
- A declared zero-length bytes body remains a body; do not infer transport headers from that distinction.
- JSON encoding supplies its content type when no explicit Concord content type is selected. Explicit selected headers take precedence.
- Do not automatically synthesize `Accept`.
- JSON decoding uses the selected representation rather than sniffing a different representation from the response.
- A JSON endpoint receiving an empty 204 response produces a decoding failure unless its decoder explicitly supports that case.
- `empty` drains in bounded chunks and reports read failure.
- `raw` returns the successful native response without buffering or draining it.
- Status classification precedes success decoding. Non-2xx responses remain unread.

Lexical defaults should be expanded and displayed in generated endpoint rustdoc. A caller should not need to find the ancestor declaration to learn the effective codecs.

**Wire and base rules**

Adapters are ordinary per-use functions with a small result contract, conceptually:

```rust
fn format(value: &DomainType) -> Result<String, BoxError>;
```

Their output is unescaped value text. Concord applies placement encoding afterward.

This supports different spellings of one type:

```text
segment id using id_path;
query "id" <- id using id_query;
header "x-id" <- id using id_header;
```

A foreign timestamp can use a local function without an orphan impl. A newtype remains a good alternative when one wire spelling is intrinsic to the application domain.

I would choose these boundaries:

- Dynamic path values are single segments.
- `/`, `%`, Unicode, `?`, and `#` are encoded as data.
- Raw text `%2F` becomes `%252F`; it is not treated as pre-escaped structure.
- Reject empty, `.` and `..` dynamic segments during preparation.
- Express intended trailing slash with a static path literal, not an empty dynamic value.
- Append declared segments to the base prefix; do not use URL resolution that can reset the prefix.
- Host-label formatting permits one validated DNS label, including an explicit IDNA conversion policy. It cannot introduce dots, ports, or authority syntax.
- Ordinary host-label composition onto an IP base fails safely.
- Static and runtime URL values use the same URL parsing and accepted-boundary rules.
- Reject base userinfo, query, and fragment; permit HTTP(S), valid escaping, IDNA hosts, ports, and path prefixes.
- Invalid header names/values become declaration errors where statically knowable, otherwise value-free preparation errors.
- Header construction must handle capacity failure through fallible checks rather than ordinary-data panics.

For local/staging use:

```rust
let local = root.clone().with_resolved_base(
    reqwest::Url::parse("http://127.0.0.1:8080/proxy")?,
);
```

The predicted search path is:

```text
/proxy/v1/tenants/acme/search
```

`with_resolved_base` explicitly replaces the declared base **and host-label composition**. It does not try to prepend `eu` to an IP address. It leaves declared paths intact.

This is a distinct override operation, not a different interpretation of the same base literal. The root still requires its domain inputs, including region, even when this override suppresses their host use.

**Ownership and non-wire values**

Retained payloads are shared internally, for example through `Arc<T>`. Initial and current choices share the same allocation until replaced.

This means:

- A non-`Clone` JSON DTO can be selected, selection-cloned, and encoded by reference.
- Large payloads are not deeply cloned by navigation or selection cloning.
- Parent setters create new structural selections.
- Caller-owned interior mutation remains observable.
- `Rc<RefCell<T>>` can be legitimate local storage, but the containing selection is not `Send`.
- Sharing `T` through `Arc<T>` requires both `Send` and `Sync` to move the containing selection between threads.
- A prepared request need not retain the non-wire context once rendering completes.
- Generic generated `Clone` implementations must clone handles without accidentally adding `T: Clone`.

For the minimal proposed surface, retained inputs are owned types. I would **not initially promise lifetime-parameterized borrowed selections**. Callers can use owned strings, `Arc<str>`, owned DTOs, or native Rust for borrowed work.

That is a deliberate limitation with a real allocation cost, not something to hide under “ordinary Rust types.”

Non-wire storage requires no separate annotation or trait:

```rust
let configured = root.clone().context(Arc::new(CallContext {
    operation_id: "catalog-import-7".to_owned(),
}));

let context = configured.context_value();
```

Broad application services can remain in a wrapper:

```rust
struct Application {
    catalog: Catalog,
    admission: ApplicationAdmission,
    credentials: ApplicationCredentials,
}
```

Neither approach requires Concord to own those services.

**Sensitivity and error boundary**

I would use explicit sensitivity plus a small documented safety-name policy for recognized secret headers, such as `authorization`, `proxy-authorization`, `cookie`, `set-cookie`, and `x-api-key`.

That policy is a safety net, not a claim to identify every secret name.

Sensitivity follows declared value identity and explicit reference/override lineage:

- Marking a value sensitive protects every Concord diagnostic placement using it.
- Using a value in a recognized sensitive header makes its other declared uses protected too.
- An inherited override cannot silently remove that protection.
- Replacing a protected contribution keeps the replacement protected.
- Removed contributions do not leak through diagnostic history.
- Arbitrary application computations outside the declared reference graph are not automatically taint-tracked.

A retained U12 variation would add:

```text
query "audit_key" <- root.api_key using utf8;
```

The wire contains the supplied value in both placements. Concord-owned request diagnostics redact both.

Protected formatting must never call a payload’s arbitrary `Debug` implementation. Selection debug can show names and presence states. Prepared debug can show sanitized rendered metadata. Bodies and raw response contents should not be automatically printed.

For runtime bases, mask the caller-supplied base components in protected diagnostics: even a valid hostname or path prefix may contain private data.

Transport and decoding errors must not expose raw causes through their normal error chain. Removing only the original URL from a native error is insufficient: redirects, headers, response content, and custom decoder errors can echo values elsewhere.

Explicit boundaries include:

- Native request inspection.
- Native response/header access.
- Decoded application DTOs.
- Raw-cause access.
- User callback behavior.
- Logging performed by the caller or native libraries.

Those are not protected Concord formatting.

**The weakest choice and its serious alternative**

The weakest part of my proposal is **eager initialization plus retained reset baselines**.

It keeps the selected value stable, but:

- An immediately overridden initializer still runs.
- Reset retains initially present values.
- Replacing a large required body can retain both the initial and current body.
- Initially present sensitive data can remain alive for reset.
- Side-effectful defaults are surprising even with a precise contract.

The strongest alternative is a two-phase draft, using the **same declaration**:

```rust
impl Catalog {
    pub fn builder(
        http: reqwest::Client,
        input: CatalogInputs,
    ) -> CatalogDraft;
}

impl CatalogDraft {
    // Same optional/default setters, returning CatalogDraft.
    pub fn finish(self) -> Catalog;
}

impl Catalog {
    pub fn tenant(&self, input: TenantInputs) -> TenantDraft;
}

impl TenantDraft {
    pub fn finish(self) -> Tenant;
}

impl Tenant {
    pub fn search(&self, input: SearchInputs) -> SearchDraft;
}

impl SearchDraft {
    pub fn page_size(self, value: u16) -> Self;
    pub fn use_default_page_size(self) -> Self;
    pub fn set_category(self, value: Option<String>) -> Self;
    pub fn finish(self) -> Search;
}

impl Search {
    pub fn edit(&self) -> SearchDraft;

    // Clone/prepare/send retain resolved values as before.
}
```

Caller:

```rust
let root = Catalog::builder(http, CatalogInputs {
    region: "eu".to_owned(),
})
.finish();

let tenant = root.tenant(TenantInputs {
    tenant_id: "acme".to_owned(),
})
.finish();

let selected = tenant.search(search_input())
    .page_size(200)
    .finish();

let copied = selected.clone();

let first = selected.prepare()?;
let second = copied.prepare()?;

// Explicitly requests a new default evaluation at the next finish.
let fresh_default = selected.edit()
    .use_default_page_size()
    .finish();
```

Alternative semantics:

- Supplied choices suppress their initializers.
- `finish()` evaluates only unchosen defaults, in source order.
- Clone, prepare, and send retain resolved choices.
- Editing retains current resolved values until explicitly invalidated.
- `use_default_*` requests a new evaluation; it does not restore an old baseline.
- There is no automatic retained initial-value reset.

This alternative is credible because it addresses actual wasted work and retention. Its cost is another phase and more generated types at roots, scopes, and endpoints. Selective finalization would reduce ceremony but make the API less uniform.

I prefer the eager proposal for cheap ordinary defaults. If realistic usage requires expensive defaults or avoiding their evaluation when overridden, I would choose the draft alternative rather than introduce lazy evaluation during preparation.

**5. Likely surprises, hidden user costs, and native boundaries**

A small implementation can impose substantial repeated costs:

- **Generated API discovery.** Named records, scope types, setters, and decoder types need useful rustdoc. Fewer macro primitives do not compensate for undocumented generated behavior.
- **Visible conversions.** Exact setters taking `String` require `.to_owned()` or `.into()`. This avoids hidden conversion rules but adds caller text.
- **Shared storage.** Cheap selection cloning still involves allocations, reference counting, delayed destruction, and `Sync` consequences.
- **Reset retention.** Initial values remain available. This does not retain every historical setter value, but snapshots and baselines can prolong lifetimes substantially.
- **Encoding repeats.** Two preparations can serialize a large body twice. The default value is retained; encoded bytes are not necessarily retained.
- **User code is observable.** Defaults, adapters, and custom encoders can panic, mutate shared objects, or perform their own I/O. Concord cannot claim purity or byte stability for arbitrary callbacks.
- **Lexical representation defaults.** They reduce repetitive declarations while making isolated endpoint reading less complete. Resolved generated docs are necessary.
- **Safe errors reveal less text.** A caller may need explicit raw inspection to diagnose malformed provider content. That friction is part of the chosen boundary.
- **Exact inherited absence.** Clearing a Concord contribution does not erase native client defaults.
- **One native execution call is not exactly-once delivery.** Native redirects or policy can produce additional HTTP activity. Concord must describe only what it controls.
- **Macro scaling remains unknown.** There is no basis here to claim 1,000 endpoints compile cheaply or that code generation is linear in every relevant dimension.

The remaining task routes are as follows. All are **reasoning dispositions, NOT_RUN**.

| Tasks | Proposed route or boundary |
|---|---|
| U01 | A zero-input root takes only the native client; `response String using text`; no hidden HTTP-client construction. |
| U02 | JSON body/output as above; create uses its named body input; delete explicitly selects `response () using empty`. |
| U03–U04 | Named required-input records; remaining same-type assignment risk is explicit. |
| U05 | Borrowing navigation captures shared structural snapshots; reuse the tenant for several selections. |
| U06–U07 | Explicit optional placement, exact setters, and independent nullable payload types. |
| U08–U10 | Captured defaults, structural snapshots, exact clear, and explicit inheritance controls. |
| U11 | Ordered `Vec` repetition; an empty collection emits no appended pairs. Under replacement it can clear ancestors. |
| U12 | Explicit sensitivity and declared reuse graph; wire-positive and protected-diagnostic-negative expectations. |
| U13 | Explicit resolved-base override with appended declared paths. |
| U14–U15 | Unread status ownership and native request/decoder transfer. |
| U16 | Two representation traits; custom encoding/decoding stays downstream. No runtime registration. |
| U17 | Reject unknown ancestor references, duplicate declarations, implicit ancestor shadowing, and incompatible adapter/input types near their causes. Include both relevant spans for duplicates. |
| U18 | Renamed dependencies and `macro_rules!` forwarding are required downstream checks. Generated runtime paths must not assume the dependency is named `concord`. |
| U19 | Core representations work without JSON; JSON is an explicit feature. Feature isolation requires separate consumers, not one unified workspace. |
| U20 | Quick start and generated docs must state effective formats, setter semantics, status handling, and raw boundaries. |
| U21–U22 | Per-use functions support domain-specific spellings and foreign types without orphan impls. |
| U23 | Explicit lexical JSON defaults, with binary/raw/empty exceptions at the endpoint. |
| U24 | Ordinary typed fields or an application wrapper; no fake header/query placement. |
| U25 | Owned non-`Clone` values supported through shared retention; borrowed retained selections deliberately unsupported initially. Consumptive native bodies use native exit. |
| U26 | Caller-owned interior mutation remains visible; no deep-freeze promise. |
| U27 | Ordinary modules and explicit visibility; supported `cfg`, `cfg_attr`, docs, and deprecation must propagate coherently to generated artifacts. Unsupported attributes should be diagnosed, not silently dropped. |
| U28 | `raw` response; native chunk/stream access; caller-owned timeout and early drop. |
| U29 | Transfer the prepared request into native request-building/body APIs, attach multipart/form data, then use the retained decoder. |
| U30 | Application-owned bounded cursor loop and bounded retry around replay-safe requests. No core traversal or replay policy. |
| U31 | Acquire admission in application code; construct a selection with the currently supplied authorization value; execute; release the application permit. |
| U32 | Native URL-query rewrite for an uncommon heterogeneous interleaving. Preserve or reconstruct the complete intended query order explicitly. |
| U33 | Shared validation rules, safe preparation errors, and explicit resolved-base semantics; boundary inputs need actual tests later. |
| U34 | Retain a generator and separate consumers for compile/codegen/fanout measurements. No performance conclusion in this pass. |
| U35 | Trace named inputs → live contributions → adapters → native request; maintenance evidence still needs an actual source change later. |
| U36 | Requires the frozen alpha source in pass 2. An accurate migration mapping cannot be invented blind. Do not preserve parallel legacy grammar merely to avoid migration. |

Two small routes deserve explicit illustration.

A bounded cursor sequence is application code:

```rust
let mut cursor = None;

for _ in 0..maximum_pages {
    let page = tenant.search(search_input())
        .set_cursor(cursor.take())
        .send()
        .await?;

    consume_items(page.items);

    match page.next_cursor {
        Some(next) => cursor = Some(next),
        None => break,
    }
}
```

A native download has no Concord stream scheduler:

```rust
let mut response = product.download().send().await?;

let first_chunk = tokio::time::timeout(
    std::time::Duration::from_secs(2),
    response.chunk(),
)
.await??;

drop(response); // Application deliberately stops early.

let _first_chunk = first_chunk;
```

The latter has crossed into native response behavior; native errors there are application-owned.

Native escape should be sufficient for:

- Request timeout, uncommon headers, and specialized reqwest options.
- Multipart, form, and consumptive streaming request bodies.
- Raw response streaming and application timeouts.
- Unusual interleaved query sequences.
- Uncommon response unions or content negotiation through a custom decoder or raw response.
- Specialized native redirect/TLS/proxy configuration on the supplied client.

None warrants a general execution framework.

The contract matrix remains entirely **NOT_RUN** here. In particular, hygiene, feature isolation, native header capacity, draining memory behavior, streaming, portability, packaging, mutation discrimination, sensitivity scale, and compile-time growth require executable evidence later. The independent semantic oracle described in `CONTRACT_CASES.md` is a suitable later validation technique; it was not run in this pass.

**6. Explicitly rejected ideas**

| Rejected idea | Counterexample |
|---|---|
| Positional required root arguments as the canonical API | Two `String` identities swap without a compiler error; adding six more makes review worse. |
| Inferring optional placement from `Option` syntax | A required `Option<Patch>` must be supplied and may encode `null`; a type alias defeats spelling heuristics. |
| Giving `None` both “clear” and “inherit” meanings | An optional default can never be deliberately removed without another, undocumented control. |
| One merge operation for every contribution | An absent optional tag override should inherit; a present empty tag sequence should be able to clear. |
| Evaluating defaults in `prepare()` or `send()` | Cloned selections produce different counter-valued defaults or idempotency values. |
| “Reset” silently reevaluating a default | A caller expects the original selected value but receives a new timestamp or identifier. |
| Implicit `Display` for every wire placement | Human-oriented timestamp/ID formatting changes the protocol, and one type needs three spellings. |
| Guessing JSON/text/empty from output type or status | `String` is ambiguous; `()` can mean JSON `null`; empty 204 handling can conceal a broken response contract. |
| Automatically consuming error documents before returning status failure | A malformed or unexpected error document destroys the caller’s opportunity to inspect the unread response. |
| A nominal native exit that loses decoding | The caller adds a timeout but must reimplement status classification and success/error conversion. |
| Requiring all retained payloads to implement `Clone` | A move-only context or non-`Clone` serializable DTO prevents cheap selection snapshots. |
| Claiming snapshots deeply freeze caller objects | An `Arc<Mutex<T>>` is intentionally shared and remains mutable. |
| Forcing non-wire context into a wire field | Application context becomes a fake header solely to remain reachable. |
| Protected outer errors with raw `source()` chains | A redirect or custom decoder error exposes a secret through routine error reporting. |
| Mirroring every reqwest option in the DSL | One uncommon timeout or multipart requirement expands the core vocabulary despite a viable typed native exit. |
| Auth/refresh, retry, admission, or pagination hooks | A caller-supplied token or cursor turns into a lifecycle subsystem the charter excludes. |
| Blanket exactly-once or stable-byte claims | Native policy can perform more HTTP activity, and custom encoders can observe mutable state. |
| Keeping two alpha grammars indefinitely | Maintainers must preserve and explain divergent semantics instead of supplying one accurate migration. |