# concord-extras

What Concord ships alongside its core. Concord is the crate `concord-api`, used as `concord` in code.

Everything here is built on the core's public traits and helpers: the writers use `body::Bounded`,
the `Json` reader `read::details`, the pagers `page::fragmentless`, the credentials, the signer and the limit
`Middleware`, and `transfer` the `page::Call` trait. Your own reader, writer, pager or middleware
uses the same items, and `gzip` keeps the content type of the body it compresses with
`Body::get_content_type`. Only the `tower` adapter also uses a hidden helper of the core, for the
URL of a response; `multipart` checks its boundary inside a stream part with the public
`Body::map_stream`.

| What | Items |
|---|---|
| Body writers | `json(x)`, `form(x)`, `multipart()` (feature `multipart`) |
| Nested forms and query pairs | `bracket_form(value)`, `bracket_pairs(value)` (feature `form`) |
| Gzip request bodies | `gzip(body)` (feature `gzip-body`), with Content-Encoding set by the description |
| MIME multipart bodies | `multipart_mixed(parts)`, `multipart_related(parts)` (feature `multipart`) |
| JSON record bodies | `json_lines(records, max_bytes)` (feature `json`), with a bound on the complete body |
| Readers | `Json`, `Text`, `Binary`, `Stream`; for records, `JsonLines` (feature `json`) and `Sse`, which return a `RecordStream` |
| Pagers | `by_offset`, `by_offset_limit`, `by_page`, `by_cursor`, `by_link`, `by_next_url` |
| A credential | `basic(user, password)`, which returns a `Secret` |
| A token that is renewed | `auth::Renewed`, `auth::after_401` (feature `auth`) |
| A refresh token that rotates | `auth::Rotating`, `auth::RotationDriver`, `auth::CredentialStore`, `auth::TokenPair` (feature `rotating`) |
| Signing | `sign::SigV4`, `sign::Credentials`, `sign::body_bytes`, `sign::check_origin` (feature `sign`) |
| Rate limits | `limit::Limit` (feature `limit`) |
| A URL that the API returned | `transfer::at(call, url)`, on the call of a second description |
| A transport | `tower::Tower`, a `tower` service as a transport (feature `tower`) |

## Who depends on it

For most users, nothing: depend on `concord-api` instead, which re-exports all of this and enables
these features through its own (`json`, `form`, `multipart`, `gzip-body`, `tower`, `auth`, `rotating`,
`sign`, `limit`).

