# Customization

Concord keeps the common path small, but the advanced API exposes stable extension points for projects that need formats or pagination styles outside the built-ins.

Use these extension points when the protocol is part of your API contract. Do not use them to change runtime pipeline order or to bypass redaction.

## Custom Codecs

A request body codec implements `BodyCodec`. A response codec implements `ResponseCodec`. The shared marker trait is `ContentType`; codec markers carry their wire content identity through an associated `Content` type.

```rust
use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
};
use std::marker::PhantomData;

pub struct Compact<T>(PhantomData<T>);
pub struct CompactContentType;

impl ContentType for CompactContentType {
    const CONTENT_TYPE: &'static str = "application/x-compact";
}

pub struct CreateUser {
    pub name: String,
}

pub struct User {
    pub id: u64,
    pub name: String,
}

impl BodyCodec for Compact<CreateUser> {
    type Value = CreateUser;
    type Content = CompactContentType;

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::copy_from_slice(value.name.as_bytes())))
    }
}

impl ResponseCodec for Compact<User> {
    type Value = User;
    type Content = CompactContentType;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        let text = std::str::from_utf8(&bytes)
            .map_err(|source| CodecError::with_source("compact response is not utf-8", source))?;
        let (id, name) = text
            .split_once(':')
            .ok_or_else(|| CodecError::new("compact response must be `id:name`"))?;
        Ok(User {
            id: id
                .parse()
                .map_err(|source| CodecError::with_source("compact id is invalid", source))?,
            name: name.to_string(),
        })
    }
}
```

Use the marker type directly in the DSL:

```rust
api! {
    client ExampleApi { base "https://example.com" }

    POST CreateUser(body: Compact<CreateUser>)
        as create_user
        path ["users"]
        -> Compact<User>
}
```

Codec rules:

- `ContentType` provides the wire content identity for buffered codecs and format markers.
- `BodyCodec::try_content_type()` and `ResponseCodec::try_accept()` default from the associated `Content` marker. Override them when a codec intentionally omits the header or needs to surface a typed validation error.
- `EncodeContext` and `DecodeContext` provide endpoint metadata for contextual errors.
- `CodecError` messages must be safe to display. Never include secrets or raw credentials.
- Built-in `Json<T>` and `Text<String>` use `JsonContentType` and `TextContentType`. The core `NoContent` codec intentionally omits request and response content headers. The DSL spelling `-> NoContent` is response-only, returns `()`, and remains distinct from the buffered codec; request-side `NoContent` remains invalid. The DSL spelling `-> Bytes` is response-only, returns `bytes::Bytes`, uses the ordinary bounded buffered response path, and is distinct from custom binary codecs and `execute_raw()`. Request-side `Bytes` remains unsupported.

## CSV Record Formats

CSV is a `Records<T, Csv<Cfg>>` record format, not a new endpoint family. It reuses `RecordBody<T>` and `RecordStream<T>` as the runtime values.

Custom CSV behavior implements `CsvConfig`.

Use `CsvCommaDelim`, `CsvSemicolonDelim`, or `CsvTabDelim` as the built-in configs. The config type selects the delimiter and whether headers are enabled; delimiter and header state are not encoded as `Content-Type` parameters.

CSV support uses the same `ContentType` marker path as the rest of the advanced API and uses `text/csv`.

## Page-Shape Traits

Paginated responses expose their items by implementing `PageItems`.

```rust
use concord_core::prelude::PageItems;

pub struct Page<T> {
    pub items: Vec<T>,
}

impl<T: Send + 'static> PageItems for Page<T> {
    type Item = T;

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}
```

Cursor-based built-ins also require `HasNextCursor`.

```rust
use concord_core::prelude::HasNextCursor;

impl<T: Send + 'static> HasNextCursor for Page<T> {
    type Cursor = String;

    fn next_cursor(&self) -> Option<Self::Cursor> {
        None
    }
}
```

## Endpoint-State Custom Pagination

An endpoint-state custom controller implements `EndpointPaginationController<E, Page>`. The controller owns pagination state and returns `PageApplyResult` for the next page while the endpoint-state runtime applies those bindings to the endpoint model before planning.

`paginate endpoint_state Controller bindings Bindings { ... }` constructs the controller through `Default`, so custom controller marker types must implement `Default + EndpointPaginationController<E, Page>`.

```rust
use concord_core::advanced::{
    EndpointField, EndpointPaginationController, PageAdvance, PageApply, PageApplyResult,
    PageDecision, PageItems, ProgressKey,
};
use concord_core::prelude::ApiClientError;
use std::num::NonZeroUsize;

#[derive(Default)]
pub struct HeaderPagePagination;

pub struct HeaderPageBindings<E> {
    pub page: EndpointField<E, u64>,
    pub count: EndpointField<E, u64>,
}

#[derive(Default)]
pub struct HeaderPageState {
    page: u64,
    count: u64,
}

impl<E, Page> EndpointPaginationController<E, Page> for HeaderPagePagination
where
    E: 'static,
    Page: PageItems,
{
    type Bindings = HeaderPageBindings<E>;
    type State = HeaderPageState;

    fn init(
        &self,
        bindings: &Self::Bindings,
        endpoint: &E,
        _ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError> {
        Ok(HeaderPageState {
            page: bindings.page.get(endpoint),
            count: bindings.count.get(endpoint),
        })
    }

    fn apply(
        &self,
        bindings: &Self::Bindings,
        state: &mut Self::State,
        endpoint: &mut E,
        _ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError> {
        bindings.page.set(endpoint, state.page);
        bindings.count.set(endpoint, state.count);
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(state.count as usize),
        })
    }

    fn advance(
        &self,
        _bindings: &Self::Bindings,
        state: &mut Self::State,
        page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        if page.item_count_hint() == Some(0) {
            return Ok(PageDecision::Stop);
        }
        state.page += 1;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}
```

Declare it with explicit endpoint-state pagination:

```rust
api! {
    client ExampleApi { base "https://example.com" }

    GET ListItems(page: u64 = 0, count: u64 = 100)
        as list_items
        path ["items"]
        query {
            "page" = page,
            "count" = count,
        }
        paginate endpoint_state HeaderPagePagination bindings HeaderPageBindings {
            page = page,
            count = count
        }
        -> Json<Page<String>>
}
```

Rules:

- Built-in pagination keeps using configuration blocks.
- Endpoint-state custom pagination uses `paginate endpoint_state ... bindings ...`.
- Endpoint-state custom controller types must implement `Default`.
- `EndpointField` values are written by the controller and read by the endpoint planner.
- `PageApplyResult::expected_items_per_page` tells the runtime how many items the current page requested. Set it during every `apply()` call that asks for a known page size.
- `PageItems::item_count_hint()` must be exact when present. Implement it whenever possible so runtime empty-page stop, hard-item-cap overflow, and provable `TakeItems` completion can be decided before `advance()`.
- With both an exact hint and an expected page size, the runtime also owns generic short-page stop and will not call `advance()` for terminal hinted pages.
- Without an exact hint, `collect()` remains exact after consuming the page, but controller advance may already have run. Without an expected page size, Concord cannot generically detect a short page before `advance()`.
- `progress_key` is used for loop detection when enabled.
- Runtime retry, auth, rate-limit, and redaction behavior still follow the fixed pipeline.

Complete examples live in `concord_examples/src/custom_codec.rs` and `concord_examples/src/endpoint_state_custom_pagination.rs`.
