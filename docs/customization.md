# Customization

Concord keeps the common path small, but the advanced API exposes stable extension points for projects that need formats or pagination styles outside the built-ins.

See [Security Model](security_model.md) for how `prelude`, `advanced`, and `dangerous` are separated.

Use these extension points when the protocol is part of your API contract. Do not use them to change runtime pipeline order or to bypass redaction.

Custom transports are an advanced caller-owned security boundary. Concord's default reqwest transport disables redirects, but `with_reqwest_client(...)` hands redirect, proxy, TLS, cookie, and other reqwest client policies to the caller. Both the default constructor and `with_reqwest_client(...)` require the `transport-reqwest` feature; `with_transport(...)` remains available without reqwest.

Runtime hooks and debug sinks also sit on a security boundary. They receive sanitized metadata views, not raw header maps, and they never receive request or response body bytes or raw auth material. `pre_send` runs after rate-limit acquisition and before raw auth transport materialization, `post_response` runs after an HTTP response is received and before response body read and endpoint decode, and `transport_error` only observes initial transport-send failures. Sensitive header names and sensitive query values are redacted before callback invocation. High-volume debug can add measurable overhead.

Generated Rustdoc for defaulted setters describes the declared default/reset behavior without rendering the source default expression value. Runtime defaults and `Option` reset semantics are unchanged.

Buffered response body-read transport failures are sanitized before they become public `ApiClientError::Transport` values. The public error keeps the endpoint, method, and transport kind, but it does not render the raw body-read source chain. Response body-size limit errors remain structured as their own typed errors, and buffered response decode failures are sanitized separately.

## Dangerous Dev Body Capture

Dev body capture is not part of the debug, hook, or error surface. The deprecated `concord_core::dangerous::DevBodyCaptureConfig` is behind `dangerous-dev-tools`, disabled by default, dev-only, and local-file-only. Enabling the feature does not start capture by itself. When explicitly configured, it writes raw selected response bytes to disk with no redaction, never captures request bodies, skips protected auth-bearing requests and auth endpoint traffic by default, and treats `max_bytes` as a capture-size filter rather than redaction or a truncation guarantee. Do not enable it in production, CI logs, CI artifacts, shared directories, user-visible support bundles, or any environment without controlled local filesystem permissions.

## Custom Codecs

A request body codec implements `BodyCodec`. A response codec implements `ResponseCodec`. The shared wire-content trait is `ContentType`; codec markers carry their wire content identity through an associated `Content` type.

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

Dynamic path segments are validated before transport. Required dynamic path values must stringify to a non-empty segment, and optional dynamic path values reject `Some("")` while preserving the existing `None`-omits-segment behavior where supported. The empty-string rule does not apply to query parameter values.

Codec rules:

- `ContentType` provides the wire content identity for buffered codecs and format markers.
- `BodyCodec::try_content_type()` and `ResponseCodec::try_accept()` default from the associated `Content` marker. Override them when a codec intentionally omits the header or needs to surface a typed validation error.
- `EncodeContext` and `DecodeContext` provide endpoint metadata for contextual errors.
- `CodecError` messages must be safe to display. Never include secrets or raw credentials.
- Buffered request-body encode failures are sanitized again at the client boundary. Public `ApiClientError::Codec` values for buffered request preparation use a generic request-body encoding message and do not expose raw codec messages or nested codec sources.
- Buffered response decode failures are also sanitized at the client boundary. Public `ApiClientError::Decode` values for buffered response handling use a generic response-body decode message and do not expose raw codec messages or nested codec source chains. Streaming decode paths remain separately sanitized by their own response-specific wrappers.
- Built-in `Json<T>` and `Text<String>` use `JsonContentType` and `TextContentType`. The core `NoContent` codec intentionally omits request and response content headers. The DSL spelling `-> NoContent` is response-only, returns `()`, and remains distinct from the buffered codec; request-side `NoContent` remains invalid. The DSL spelling `-> Bytes` is response-only, returns `bytes::Bytes`, uses the ordinary bounded buffered response path that materializes payloads in memory, and is distinct from custom binary codecs and `#[cfg(feature = "dangerous-raw-response")] execute_raw_response()`. Request-side `Bytes` remains unsupported.

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

## Custom Pagination

A custom pagination controller type implements `Default + EndpointPagination<Page>`. Generated endpoints set `PaginatedEndpoint::Pagination = Type` and keep the endpoint model in sync through `PaginateBinding<Type>` before planning.

`paginate Controller { ... }` constructs the controller through `Default`, so custom controller marker types must implement `Default + EndpointPagination<Page>`.

Assignment rules are shared across built-in and custom pagination controllers:

- endpoint-field assignments are loaded from the endpoint into the pagination object and stored back after the page advances
- literal and config assignments initialize pagination fields but are not stored back to endpoint fields
- built-in cursor pagination uses `CursorPagination<String>`

```rust
use concord_core::advanced::{
    EndpointPagination, PageAdvance, PageApply, PageDecision, PageItems,
    ProgressKey,
};
use concord_core::prelude::ApiClientError;
use std::num::NonZeroUsize;

#[derive(Default)]
pub struct HeaderPagePagination {
    pub page: u64,
    pub count: u64,
}

impl<Page> EndpointPagination<Page> for HeaderPagePagination
where
    Page: PageItems,
{
    fn apply(
        &mut self,
        _ctx: PageApply<'_>,
    ) -> Result<(), ApiClientError> {
        if self.count == 0 {
            return Err(ApiClientError::pagination(
                concord_core::advanced::ErrorContext {
                    endpoint: "ListItems",
                    method: ::http::Method::GET,
                },
                concord_core::error::PaginationErrorKind::InvalidSize,
                "custom pagination requires a non-zero page size",
            ));
        }
        Ok(())
    }

    fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        usize::try_from(self.count).ok().and_then(NonZeroUsize::new)
    }

    fn advance(&mut self, page: &Page, _ctx: PageAdvance<'_>) -> Result<PageDecision, ApiClientError> {
        if page.item_count() == 0 {
            return Ok(PageDecision::Stop);
        }
        self.page = self.page.saturating_add(1);
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        Some(ProgressKey::U64(self.page))
    }
}
```

Declare it with the uniform `paginate Type { ... }` form:

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
        paginate HeaderPagePagination {
            page = page,
            count = count
        }
        -> Json<Page<String>>
}
```

Rules:

- Built-in pagination and custom pagination both use `paginate Type { ... }`.
- Custom controller types must implement `Default + EndpointPagination<Page>`.
- `EndpointPagination::expected_items_per_page()` tells the runtime how many items the current page requested. Set it during every `apply()` call that asks for a known page size.
- `PageItems::item_count()` must return the exact page size. Implement it whenever possible so runtime empty-page stop, hard-item-cap overflow, and provable `TakeItems` completion can be decided before `advance()`.
- With an exact item count and an expected page size, the runtime also owns generic short-page stop and will not call `advance()` for terminal short pages.
- Without an expected page size, Concord cannot generically detect a short page before `advance()`.
- `progress_key` is used for loop detection when enabled.
- Pagination loop diagnostics keep the progress key internal; public errors report only safe metadata such as page index and key kind/length, not raw cursor or byte contents.
- Runtime retry, auth, rate-limit, and redaction behavior still follow the fixed pipeline.

Complete examples live in `concord_examples/src/custom_codec.rs` and `concord_examples/src/custom_pagination.rs`.


