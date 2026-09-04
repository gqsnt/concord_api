# Responses, status, representations, and streams

Response design separates endpoint success semantics, representation/media validation, acquisition, exact caller output, and lazy post-return lifecycles.

## Endpoint-owned accepted status

```rust
status success;                // final 2xx
status 200;
status [200, 201, 204];
status [200..=299, 304];
```

Omission means final `200..=299`. Status is finite static endpoint data, not a callback hidden inside a JSON or custom response representation.

Authentication challenge attribution runs before status acceptance. A status cannot be both a recoverable auth challenge and an accepted endpoint outcome.

Expected non-2xx domain outcomes remain possible:

```concord-pseudocode
GET find(id: ItemId) -> FindItem via FindItemRepresentation {
    status [200, 404];
}
```

The representation receives an already accepted status/head and returns a domain enum.

## Response representation role

Conceptually:

```rust
pub trait ResponseRepresentation<Output>: Send + Sync + 'static {
    type Driver: SealedResponseDriver<Output = Output>;

    const DESCRIPTION: ResponseRepresentationDescription;

    fn default_accept() -> Option<StaticHeaderValue>;

    fn validate_accepted_head(
        head: AcceptedResponseHead<'_>,
    ) -> Result<(), ResponseRepresentationError>;
}
```

The exact trait/driver shape is prototype-gated. Fixed authority:

- representation receives only an accepted bounded head and chooses/configures a supported core driver;
- it owns response media negotiation/validation and conversion semantics;
- core owns native response, polling, content decoding, limits, progress, deadline, cancellation, and stream correlation;
- status acceptance remains outside the trait;
- custom representations cannot create arbitrary native drivers.

## Exact caller output

The arrow is exactly ordinary `.await` output.

### Complete values

```rust
-> Item
-> Vec<Item>
-> Bytes
-> String
-> ()
-> PageEnvelope<Item>
```

The operation does not return until the complete semantic value exists or fails.

### Lazy bytes

```rust
-> ByteStream
```

`ByteStream` yields post-content-decoding byte chunks. Chunk boundaries are not stable semantic records. The type may support the ecosystem stream trait and `AsyncRead` where practical.

### Lazy semantic items

```rust
-> ItemStream<Event>
```

`ItemStream<T>` yields semantic decoded items, with framing/decoding/item errors distinct from byte-stream failures.

Neither stream is `Clone` without real replay/multicast semantics.

## Built-in pair matrix

The exact implementation is gated by Rust coherence and diagnostics. The intended finite first-party matrix is:

| Representation | Output | Meaning |
|---|---|---|
| `Json` | `T: DeserializeOwned` | one complete JSON document |
| `Text` | `String` | complete validated text |
| `Binary` | `Bytes` / `Vec<u8>` | complete post-decoding bytes |
| `Binary` | `ByteStream` | lazy bounded bytes |
| `NdJson` | `Vec<T>` | incremental records collected completely |
| `NdJson` | `ItemStream<T>` | lazy semantic records |
| `Csv<Cfg>` | `Vec<T>` | incremental rows collected completely |
| `Csv<Cfg>` | `ItemStream<T>` | lazy semantic rows |
| `Empty` | `()` | accepted body must contain zero decoded bytes |
| `Discard` | `()` | body deliberately ignored under finite discard limits |

Do not make `Json<()>` the implicit no-body path. JSON null, zero bytes, and ignored bytes are different contracts.

## Media ownership

Response representation owns default `Accept` and accepted-content validation. Generic public headers cannot set `Accept` or `Accept-Encoding`.

Dynamic per-request media remains deferred. A local endpoint chooses another representation marker/adaptor instead.

## Runtime ordering

```text
response head observed
→ parse standard server delay and limiter feedback
→ auth challenge attribution/recovery
→ endpoint status acceptance
    rejected → policy retry or bounded status error
    accepted → representation head/media validation
             → sealed bounded acquisition
             → conversion or stream acquisition
```

Rejected statuses do not invoke the success representation decoder.

Accepted representation validation/decode failure is terminal by default; repeating deterministic malformed content is not an automatic retry strategy.

## `ApiResponse<T>`

```rust
let response: ApiResponse<Item> = request.response().await?;
```

Exposes managed accepted metadata through bounded explicit accessors. It does not return native response ownership or automatic unsanitized header maps.

## Accepted bytes

Endpoint declaration:

```rust
expose accepted_bytes;
```

Generated:

```rust
let response: ApiResponse<Bytes> = request.accepted_bytes().await?;
```

Semantics:

- same request, auth, rate, deadline, retry, negotiation, and endpoint status;
- complete bounded post-content-decoding byte acquisition;
- bypass response-representation accepted-head/body validation and output conversion;
- no native response escape;
- absent unless explicitly exposed and mechanically supportable.

For a lazy endpoint, accepted bytes intentionally changes delivery to complete bounded collection and therefore still requires explicit exposure.

## Stream lifecycle

The request operation completes when the accepted stream handle is acquired. Later polling:

- can fail independently;
- uses post-return byte/item/progress/read-idle limits;
- is never transparently retried after yielding data;
- emits stream-correlated completion/failure/drop evidence;
- is canceled locally on drop, without claiming remote effect cancellation.

## Decompression and bounds

All body/status/pagination/accepted-byte/stream limits count post-content-decoding bytes. Transport time and read-idle limits still bound receipt/decoding. Concord does not claim a compressed-wire-byte bound if the transport cannot expose it.

## Diagnostics

Prefer structured format/phase/location/reason:

```text
format: Json
phase: Decode
location: Field("items").AnyIndex.Field("created_at")
reason: TypeMismatch
```

Bound segment count/bytes, remove values and dynamic sensitive keys, normalize indices where useful, and keep raw third-party strings protected.
