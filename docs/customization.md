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
- Built-in `Json<T>` and `Text<String>` use the same trait path. `NoContent` intentionally omits request and response content headers.

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

## Custom Pagination Controllers

A custom controller implements `PaginationController<Page>`. The controller owns pagination state, mutates the next page request through `PageRequest`, and decides whether to continue after each page when the runtime has not already stopped.

`paginate TypePath` constructs the controller through `Default`, so custom controller marker types must implement `Default + PaginationController<Page>`.

```rust
use concord_core::advanced::{
    PageAdvance, PageDecision, PageInit, PageRequest, PaginationController, ProgressKey,
};
use concord_core::prelude::ApiClientError;
use std::num::NonZeroUsize;

#[derive(Default)]
pub struct HeaderCursorPagination;

#[derive(Default)]
pub struct HeaderCursorState {
    page: u64,
}

impl PaginationController<Page<String>> for HeaderCursorPagination {
    type State = HeaderCursorState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(HeaderCursorState::default())
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("page", state.page);
        request.set_query("limit", 100);
        request.set_expected_items_per_page(
            NonZeroUsize::new(100).expect("page size is non-zero"),
        );
        Ok(())
    }

    fn advance(
        &self,
        state: &mut Self::State,
        page: &Page<String>,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        let _ = page;
        state.page += 1;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}
```

Declare it without a configuration block:

```rust
api! {
    client ExampleApi { base "https://example.com" }

    GET ListItems
        as list_items
        path ["items"]
        paginate HeaderCursorPagination
        -> Json<Page<String>>
}
```

Controller rules:

- Built-in pagination keeps using configuration blocks.
- Custom pagination uses `paginate TypePath` without a block.
- Custom pagination controller types must implement `Default`.
- `PageRequest` can set or remove query parameters and headers. Query keys may be owned or dynamic strings. Header mutation is fallible and returns `ApiClientError` for invalid header names instead of panicking.
- `PageRequest::set_expected_items_per_page(NonZeroUsize)` tells the runtime how many items the current page requested. Set it during every `apply()` call that asks for a known page size; the value is per-page and does not persist.
- `PageItems::item_count_hint()` must be exact when present. Implement it whenever possible so runtime empty-page stop, hard-item-cap overflow, and provable `TakeItems` completion can be decided before `advance()`.
- With both an exact hint and an expected page size, the runtime also owns generic short-page stop and will not call `advance()` for terminal hinted pages.
- Without an exact hint, `collect()` remains exact after consuming the page, but custom `advance()` may already have run. Without an expected page size, Concord cannot generically detect a short page before `advance()`.
- `progress_key` is used for loop detection when enabled.
- Runtime retry, auth, rate-limit, and redaction behavior still follow the fixed pipeline.

Complete examples live in `concord_examples/src/custom_codec.rs` and `concord_examples/src/custom_pagination.rs`.
