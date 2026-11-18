# Scope and limits

*[Concord](../../README.md) · [docs](../README.md) · design · previous: [alternatives](alternatives.md) · next: [prior art](prior-art.md)*

This page is for a developer deciding whether to adopt Concord. It covers what is in and what is outside, with the escape hatch for what is outside; how a need becomes a value, a recipe or nothing; what was traded for what; what can come later without breaking anyone; the known limits; and the stability that you can count on. Read it before you decide.

## What is in, and what is outside

| | What |
|---|---|
| **In** | The tree, the signature, the block, parts. The body that is sent, as a value. The response: the simple rule, a reader, a status that means something, the API's own error. Authentication: a secret value, placed like any other value. What every request goes through: `middleware`. Retry. Pages. The timeouts. Three promises and seven safe defaults. Beside the core, in `concord-extras`: four middleware, a token that is renewed (`concord::auth`, feature `auth`), a refresh token that the server replaces at each use (`concord::auth::Rotating`, feature `rotating`), AWS Signature Version 4 (`concord::sign`, feature `sign`) and rate limits (`concord::limit`, feature `limit`); writers of nested forms, gzip bodies, MIME multipart bodies and JSON Lines; readers of records in JSON Lines and server-sent events; a call on a URL that the API returned (`concord::transfer`). |
| **Outside** | Concord has a place for a signer of another protocol (a provider's own HMAC proof, OAuth 1, HTTP message signatures), a flow that obtains a token (OAuth2 and the like), a retry budget for a whole program and limits shared between processes, but ships none of them: each is a middleware that you write. The store of a refresh token that rotates, and a lock on its account between processes, are the application's. A budget that only the body of a response states is not read by the limit. A reader of CSV, of binary frames or of multipart responses, a redirect that is followed, a pool of hosts and an upload session protocol are outside too; the guide has a recipe for binary frames, a multipart response, a documented redirect and several hosts. A cache of responses. A blocking client. A browser (`wasm`): there the browser decides about redirects and timeouts, so the promises that rely on them (see [what Concord promises](../guide/promises.md)) cannot be made. |

The test that sorts them: **is it a fact about the API that every caller shares, or state that lives across calls?** A fact goes in the description. State goes in a Rust value of the application, and each layer has an escape hatch for it ([every layer has a way down](../guide/extending.md#extension-points-and-escape-hatches)).

## A value, a recipe, or nothing

This section is for a contributor who proposes something for Concord to ship. A need that the description cannot express ends in one of three places: a value in `concord-extras`, a recipe in the guide, or nothing in Concord at all. Each value is public surface that Concord keeps for as long as it exists, so the bar is high, and the three answers are decided by the same questions.

**A value** when the need holds state across calls, or follows a protocol that is easy to get wrong, and is the same for every API that has it. A user who writes it alone is likely to get an edge case wrong.

- **Renewal**: `auth::Renewed` shares one renewal among the calls that wait, and `auth::Rotating` saves a new refresh token before either token is used. Both orders are easy to get wrong, and OAuth is the same everywhere.
- **Limits**: `limit::Limit` keeps a wait after a `429`, a cap on calls in flight and budgets learned from headers, with a bound on its memory.
- **Signing**: `sign::SigV4` canonicalizes a request as AWS specifies, refuses to hash a stream, and never signs for another origin.
- **Record framing and bodies**: `read::JsonLines` and `read::Sse` frame records within a bound; `json_lines`, `gzip` and `multipart_related` write bodies whose format is fixed by a standard, not by an API.
- **A returned URL**: `transfer::at` refuses a URL that would not be sent as it is written.

**A recipe** when a few lines over public traits do the job, and the details differ from one API to the next. The recipe is complete code in the guide, compiled and run by the repository's tests, and the user reads it and changes it ([recipes](../guide/recipes.md)).

- **A provider's own HMAC proof**: each provider has its own string to sign, so the recipe uses `hmac` and `sha2`, and Concord ships only the two checks that such code gets wrong, `sign::body_bytes` and `sign::check_origin` ([a signature](../guide/credentials.md#a-signature)).
- **Binary frames**: Docker and AWS frame their streams differently, and `tokio-util` already reads both over a `ByteStream` ([binary frames](../guide/recipes.md#binary-frames)).
- **A key made for one write**: the header and the format of the key are the API's ([a key made for one write](../guide/retries.md#a-key-made-for-one-write)).
- **A documented download redirect**, and **several hosts**: which redirect to follow, and which host to try next, are the API's and the application's choices.

**Nothing** when the need belongs to one API, or to the application. Concord gives the place for it (an arm, a function of the author's, a middleware, a `when` rule), and ships no code.

- **The meaning of an API's events**: `Sse` frames events and gives them no meaning; the author's function reads them into the API's types ([the Anthropic example](../examples/anthropic.md)).
- **An error inside a `200`**, as Slack and Telegram send it: an arm on the 2xx, in the description.
- **The store of a refresh token** and a lock between processes: `Rotating` defines the trait `CredentialStore`, and the application implements it.
- **CSV**: `read Stream;` and a crate for the format.

A need of one API alone stays with that API, however often it is asked for.

## The tradeoffs

Each row is a place where no choice is perfect. It says what was chosen, what the other side would give, and the escape hatch for when you need the other side. An escape hatch is code that you write in your own project; none needs a change to Concord.

Many rows come from the real APIs in [the examples](../examples/README.md). This table is the short form. **The full case for and against each choice, with the strongest other design, is in [Alternatives](alternatives.md).**

**The language**

| What was chosen | The other side | The escape hatch |
|---|---|---|
| **An optional input is marked on its name:** `count?: u32`. | It is a mark that Rust does not have. `count: Option<u32>` would be plain Rust, but the rule would then depend on a spelling, which an alias hides. | — `x: Option<T>` with no mark is a required argument that may be `None`. `x?: Option<T>` keeps "not set" apart from "set to nothing", for the author's own code. |
| **An input is written twice:** in the signature and in the block. | Placement on the parameter (Retrofit, OpenAPI) writes it once, but puts the wire in the signature. | `query { count; }` keeps the second mention to one word. Inputs that many nodes share are written once, in a part. |
| **A part is small:** a named group of inputs, with the statements that place them. | It holds no `path`, no `problem`, no `middleware` and no child, and it names no other part. (It may hold a `base`: an input that goes in the host comes with the line that places it.) | — |
| **What an argument accepts is a closed list:** the type, a reference to it, a string for a `String` or a `Secret`, an array or a slice for a list. | `ids("p-1")` does not compile for `puuid: Puuid`. | `Puuid::from("p-1")` at the call, or the author declares `puuid: String`. |
| **An input has a Rust name.** | A key of the API that is a Rust keyword (`type`, `ref`) cannot be the name of an input. | Use another name in the signature, and write the key in the entry: `"type" = kind;`. |
| **The words of the language are keywords only at the start of a statement.** | An input may have the name of any word of the language ([grammar](../reference/grammar.md)), and `query { query; }` is legal. | — |
| **An input shadows an item with the same name in the expressions of its node.** That is Rust's own rule. (The name before the block in `body f { … }` is a path, not an expression, so an input does not shadow it.) | `body form(form);` does not compile. This is why the shipped pagers are named `by_…`: APIs name their inputs `offset`, `cursor` and `page`. | Name the item by its path: `body concord::form(form);`. |
| **A short entry in `headers` whose name contains `_` is rejected.** | `headers { user_agent; }` would be shorter, and would send a header named `user_agent`. | `"user-agent" = user_agent;`. In `query` the short entry is unchanged. |
| **The tree is the public API.** One level is one call. | Moving an endpoint to another scope changes the calls that callers wrote. | Your own methods beside the generated ones give any shape you like. |
| **`path` adds whole segments.** | A suffix after a resource (`Messages.json`, `Messages/{id}.json`), or a prefix with no `/` after it (`conversations.` before `list`), cannot be split between a scope and its endpoints. | The scope has no `path`; each endpoint writes the whole name. |
| **An input belongs to the node that needs it.** | A credential that only one branch uses creates a scope that is not in the URL. | That scope is a real one: it adds an input. |
| **An input of a scope has one type.** | A route type that is narrower for some endpoints below. | A doc comment; or another scope, with the narrower type. |
| **An input is required or optional on its own.** | The language cannot say "exactly one of these inputs". | None in the description: the API rejects the call. |
| **Required arguments are positional.** | Two strings side by side can be swapped without any error. | A type for each, as anywhere in Rust. |
| **A call owns its data.** An argument passed by reference is cloned. | Borrowing would save the copy, but tie the call to a lifetime. | Pass the value itself: nothing is copied. |
| **The description names every key.** | A header whose name the caller chooses (`x-amz-meta-*`), a header computed from the bytes of the body, a map of query pairs. | The author's own method on the call, with `.edit_request(…)`: about ten lines. Reading is open: all the headers of a response are in `headers.as_map()`. |
| **The block is the whole wire, as written.** A key in a different case from its input takes the long form, once for each key: `"labelSelector" = label_selector;`. | It is the one repetition that some real APIs still have (Kubernetes most of all). A word that changes the case of every key would teach the macro a vocabulary. | A part keeps it to once. |
| **A joined list sends its key even when it is empty.** | `labels.join(",")` of an empty list sends `labels=`. | `labels?: Vec<String>` and `"labels" = labels.as_ref().map(\|l\| l.join(","));`. |
| **A URL is built from parts.** `base` takes no query, and `at_url` sets a path and a query, never an origin. | A URL that the server returns whole (a signed upload URL with its query) needs a second description. | On the same origin: `call.at_url(url)`. On another origin it is no longer this API: describe that host on its own, with its own `base` and none of your keys, and send its call with `concord::transfer::at(call, url)`, which refuses a URL that would not be sent as it is written. `.edit_request` on a call of the API would carry every header that the description places to that origin. |
| **A value is always raw text**, and Concord encodes it. | An id that the provider returns already encoded is encoded again. | Your own type that decodes it once. |
| **A value stays where it is placed.** A `{*x}` placeholder keeps empty segments: `folder/` and `a//b` are sent as they are. | A segment that is `.` or `..` cannot be sent: the URL library under `reqwest` removes it, when it parses a URL and in every setter of a path, and has no supported, portable way to keep it. Concord refuses such a path rather than change it. | None inside a description: the request is rejected when it is built. For an upload to an object store with such keys, a form upload that carries the key as a field of the form, as S3's does. |
| **`with_base` moves every request to one place.** It replaces the whole base; the `path` lines stay. | An API on two hosts (with an upload host) cannot have them moved separately. | In the description, the second host is a `base` of its own: on the one endpoint that lives there, or on a scope. |
| **A secret does not follow a change of `base`** ([rule 16](../reference/rules.md#behaviour)). | An upload host that takes the same key needs one more line. A `path` with a secret cannot be written again or removed. | Under the new base: `override "authorization" = "Bearer {client.token}";`, or `remove`. For a `path`: write it under each `base` that needs it, not above them. |
| **No `Accept` header from a reader.** Only what is written is sent (`reqwest` adds its own `accept: */*`). | `curl --json` and most SDKs send one. An API that returns XML unless asked otherwise needs it. | One line on the client: `headers { "accept" = "application/json"; }`. |
| **`json(x)` writes, `Json` reads.** | Two names that differ by one capital letter. | — It is plain Rust naming: a function and a value. |
| **An arm binds what it uses:** `200 { body } => …`. | Unlike in a Rust struct pattern, fields are left out without `..`. | — |
| **An inherited arm that binds a body does not apply to a `HEAD`.** | It is one exception in the rule of inheritance. | — It is checked when the description compiles; nothing about it is implicit. |
| **Arms are compared by the statuses they match, not by how they are written.** | An exact status under a class arm from above needs `override`, and `override 2xx` over exact arms from above is rejected. | `override 404 => None`; `override` or `remove` for each exact arm. |
| **An `Option` or a `Result` after the arrow is read as its inner type**, and recognized from a closed list of spellings ([C1.7](../reference/contracts.md)). | An alias after the arrow is not recognized: it is read whole. | Spell `Option<…>` or `Result<…, …>` after the arrow. |
| **With no arrow, nothing is read.** An endpoint with no arrow, under an arm that would read a body, does not compile. | Under a client whose arm reads every 2xx, an endpoint that returns nothing still needs an arrow or a `remove`. | An arrow (`-> Ignored`, or your own type that reads anything), or `remove` for that arm. |
| **Behaviour is opaque.** | A tool can read all of the placement, but not what `body`, `read`, `paginate` or an arm do: they are Rust. A description can be generated from OpenAPI; going back loses those. | — |
| **The macro knows no format, no authentication scheme and no pager.** `json`, `Json`, `by_offset` are ordinary Rust, imported. | A description starts with a `use` line. A typo in `read Jsno;` gets a message from `rustc`, not from Concord. | — It is what lets your own reader, writer or pager take the same place, and it keeps the macro small. |
| **No format by default.** | One more line on every client: `read Json;`. | — The core has no format, and an API that speaks another one writes its reader in the same place. |
| **`by_offset` and `by_offset_limit` are two names.** | Rust has no function that takes either one or two arguments. | — |
| **In `paginate`, an input is the input itself, as a slot.** | Everywhere else in a block, the name of an input is its value, so this is easy to misread. | A wrong use does not compile. A pager written in place receives the call, and sets an input with its method: `paginate move \|call, page\| …`. |

**The response and the error**

| What was chosen | The other side | The escape hatch |
|---|---|---|
| **One error type for a client.** | One type for each endpoint (the AWS SDKs). An API that rejects requests in several shapes. | What one endpoint can return goes in its arrow: `-> Result<Charge, Declined>`. Another shape of error response: an arm that reads it and returns the client's problem. |
| **The type after the arrow is fixed.** | `expand`, which changes the shape of a response, cannot change the type through an option. Nor can a `GET` that may be conditional. | For `expand`: the author's type has both shapes. For a conditional `GET` or `HEAD`: `send_if_none_match(etag)`, which returns an `Answer<Option<T>>`, `None` on a `304`. |
| **`form` is flat, and `bracket_form` has one convention for nested keys:** `a[b]` for a map, `a[0]` for every list. | Rack and Rails read `a[0]=x` as a map with the key `"0"`, and other APIs have other conventions. | Your own function that returns a `Body`, built on `bracket_pairs` or not. |
| **A writer is a function that returns a `Body`, with one convention each.** `bracket_form` and `bracket_pairs` take what `form` takes; `gzip` compresses a body held in memory; `multipart_mixed` and `multipart_related` write parts with their own headers; `json_lines` writes one value per line, under a bound on the whole body. | `gzip` refuses a stream, and the description writes `content-encoding` itself: a body carries no header but its content type. A provider's own multipart convention (another root, a `start` parameter, an embedded request built for you) is not written. | Your own writer, a function that returns a `Body` ([a writer of bodies](../guide/extending.md#a-writer-of-bodies)). |
| **With `body json(x)`, a body follows its type, not the rule of entries.** In an entry, a `None` writes nothing. | In a body that an expression builds, a `None` is whatever the type writes: `json!({ "cursor": cursor })` writes `null`. | A body made of entries, `body json { cursor; }`: there a `None` writes nothing, and `"k" = Some(x);` writes it as `null`. Or `#[serde(skip_serializing_if = "Option::is_none")]` on your own struct. |
| **A body made of entries is one value that `serde` serializes**, passed to the function before the block. The macro still knows no format. | A form has no `null`, and takes no list and no struct as a value. A struct that holds a `Secret` cannot be serialized: `body json(task)` with a secret field does not compile. | A `Secret` as an entry is written as its text: `body form { grant_type; client_id; client_secret; }`. |
| **A `Body::stream` is sent once, and a stream has no length.** | A one-shot stream cannot be retried. A server may reject a body with no length. | `Body::repeatable`, opened again for each attempt ([a body that is opened again](../guide/bodies.md#a-body-that-is-opened-again-for-each-attempt)); or a loop in the caller ([a body that is a stream](../guide/bodies.md#a-body-that-is-a-stream)). `"content-length" = size;` in the description. |
| **An edit sees a repeatable body as a placeholder.** `edit_request` runs once, before the first attempt, and must leave the body alone. Reading or removing the placeholder, or a detected replacement by a different kind of body, is a Build error. Wrappers and look-alikes cannot always be detected; if building succeeds, the declared repeatable body is sent. | An edit cannot sign or rewrite such a body. | A middleware, which sees each attempt's own stream. |
| **An error prints names, never values**, and its `source()` never holds a value from the response. | When you read a log at night, the values are what you want. | `e.details()`, when you ask for it: the kept body, and a reader's own message. |
| **An error is made by Concord.** | Your own method that wraps two calls cannot build a `riot::Error`. | Your own error type around it. |
| **`Text` is strict UTF-8.** | `reqwest` reads the declared charset and replaces what does not fit. | `Binary`, or your own reader. |
| **The page type is what the caller gets.** | A pager that needs `result_info` makes the caller see it too. | `.pages().items()`. |
| **A trait says what a page is:** `concord::page::IntoItems`. | The author's own list type needs a few lines before `.items()` and the counting pagers accept it. `Option` and `Result` never have it. | Implement the trait for your `List<T>`; `Vec<T>` already has it. |
| **A call with pages is not awaited directly.** | One more word for one page. And adding `paginate` to an endpoint later is a change that its callers see when they compile. | `.send().await` for one page; `.pages()` for all of them. |
| **A stream is bytes, unless a reader of records frames it.** `JsonLines` and `Sse` take a bound with no default, and the first error ends the stream. | An error record inside a stream, an event that ends a stream, and a reconnection are yours to handle. CSV and other formats are not read. | Match the record in your own type (`JsonLines` reads an `enum`); for another format, `read Stream;` and a crate for the format. |
| **Concord's own client does not decompress unless you turn it on.** A body is passed on as it arrived. | An API that compresses its responses only when asked. By default Concord asks for nothing: no `accept-encoding` is sent unless the description writes it, or unless decompression is turned on in `concord::Http::with(…)`, which then sends it. | Turn on the feature `gzip` or `deflate` of `concord-api`, and decompression in `concord::Http::with(…)` ([compression, proxies and TLS](../guide/calling.md#compression-proxies-and-tls)); a feature alone changes nothing. Or write the header and your own reader that decompresses, or bring your own `reqwest::Client` that does. A reader's limit counts the bytes that the transport delivers: the decoded bytes, once decompression is on. |
| **No redirect is followed.** | A download that responds with a redirect takes two calls. An API that moved a resource responds with `301`, and the call fails with it. | An arm for the `302`, or `send_raw()`, and a second request by a client with no credentials ([a documented download redirect](../guide/extending.md#a-documented-download-redirect)). Or your own `reqwest::Client` that follows them, given to `with_transport`: it is then outside [G3](../guide/promises.md), and a signed request is not signed again. `Http::with` cannot do it. |

**Credentials, retry, pages, timeouts**

| What was chosen | The other side | The escape hatch |
|---|---|---|
| **A status proves nothing about what the server did.** A `POST` that is not idempotent is not retried after a `429`. | Most APIs turn a request away with `429` before they do anything. | The author, who knows, tells the policy: `Retry::standard().not_applied_on(429)`, shipped as `slack::retry()`. Not `idempotent;`: that would also retry the request after a lost connection. |
| **Retry is small:** `none`, `standard`, `tries`, `not_applied_on`, `when`, `unless`. The waits are fixed. | There is no setting for the first wait or for the longest one. A wait over 60 seconds, requested by `Retry-After` or by a rule, means no further attempt. | A `when` rule returns a wait of your own, as a minimum. Past 60 seconds: a loop of your own, with `e.retry_after()`. |
| **`with_retry` on a call replaces the client's whole policy for that call.** Rules add up only inside one policy. | The author's rules are not consulted for that call. | Pass the author's policy on: `with_retry(github::retry().when(mine))`. |
| **Retries are off until you turn them on.** | `requests` leaves it off too. Stripe's library went from 0 retries to 1 in 2023. | One line: `with_retry(Retry::standard())`. |
| **`PUT` and `DELETE` are idempotent, as HTTP says.** | An API that breaks that promise would see a write twice. | `idempotent false;` on that endpoint. |
| **An API's own "try later" is a value, not a statement.** | A statement would apply it for every caller. | The author ships `pub fn retry() -> Retry<…>`; the application names it once (see [retries](../guide/retries.md)). |
| **After a failure where the request may have been processed, only an idempotent endpoint is retried**, even when the API said "later" inside a `200`. | The API might mean "not done", and Concord cannot know. | A loop of your own, where you know. |
| **A rejected certificate is retried like any failed connection.** | Concord cannot tell the two apart from what `reqwest` reports. | Attempts are few. |
| **What every request goes through is a `middleware`, on the client.** It is made once, in `new`, runs for every attempt and every page, and nothing takes it away. `with_transport` replaces the transport, beneath it. A caller's middleware runs first and the description's last, so nothing changes a request after it is signed. | It cannot be placed on a scope or an endpoint, and no node below can switch it off. Its expression sees only the client's required inputs, and cannot fail. | An API whose public endpoints must not be signed: a middleware that inspects the request, or `info.endpoint()`. What can fail is done in `handle`. A caller adds their own with `with_middleware(…)`; it sees the request before it is signed, and their own transport sees it as it leaves. |
| **A secret is read when the request is built.** | A key that is replaced while a call waits between two attempts is not in the later attempt. | The next call has it. A middleware reads what it wants at each attempt. |
| **A token that is renewed never sends a request again by itself.** `concord::auth::Renewed` returns a `401` as it is, and `after_401` asks the engine for one more attempt. | An unmarked `POST` sent with a token that the server had already revoked is not recovered: OkHttp's `Authenticator` would send it again. | `not_applied_on(401)` in the policy when the API promises that a `401` applied nothing, or `idempotent;`; or a loop in the caller. |
| **A refresh token that rotates is renewed by a future that the application polls.** `Rotating::new` returns the middleware and its `RotationDriver`; Concord starts no task. | One more value to keep: a call that waits while nobody polls the driver fails with `NotPolled`, and a dropped driver stops the credential. A library that starts its own task needs no such step. | The application spawns it (`tokio::spawn`, as the example of `Rotating` does), and saves by its own means a pair that the driver hands back in `RotationError::Unsaved`. |
| **The `renewal_timeout` of `Rotating` bounds the wait of a call, never the exchange.** Cancelling an exchange that was sent can lose the replacement token. | A token endpoint that hangs holds the account until it answers. | The timeout of the client that calls the token endpoint (`with_timeout`). |
| **The shipped signer is AWS Signature Version 4.** `SigV4` is a middleware for one origin and one region; `SigV4::sign` signs one request for its origin, in a region chosen per call; `SigV4::sign_any_origin` signs one request without checking its origin, for an adapter whose hosts are known per call, and that adapter checks the origin. | No other protocol is shipped, and no digest helper: a provider's own proof is written with `hmac` and `sha2`. | A middleware of your own, with `sign::body_bytes` and `sign::check_origin` ([a signature](../guide/credentials.md#a-signature)). |
| **A limit is local to one process, and reads only the head of a response.** | Two processes that share one budget each think they have all of it. A budget that only the body of a response states is not seen. | Windows known in advance, below the shared budget (`Bucket::window`); the `429` and its `Retry-After` still hold the buckets. |
| **After a response is received, a limit never fails the call.** | A header that cannot be read teaches nothing, so the limit may let through more than the server allows, and the next response is a `429`. | That `429` holds the buckets that it hit, when it has a `Retry-After` or the limit sets a wait for one that has none (`without_retry_after`); with retries on, the engine sends the request again, through the same limit, when it may be repeated. |
| **A `POST` that is not marked is not retried after a failed connection.** A transport says what failed; only `blocked()` or `not_sent()` says that nothing was sent, and the retry policy does not read `not_sent()`. | Go's client retries when nothing was written. | `idempotent;` when the server supports idempotency keys and the same key is sent again; or a loop in the caller, where you know what a repeat does. `Connect` alone is never evidence that a request may be sent again. A token exchange goes through `RenewalError::from_call`. |
| **Basic credentials are encoded as UTF-8.** | `requests` and OkHttp use Latin-1; RFC 7617 leaves it open. | For a legacy server: write the header yourself. |
| **The same request twice in a row is an error.** | Following a log, where the same cursor means "nothing new yet"; a poll that sends the same request again (Telegram's `getUpdates` after an empty batch). | A loop of your own. |
| **A longer cycle, or a list that never ends, is not detected.** | Remembering every request costs unbounded memory. | Count the pages, or put one timer around the loop. |
| **A next URL is sent as the server returned it.** | A query pair that the description places is not added again. | Nothing to do for a credential: a pair whose value is a `Secret` is written on it, and a signing middleware sees whatever goes out. |
| **`by_offset` adds the number of items that came back.** | An API that wants the requested page size added (Box). | The function form: three lines. |
| **Pages yield values, not heads.** | A quota header on every page. | A middleware sees every response, with the name of its endpoint. |
| **Every page goes to the origin of the first request.** | An API whose next page lives on another host. | A loop of your own. |
| **After `send_raw()`, Concord's part is over.** | Neither timeout covers the `reqwest::Response` that you hold. | Your own timer around reading its body. |
| **A body held in memory goes out in one piece.** A stream body is a sign of life at each chunk; bytes in memory are not. | A large body from memory on a slow connection looks like silence. If it were handed over in chunks, a transport could not read it (a signer hashes it). | `with_idle_timeout(…)` on that call; or a stream. |
| **The idle timeout watches the transport and the reading of a body, and nothing else.** A middleware that waits is not a silent server, and neither is a reader's own wait. | A middleware that waits forever is bounded only by `timeout`, which has no limit until you set one. | `with_timeout(…)`; or a timer inside the middleware. |
| **An endpoint that is silent on purpose says so:** `idle_timeout <expression>;` on an endpoint, a scope, the client or a part. | One more statement, and one more place where a call's idle timeout can come from. | The order is fixed: `with_idle_timeout` on the call, then the statement, then `with_idle_timeout` on the client, then 30 seconds. |
| **One log event: `retry`.** | No event for each attempt and each response. | A middleware sees every attempt and every response. |
| **A timeout ends a wait; it cannot stop code.** | Your own code that blocks with no `.await` (in a middleware, a transport, a reader, a pager) is not interrupted by `timeout`. | Run it on a thread meant for blocking work, and `.await` it. |

**What Concord is built on**

| What was chosen | What it costs |
|---|---|
| **It is tied to `reqwest`.** `request()`, `edit_request`, a middleware and the transport all use `reqwest`'s types. That is what makes the escape hatches real. | A new major version of `reqwest` is a new major version of Concord. Another HTTP library would mean other signatures for those four. |
| **`concord::Http` lets `reqwest` resend nothing on its own** (`retry::never()`, after your settings); its connection pool still sends again a request that an unusable reused connection handed back unwritten, which never puts a request on the wire twice ([what can send a request twice](../guide/retries.md#what-can-send-a-request-twice)). | A `GET` with no retry policy fails after an HTTP/2 refusal that `reqwest`'s default would have resent: `Retry::standard()` repeats it, counted. A `reqwest::Client` of your own resends as its policy is configured, below the count. |
| **Async only, on Tokio.** `tokio` is in no public signature of `concord`. The core uses only its timer; so does the credential of `auth`, whose renewal is a shared future of `futures-util`; `rotating` adds its `sync` feature, for the calls that wait for the driver; `sign` reads its clock, so that a signature follows virtual time in tests; the limit of `limit` uses its timer and, to wake the calls that wait, its `sync` feature. | A blocking program runs it on a runtime of its own. Concord needs a Tokio runtime, as `reqwest` does: **sending a call needs a Tokio 1.x runtime with its time and IO drivers**; polled elsewhere, the send panics with Tokio's own message. Building a client and a call needs no runtime. The front page of `concord` says so under "Panics": outside the test helpers, it is the one panic that the library documents. `concord-testkit` names Tokio's `Instant` (`script::Request::at`), so Tokio 1 is part of the surface of the test kit ([what the public surface is tied to](../reference/crates.md#what-the-public-surface-is-tied-to)). |
| **After a drop, Concord promises only that it stops,** and starts no further attempt. | The request may have reached the server. What the HTTP library does with the connection is up to the library. |
| **A fake does not test the network.** | Real waits and redirects are tested with real sockets on `127.0.0.1`. No test calls a real API. |
| **A crate named like its client.** | Inside a crate named `riot`, `use riot::*;` is ambiguous: write `use ::riot::*;`. |
| **Choosing the tree is still a choice.** | Four pieces of advice narrow it ([choosing the tree](../guide/describing-an-api.md#choosing-the-tree)). They do not remove it. |

## What can come later without breaking anyone

None of these is in Concord today, and none has been ruled out. Each can be added later, and what compiles today keeps its meaning.

| Can come later | What does the job now |
|---|---|
| A default written in a signature (`version: String = "2026-01"`) | Fixed text for a pinned value; an optional input (`version?: String`) for the rest. |
| A name for a `base`, moved by its name | `with_base` moves every request; a second host is a `base` of its own in the description. |
| `middleware` on a scope | A middleware on the client that looks at `info.endpoint()`. |
| A part with a type parameter | The inputs written on the node. |
| Several keys on one arm (`2xx \| 301 { … }`) | The same arm, written twice. |
| A rest entry (`query { .. }`) | Each input named in the block: `query { start; count; }`. |
| `fake` that also receives the `TryInfo` | Your own type with `Transport`: its `send` receives the `TryInfo`. |
| Redirects followed by the engine | An arm for the `3xx`; or your own `reqwest::Client` that follows them. |
| A map of pairs as one input (the metadata headers of an object store) | The description names every key; the author's own method with `.edit_request(…)` covers the rest. |
| A part that holds scopes and endpoints (the same resources under two parents) | The scopes written under each parent. |
| One endpoint under two methods (a search as `GET` and as `POST`) | Two endpoints, with two names. |
| A `path` that replaces the paths above | The shared `path` is not written above: each node below writes its own. |
| A pager that may say "again" (an endless poll) | Pages stop at the same request twice, so the poll is a loop in the caller. |
| A `base` that holds a query (a signed URL that the server returns whole) | A second description for that host, and `concord::transfer::at(call, url)`. |
| A statement that names your own function mapping a name to its key (`keys camel;`), which the macro copies like every right-hand side | The long form, once for each key: `"labelSelector" = label_selector;`. A part keeps it to once. |
| A scope with a type parameter (a typed Kubernetes custom resource) | One scope for each type, or an untyped value. |
| A helper that tracks the location of a failure through any `serde` format, for your own reader | The reader builds its `Place` one component at a time, or gives none (see [reading a success](../guide/responses-and-errors.md#reading-a-success)). |

## Not in Concord, and why

| Not in Concord | The reason, and what does the job |
|---|---|
| `keep`, to send a key whose value is `None` | Your own type whose `Serialize` writes an empty string. |
| A reader of CSV records | `read Stream;` yields the bytes, and a crate for the format splits them into records. `JsonLines` and `Sse` read JSON Lines and server-sent events. |
| A binary-frame reader | `read Stream;`, `tokio_util::io::StreamReader` and `LengthDelimitedCodec` split bounded frames; provider checks and terminal events belong to the application ([binary frames](../guide/recipes.md#binary-frames)). No fixed amount of work per poll is promised by that adapter. |
| A multipart response reader or a batch dispatcher | `multer` under a whole-response limit reads finite replies; `multipart_mixed` writes the request. Correlation, nested changesets and replay policy stay in the application, and completed replies survive a later failure ([the recipe](../guide/recipes.md#a-multipart-response-and-its-batch)). A per-field limit alone does not protect parser memory. |
| An incremental JSON document reader | `read Json.limit(…)` reads the complete document into its type, an array into `Vec<T>`. The bound is for the whole body; there is no incremental JSON reader. |
| A host pool or automatic failover | Separate clients declare each trusted origin and its credential. The application's loop chooses repeatable reads, disables engine retries and puts one timeout around the whole ([several hosts](../guide/recipes.md#several-hosts)). Each new call otherwise starts its own attempt count and timeout. |
| The content-type check by default | `read Json.strict();` for those who want it. |
| A raw call on a handle (`handle.call(method, path)`) | It would be a second way to make a request, with its own rules for what it inherits, and no real API needs it. An endpoint in the description, or your own method beside the generated ones, does the job. The name `call` stays reserved, so that it can come later without breaking anyone. |

## Known limits

- **The crates are not on crates.io yet.** The version is `5.0.0-rc.1`.
- **No test calls a real API.** Real waits and redirects are tested with real sockets on `127.0.0.1`, and no test opens a TLS connection. The descriptions of real APIs in [the examples](../examples/README.md) are compiled, not called.
- **Editor support inside `api! { }` has not been verified.** What the macro does for an editor, and what is still unknown, is in [the editor](../internals/macro.md#the-editor).
- **One client size has been measured: 500 endpoints.** Its first `cargo check` time and its peak memory are in [size and compile time](../internals/macro.md#size-and-compile-time); larger clients have not been measured.

## What can change without breaking

A project ages well when it can add things for years without breaking anyone. These are the lines that Concord keeps.

**For the author of a description.** The callers of a generated client are not broken when the author:

| The author… | Because |
|---|---|
| adds an endpoint or a scope | It is a new method. |
| adds an optional input | It is a new method on the call. Nothing else moves. |
| adds `idempotent`, a header, a query pair | The signature does not change. |
| moves optional inputs, with their statements, into a part | A part generates nothing by itself. The node has them as if it had written them. |
| changes a `path`, or the placement of an input | The wire changes; the call does not. |
| marks a node `#[deprecated]` | The mark goes on the node’s type and the method that returns its handle or call. For a scope, it also goes on the module. For an endpoint, it also goes on its setters on `Call<E>` and the setter functions on the endpoint's type. |

They **are** broken when the author adds or reorders a required input, changes a type, moves or renames a node, adds an arm that changes the type after the arrow, or adds `paginate` to an endpoint: its call can then no longer be awaited directly, and its callers see it when they compile.

**For Concord itself.**

| | |
|---|---|
| The methods of a call | An API option keeps its own name. The reserved names are exact names, per receiver: on a call, the methods that a call has (`send`, `request`, `edit_request`, `read_response`, `pages`, `at_url` and the others, plus the two that it has from Rust, `into_future` and `clone`). The whole table is [C1.6](../reference/contracts.md). One prefix is reserved for a name that becomes a method (an optional input, a child): `with_`. And the promise that goes with it: **Concord adds no name to a call, a handle or a client in a minor version, unless it starts with `with_`.** So an API may name its endpoints `send_message` and `send_photo`. A required input is an argument, and may have any name. |
| The methods of a client and of a handle | `call` and `clone`; on a client, also `new` and every name that starts with `with_`. `call` is reserved for a raw call that may come later. |
| The public enums | `Kind`, `Class` and `Kept` are `#[non_exhaustive]`: a `match` on them needs a `_` arm. |
| The words of the language | Twelve statements and one declaration (`part`). A new statement or a new method keyword is an addition: what compiles today keeps its meaning. A new reader, writer or pager is not a keyword at all: it is an item of `concord-extras`, or of your own crate. |
| The crates | People depend on `concord-api`. It depends on `concord-core` and on `concord-macros` at exact versions, so the macro and the engine that it generates code for always come as a pair. |
| The log event | Its name and its fields are a contract (see [what Concord logs](../guide/promises.md#what-concord-logs)). |
| `Retry::standard()` | Its numbers are written in the contracts. Changing one is a breaking release. |
| `reqwest` and the other crates in Concord's signatures | Their types are in Concord's public surface, so their major versions are Concord's: a new major version of `reqwest`, `http`, `bytes`, `url`, `futures-core`, `serde`, `tokio` (in the test kit), `tower-service` (feature `tower`), or of `serde_json` or `serde_urlencoded` (reached by a downcast of a writer's error), in that surface is a new major version of Concord. Where each one appears, and in which version: [what the public surface is tied to](../reference/crates.md#what-the-public-surface-is-tied-to). |
| The features | A feature only adds: turning one on never removes or changes an item. What a feature of `reqwest` turned on by another crate can change on the wire is on the same page. |
| The Rust version | Rust 1.96 or later. The minimum is raised only in a minor release, never in a patch release. |