Read its source when you write your own reader, writer, pager or middleware. Apart from the
hidden helper that the `tower` adapter uses, these use public traits and helpers. The guide covers them, with code
to copy, in [When Concord does not cover what you need](https://github.com/gqsnt/concord_api/blob/main/docs/guide/extending.md).

| Feature | What it turns on | Default |
|---|---|---|
| `json` | `json(x)`, `json_lines(records, max_bytes)`, `Json`, `JsonLines`, `SseEvent::json` | on |
| `form` | `form(x)`, `bracket_form(value)`, `bracket_pairs(value)` | on |
| `multipart` | `multipart()`, `multipart_mixed(parts)`, `multipart_related(parts)` | off |
| `gzip-body` | `gzip(body)` to compress a buffered request body and keep its content type. Response decompression is the feature `gzip` of `concord-api` | off |
| `tower` | `tower::Tower` | off |
| `auth` | `auth::Renewed`, `AccessToken`, `RenewalError`, `after_401` | off |
| `rotating` | `auth::Rotating`, `RotationDriver`, `CredentialStore`, `Stored`, `TokenPair`, `Exchange`, `RotationError`; turns on `auth` | off |
| `sign` | `sign::SigV4`, `Credentials`, `CanonicalPath`, `SignError`, `body_bytes`, `check_origin` | off |
| `limit` | `limit::Limit`, `Bucket`, `Window`, `Seen`, `LimitError`, the readers of headers `counts` and `remaining` | off |

## A token that is renewed

`auth::Renewed::new(&base, renew)` is a middleware that keeps one expiring token for one account.
`renew` is a function with no argument that fetches a token, through a client without this
middleware. When the token is missing or about to expire (`early_by`), the first call starts a
renewal, and the calls that come meanwhile wait for the same one. The renewal is a future that
those calls share, not a task: a call that is dropped leaves it to the others, and when the last one
is dropped, the next call starts another. A failure, a panic of `renew` (`RenewalError::Panicked`,
without its payload) or the end of `renewal_timeout` (30 seconds by default) lets the next call try
again; the cause of a failure is the `source()` of the `RenewalError`, which
`RenewalError::find(&error)` finds in the error of the call. `with_token` accepts any
seed that has not expired, whatever the order of the builder calls, and a seed inside `early_by` is
renewed on the first call; a renewed token that does not outlast that margin is `RenewalError::Expired`.

The token is sent only to the origin of the base (a path prefix does not narrow or widen it), in
`Authorization: Bearer <token>` or the header that `header(name, format)` sets, always marked
sensitive. A `401` is returned as it is, and the next call renews. `Retry::standard().when(after_401)`
asks the engine for one more attempt after a `401` on the first attempt; the engine still refuses to
repeat an unmarked `POST` or a `Body::stream`. The flows that obtain a token (OAuth2 and the
like) are the application's. For a refresh token that the server replaces at each use, see
`auth::Rotating`, below.

## A refresh token that rotates

`auth::Rotating::new(&base, store, exchange)` (feature `rotating`) returns a middleware and its
`RotationDriver`, for a refresh token that the server replaces at each exchange. The driver is a
future that runs the exchanges and the saves: the application polls it or spawns it, and Concord
starts no task. The application supplies the exchange, which returns a `TokenPair`, and a
`CredentialStore`, whose `load` and `save` keep one record: `Stored::Ready(pair)` or
`Stored::Refreshing`. `Refreshing` is saved before the exchange, the new pair is saved before either
token is used, a failed save is tried again as a save only, and a refresh token is exchanged once,
again only after an exchange that states that its request was never sent. The token client must not
send an exchange twice: no retry policy, and `concord::Http`, or a `reqwest::Client` with
`retry::never()` and `.redirect(Policy::none())`. A pair that the store could not keep is handed
back to the application in `RotationError::Unsaved`, never dropped. `renewal_timeout` (30 seconds by
default) bounds how long a call waits and never cancels the exchange; a call that waits while nobody
polls the driver fails with `NotPolled`, and a dropped driver stops the credential (`Stopped`). As
with `Renewed`, the token goes only to the origin of the base, in a sensitive header, and a `401` is
returned as it is. One `Rotating` and its clones own one account: the store has no lock between
processes, and no client can save a token at the moment the server issues it.

## Signing

`sign::SigV4::new(&base, credentials, region, service, path)` (feature `sign`) is a middleware that
signs each attempt with AWS Signature Version 4, after it checks that the request goes to the
origin of its base. `SigV4::sign(&mut request, region)` signs one request, in a region chosen per
call, after the same check of the origin. An adapter that signs for several origins on purpose
checks each request against its own list of origins with `check_origin`, then calls
`SigV4::sign_any_origin(&mut request, region)`, which skips the check. `CanonicalPath::Preserve` is the convention of S3 for the path, `NormalizeAndEncode` that
of the other services. `Credentials::replace` replaces the whole set for every clone, and one
signature uses one set. A body that is a stream is refused without being polled, unless
`allow_unsigned_payload` is set for S3 over HTTPS; a `Body::repeatable` is then signed again, as an
unsigned payload, at each attempt. No key, token or signature is printed. For a
provider's own HMAC proof, `body_bytes` borrows a buffered body (a stream is refused) and
`check_origin` compares the scheme, the host and the port; the hashing is done with `hmac` and
`sha2`. There is no chunk signing, presigned URL, credential discovery, OAuth flow or HTTP message
signature.

## Rate limits

`limit::Limit::new()` is a middleware that holds the calls after a `429` with `Retry-After`;
`without_retry_after(wait)` holds them for `wait` after a `429` without one.
`max_in_flight(n)` caps the calls in flight, and holds each permit until the body of the response
ends, fails or is dropped. `bucket(Bucket::new(key))` adds a bucket, described in one place: the key
that a request is charged to, its windows (`window`, known in advance; `learn`, from the headers of a
response, with the readers `counts` and `remaining`), its `cost`, and `hit_when`, which says whether a
`429` holds it. A `429` holds the buckets of the call that have no `hit_when` and those whose `hit_when`
returns `true`; when it holds none, every call waits. A call is charged to all its buckets or to none,
and a cost is never given back after the request left.

After a response is received, the limit never fails the call: a header that is absent or cannot be
read teaches nothing, spent budget stays spent, and a valid `Retry-After` still holds. A wait is
bounded by `max_hold` (60 seconds by default): a wait known to be longer fails at once with
`LimitError::MaxHold`, which `LimitError::find(&error)` finds in the error of the call. The memory
is bounded: `max_keys` keys across all the buckets (1024 by default; when room is needed, a key that
holds nothing is forgotten, and when every key holds something, one more is `TooManyKeys`), 8 windows
per bucket, 512 bytes per key, and 4,096 entries of history per rolling window, merged on the safe
side when full. No task is started. The limit is local to one process, and a budget that only the
body of a response states is not seen.

## Records

`read::JsonLines::new(max_record_bytes)` (feature `json`) and `read::Sse::new(max_event_bytes)`
read a body as records into a `read::RecordStream<T>`, which keeps the status and the headers. The
bound of one record has no default. A record over it, bytes that are not UTF-8 and a record that
does not decode are typed errors that hold no data of the response; a failure of the body is kept
whole. The first error ends the stream and releases the body. `try_next()` and `Stream` share the framing: a
`try_next()` that is dropped loses no byte, and stops the idle timer, as for `ByteStream`. The
timeouts watch the bytes that arrive, comments included. `Sse` follows the rules of server-sent
events and returns `retry` as a `Duration`; it never reconnects, and the names of error and
completion events belong to the application. `SseEvent::json` needs the feature `json`.

## A URL that the API returned

Describe the other host on its own, with its own `base` and none of the API's credentials or
middleware, then send its call with `transfer::at(uploads.send(bytes), &ticket.url)?.await?`.
`at` refuses a URL that would not be sent as it is written: a relative URL, a scheme other than
`http` and `https`, user information, a fragment, a spelling that parsing would change. Its error
never holds the URL. An accepted URL is sent with its path and query byte for byte (a lone `?`, or
a query of `&` only, included), for a description with no `Secret` query entry; the engine refuses one at another origin than the base
of that call before anything is sent. Headers that the API returned at run time are added with
`.edit_request(move |request| request.headers_mut().extend(returned.clone()))`, which replaces the
names given and keeps the repeated values of a name. The module's example shows the three steps.
No redirect is followed, no upload session protocol is run, and the call keeps the retry, timeout
and body rules of every generated call.

Minimum Rust version: 1.96.
