# Describe an API

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [How Concord sees an API](model.md) · next: [Call a client](calling.md)*

This page is for the author of a client. It shows how to describe an API in `concord::api!`, statement by statement. Throughout, the signature is for the caller, and the block is for the wire.

## The principles behind the language

| # | Principle |
|---|---|
| P1 | **The text is the tree.** |
| P2 | **The signature is for the caller. The block is for the wire.** One word from the wire leads the line, the HTTP method, because it also tells the caller what the call does: `GET` reads, `POST` creates, `DELETE` removes. |
| P3 | **Declared in the signature, used in the block.** An input that is never used is an error. |
| P4 | **A small block.** It holds only what this level adds. |
| P5 | **Rust's form wherever Rust has one.** A signature is a function signature, with one mark of its own: `?` on an input that may be left out. |
| P6 | **Exceptions are written explicitly.** |
| P7 | **Placement uses Concord's keywords. Behaviour is a Rust value.** The language has no `if` and no loop. Where something must be *done*, the block names a Rust value that does it. |
| P8 | **The same statements at every level**, except the few that only one position can have ([one kind of node, three positions](model.md#one-kind-of-node-three-positions)). |

## A whole description

```rust
use concord::{by_offset_limit, Json, Secret};

concord::api! {
    /// The Riot Games API (League of Legends subset).
    pub client Riot(
        /// The API key. Sent on every request unless an endpoint removes it.
        key: Secret,
    ) {
        headers { "x-riot-token" = key; }
        read Json;
        problem RiotProblem;

        /// Accounts (regional routing).
        scope account(route: AccountRoute) {
            base "https://{route}.api.riotgames.com";
            path "riot/account/v1/accounts";

            /// The account behind a Riot ID (`name#tag`). `None` when there is no such player.
            GET by_riot_id(game_name: String, tag_line: String) -> Option<Account> {
                path "by-riot-id/{game_name}/{tag_line}";
                status { 404 => None }
            }

            /// The account of a signed-in player.
            /// Uses the player's own token instead of the API key.
            GET me(access_token: Secret) -> Account {
                path "me";
                headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
            }
        }

        /// Matches (regional routing).
        scope matches(route: MatchRoute) {
            base "https://{route}.api.riotgames.com";
            path "lol/match/v5/matches";

            /// Match ids of a player, newest first.
            GET ids(
                puuid: Puuid,
                start?: u32,
                count?: u32,
                queue?: i32,
                start_time?: i64,
            ) -> Vec<MatchId> {
                path "by-puuid/{puuid}/ids";
                query { start; count; queue; "startTime" = start_time; }
                paginate by_offset_limit(start, count);
            }

            /// One match.
            GET get(id: MatchId) -> Match { path "{id}"; }
        }
    }
}
```

The types are plain Rust, written beside the macro:

```rust
#[derive(Clone, Serialize, Deserialize)] pub struct Puuid(String);

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchRoute { Americas, Asia, Europe, Sea }     // written `europe` in the host
```

`Secret` is a type from the `concord` crate, used like any other type. `Json` and `by_offset_limit` are ordinary items of the same crate, brought in by the same `use` line.

Hide the blocks, and what remains is exactly what a caller needs:

```text
client Riot(key: Secret)
    scope account(route: AccountRoute)
        GET by_riot_id(game_name: String, tag_line: String) -> Option<Account>
        GET me(access_token: Secret) -> Account
    scope matches(route: MatchRoute)
        GET ids(puuid: Puuid, start?: u32, count?: u32, queue?: i32, start_time?: i64) -> Vec<MatchId>
        GET get(id: MatchId) -> Match
```

This is what `riot.matches(Europe).ids(&puuid).count(5)` puts on the wire:

```text
GET https://europe.api.riotgames.com/lol/match/v5/matches/by-puuid/<puuid>/ids?count=5
x-riot-token: <key>
```

Concord adds no header of its own to this request. (The HTTP library adds `accept: */*`.)

The first example of [the tour](tour.md#an-api-is-a-tree-write-it-as-one) shows the smallest client, with no input and no scope.

## A signature

| Written | Meaning |
|---|---|
| `GET`, `POST`, … | The HTTP method of the endpoint. |
| `puuid: Puuid` | Required: an argument. |
| `queue?: i32` | Optional: a setter `.queue(…)`. Nothing is sent when it is not set. |
| `labels: Vec<String>` | A list. In a query, the key is sent once for each item. `labels?: Vec<String>` is an optional list. |
| `key: Secret` | Text that is never printed. |
| `..page` | The inputs of a part, after the node's own ([a part](#a-part-the-same-inputs-on-many-endpoints)). |
| `-> Vec<MatchId>` | What the call returns. |
| `-> Option<Account>`, `-> Result<Merged, Refused>` | A 2xx response is read as the inner type, and returned as `Some` or `Ok`. The arms produce the other values: `404 => None`, `409 { body } => Err(body)`. |
| no arrow | The call returns `()`: the status is the whole result. |

Apart from the method, nothing about the wire appears in a signature. An input accepts `///` doc comments and no other attribute, and so does a part. `#[cfg]` may be placed on a node, and `#[deprecated]` and `#[doc(hidden)]` on a scope or an endpoint ([message 14](../reference/messages.md)).

**The type of an input must be `Clone + Send + Sync + 'static`**, so it cannot borrow: a call owns its inputs, and can be cloned, sent more than once and moved to another task ([contracts](../reference/contracts.md) C1.8). So a struct used as a body derives `Clone` too: `#[derive(Clone, Serialize)] struct NewTask { … }`. Without it, `rustc` says ``the trait bound `NewTask: Clone` is not satisfied`` ([troubleshooting](troubleshooting.md#it-does-not-compile)).

**An optional input is marked on its name: `count?: u32`.** It is then not an argument but a setter with the same name, on the call (or on the client or the scope handle, for an input of a client or a scope). The declared type is the type of the value when one is given; in the block, the name is a `&Option<u32>`. A type alias changes nothing, because the mark makes the input optional, not the type. An optional input cannot fill a placeholder in a `path` or a `base`.

**The mark and `Option` together allow three states.** `milestone?: Option<u64>` is either not set, set to `None`, or set to a number. A JSON body of entries ([the block](#the-block)) serializes the three states as they are: the key left out, `null`, the number. In a query, a header or a form, the first two both send nothing. Without the mark, `x: Option<T>` is a required argument that may be `None`. See [arguments](calling.md#arguments) for what each setter accepts.

A signature has no default values. A filter that the provider documents as optional is marked with `?`, even when its documentation names a default: that default belongs to the provider, and it can change. When nothing is given, nothing is sent.

**The name of an input** may be a keyword of the language: GitHub has `scope contents(path: String) { path "contents/{*path}"; }`, because those keywords are keywords only at the start of a statement. It may not be a Rust keyword: GitHub's `type` becomes `kind?: RepoType` in the signature, and the entry spells out the key, `query { "type" = kind; }`. An input also shadows a Rust item of the same name in the expressions of its node, as a variable does in Rust: with an input named `form`, the function is written with its path, `body concord::form(form);`.

## The block

**Placement: where something goes.**

| Statement | Says | Position |
|---|---|---|
| `base "https://{route}.api.example.com";` | Where the URL starts: the scheme, the host, and optionally a path prefix (`"https://api.example.com/v1"`). An endpoint to which no `base` applies (its own or one from above) does not compile. | any |
| `path "a/{x}";` | The path segments that this node adds. It may end with `/`: the URL ends with `/` when the last `path` does, or the `base` when no `path` follows. | any |
| `query { … }` | Query pairs for this node and everything below. | any |
| `headers { … }` | Headers for this node and everything below. | any |

**Behaviour: what to do. Each statement takes a Rust expression.**

| Statement | Says | Position |
|---|---|---|
| `body json(task);` | The request body: the value of the expression ([a body that is sent](responses-and-errors.md#a-body-that-is-sent)). | endpoint |
| `body json { chat_id; text; }` | The request body, made of entries: one key for each (below). | endpoint |
| `read Json;` | How a response body is read ([reading a success](responses-and-errors.md#reading-a-success)). Written once on the client; an endpoint that differs declares `read Text;`. | any |
| `paginate by_offset(start);` | This endpoint is paginated ([pages](pages.md)). | endpoint |
| `status { 404 => None, … }` | What a status means here ([when a status means something](responses-and-errors.md#when-a-status-means-something)). | any |
| `middleware Signed::new(key.clone());` | What every request passes through as it is sent: a signature, a token that is renewed ([middleware](#middleware-what-every-request-goes-through)). | client |

**Three facts.**

| Statement | Says | Position |
|---|---|---|
| `problem RiotProblem;` | The type of the API's own error body. It is read with the client's reader; `problem S3Problem read Xml;` names a different reader. | client |
| `idempotent;` | Sending this request twice has no further effect. When nothing is written, the method decides: a `GET`, a `HEAD`, an `OPTIONS`, a `PUT` and a `DELETE` are already idempotent. | endpoint |
| `idempotent false;` | This `PUT` or `DELETE` must not be sent twice: GitHub's `PUT` that merges a pull request returns `405` the second time. The keyword may be followed by any Rust expression that evaluates to a `bool`, with the inputs as variables: `idempotent key.is_some();`. | endpoint |
| `idle_timeout Duration::from_secs(60);` | How long the calls below may stay idle: for a long poll, or a stream that is quiet for minutes. The nearest one applies. | any |

**An entry** in `query { }` or `headers { }`:

| Written | Meaning |
|---|---|
| `count;` | The key is `count`, and the value is the input `count`. |
| `"startTime" = start_time;` | The key as written; the value is an input. |
| `"v" = "2";` | Fixed text. |
| `"x-api-version" = "v{version}";` | A template. |
| `"labels" = labels.join(",");` | Any Rust expression: here a list, sent as one value. |
| `"labels" = labels.as_ref().map(\|l\| l.join(","));` | The same for `labels?: Vec<String>`: a comma-separated list that sends nothing when it is not set. |
| `"x-api-version" = version.as_deref().unwrap_or("2026-01");` | A default that the description owns, for `version?: String`. |
| `override "k" = …;` | Replace an entry set above. |
| `remove "k";` | Stop sending an entry set above. |

An entry whose value is `None` is left out: in `query`, in `headers`, and in a body of entries (below). No `if` is needed.

Two rules about entries are checked, because a wrong guess would otherwise go unnoticed ([rules](../reference/rules.md)):

- **In `headers`, a short entry whose name contains `_` is rejected.** `headers { user_agent; }` would send a header named `user_agent`, so write `"user-agent" = user_agent;`. In `query`, the short form works as usual. (`host` cannot be a key of `headers` either: the host comes from the `base`.)
- **A secret does not follow a change of `base`.** Under a different `base` further down, an entry that holds a `Secret` must be written again or removed, as `upload_asset` does below ([a secret stays with its base](credentials.md#a-secret-stays-with-its-base)).

```rust
/// GitHub's uploads live on another host.
POST upload_asset(name: String, file: concord::Body) -> Asset {
    base "https://uploads.github.com";
    headers { override "authorization" = "Bearer {client.token}"; }     // or: remove "authorization";
    path "assets";
    query { name; }
    body file;
}
```

**A body made of entries.** When a request body consists of the inputs, it is written like a `query`. All of Telegram's methods are of this kind:

```rust
part formatted(parse_mode?: ParseMode, disable_notification?: bool) {
    body json { parse_mode; disable_notification; }
}

POST send_message(chat_id: ChatId, text: String, ..formatted) -> Message {
    path "sendMessage";
    body json { chat_id; text; "reply_markup" = markup_of(text); }
}
```

An entry is `name;` or `"key" = value;`, as in `query` (a template counts as a value here too), but without `override` or `remove`. The name before the block is a path to a function, resolved from where `api!` is invoked: `json`, `form`, or your own. It is called with the entries, and an input of the same name does not shadow it. A part may hold such a body, as `formatted` does. The node that names the part then writes either a body of entries with the same function, or no body, and the entries are merged, the part's first ([rule 25](../reference/rules.md)).

**Here too, a `None` is left out.** The outer level of `Option` means presence; whatever is inside is serialized as the format serializes it:

| Written | What goes out |
|---|---|
| `parse_mode;` for `parse_mode?: ParseMode`, or `"k" = value;` where the value is an `Option` | left out, or the value |
| `"k" = Some(x);` with `x: Option<T>` | `null`, or the value: the way to write a `None` as `null` |

A form has no `null`, so in a form an inner `None` produces no pair either. See the [grammar](../reference/grammar.md) for the syntax of such a body, and [rules](../reference/rules.md) 8 and 9 for the full table and what happens without a warning.

**A `Secret` may be an entry of a body.** It is serialized as its string, as in a query. For example, a token endpoint:

```rust
POST token(grant_type: String, client_id: String, client_secret: Secret) -> Token {
    path "oauth/token";
    body form { grant_type; client_id; client_secret; }
}
```

A struct that holds a `Secret`, however, is still never serialized: `body json(task);` with a secret field does not compile.

**An endpoint that is idle on purpose.** A long poll holds the request open on purpose, so its author declares how long it may stay idle. The expression is Rust, with the inputs as variables, and evaluates to a `std::time::Duration`:

```rust
GET get_updates(offset?: i64, timeout?: u32) -> Vec<Update> {
    path "getUpdates";
    query { offset; timeout; }
    idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
}
```

A caller's `with_idle_timeout` on the call still takes precedence over it ([timeouts](calling.md#timeouts)).

**An arm** in `status { }` is written like an arm of a `match`: `404 => None`. An arm that needs the response binds what it uses: `201 { body, headers } => …`. The fields are `body`, `headers` and `status`; naming `body` is what causes the body to be read, and `body: task` binds it under another name ([when a status means something](responses-and-errors.md#when-a-status-means-something)). An endpoint with no return type does not compile when an arm that applies to it, its own or an inherited one, reads a body: give the endpoint a return type, or remove the arm.

`override` and `remove` may also precede an arm: `override 2xx => …,`, `remove 404,`. They apply to whatever has a key, where a clash could otherwise go unnoticed. An arm is written, overridden and removed by its key. What an arm covers decides when `override` is needed: a class arm from above (`4xx`) covers every status in its class, so an exact status below it is written `override 404 => None` ([rule 13](../reference/rules.md)). A statement that holds a single value (`base`, `read`, `idle_timeout`) takes neither keyword: the nearest one is used. A `path` only appends ([rule 10](../reference/rules.md)).

## A part: the same inputs on many endpoints

Settings are shared down the tree, but some inputs repeat across branches: every Stripe list has the same two inputs and the same pagination, and every `POST` takes an idempotency key. A **part** is a named group of inputs, together with the statements that place them:

```rust
pub client Stripe(key: Secret) {
    base "https://api.stripe.com";
    headers { "authorization" = "Bearer {key}"; }
    read Json;
    problem StripeProblem;

    part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate by_cursor(starting_after, List::next_cursor);
    }
    part keyed(idempotency_key: String) {
        headers { "idempotency-key" = idempotency_key; }
        idempotent;
    }

    scope customers {
        path "v1/customers";
        GET list(email?: String, ..page) -> List<Customer> { query { email; } }
        POST create(customer: NewCustomer, ..keyed) -> Customer { body form(customer); }
    }
}
```

```rust
let page = stripe.customers().list().email("a@b.example").limit(50).send().await?;     // one page
let made = stripe.customers().create(new_customer, "order-7781").await?;
```

A part is written in the block of the client, after the statements and before the children. A node names it at the end of its inputs, with `..`, and then has the inputs and the statements of the part **as if it had written them**: `list` has `.limit(…)` and `.starting_after(…)`, and it is paginated; `create` takes the key as its second argument, and may be retried. Inside a part, a bare name refers to an input of the part, and `client.x` to an input of the client.

A part may hold a `base`, since a part exists to keep an input together with the statement that places it. Riot puts a route in the host of almost every scope:

```rust
part regional(route: Region) { base "https://{route}.api.riotgames.com"; }

scope matches(..regional) { path "lol/match/v5/matches"; … }
scope account(..regional) { path "riot/account/v1/accounts"; … }
```

A part holds inputs and statements, and no other element of the tree: no `path`, no `problem`, no `middleware`, no child. It generates nothing by itself. See the [grammar](../reference/grammar.md) for the exact syntax, [rule 25](../reference/rules.md) for the rule, and messages 35 to 38 in [the messages](../reference/messages.md) for the errors of an invalid part.

## The whole language

| Group | Words |
|---|---|
| Nodes | `client`, `scope`, and a method: `GET` `HEAD` `POST` `PUT` `PATCH` `DELETE` `OPTIONS` (`METHOD "…"` for another one) |
| Signature | `x: T`, `x?: T`, `..part`, `-> T` |
| Placement | `base`, `path`, `query { }`, `headers { }` |
| Behaviour | `body`, `read`, `paginate`, `status { }`, `middleware` |
| Facts | `problem`, `idempotent`, `idle_timeout` |
| Entries | `name;`, `"key" = value;`, in `query { }`, `headers { }` and `body json { }` |
| Arms | `404 => …`, `201 { body, headers, status } => …` |
| Exceptions | `override`, `remove` |
| Shared across branches | `part name(inputs) { }` |
| Templates | `"text {value}"`, `{client.value}`, `{*path}` |

Twelve statements and one declaration: that is the whole language, and the [grammar](../reference/grammar.md) fits it on one page.

## Details and edge cases

This section covers what the examples above do not show. [The rules](../reference/rules.md) give the short form to learn; [the contracts](../reference/contracts.md) give the exact behaviour, line by line.

**A template** is a string with `{name}` placeholders.

| Written | Meaning |
|---|---|
| `{puuid}` | An input of this node. |
| `{client.tenant}`, `{matches.route}` | An input of a node above, named with its owner. A bare `{tenant}` does not refer to it. |
| `{id}.json`, `v{version}` | A placeholder may fill part of a path segment. Its value can never add a segment. |
| `{*key}` | A value that is itself a path (`src/lib.rs`): its `/` characters are kept, and each segment is encoded. A segment may be empty, so an object key like `folder/` or `a//b` is sent as is. It is allowed in a path only. |
| `{{` and `}}` | A literal brace. |

**How a value is serialized.** A value is serialized by `serde`, in a body and also in a path, a query and a header, so a type has one wire form: an enum with `#[serde(rename_all = "lowercase")]` is `europe` in a host, in a query and in a JSON body. In a query or in headers, a `None` is not sent, and a list sends its key once for each item. In a path or a host, a value must be a single string. Anything else, such as a struct or a map, is rejected when the request is built ([rules](../reference/rules.md) 6 and 8).

When the wire form of a type is not what `serde` produces by default, use your own type with its own `Serialize` implementation. Dates are the usual case: one API wants RFC 3339, another wants seconds.

**In a behaviour, an input is a variable that borrows the input.** In `body json(task);`, the name `task` is a `&NewTask`. `"authorization" = basic(client.key, "");` refers to an input from above by its owner, with the same spelling as in a template: `client.key` is a reference too, like a bare name. A body made of several inputs is a body of entries ([the block](#the-block)). When its shape is not flat, write it as the JSON it is, `body json(json!({ "filter": { "query": query } }));`, and there a value follows its type: an optional input that was not set is a `None`, and `json!` serializes it as `null`. (`paginate` is the only place where the name of an input refers to its parameter on the call rather than its value; see [the shipped pagers](pages.md#the-shipped-pagers).)

**The macro copies the expression as written**, and the compiler checks it against a trait of the core. So your own reader, writer or pager can take the place of a shipped item. See [when an expression runs](../reference/grammar.md#when-an-expression-runs) for when each expression runs and what a `?` inside it does.

**What is rejected.** An invalid description does not compile, and the message says what to write instead: [the messages](../reference/messages.md) lists every one. A value that cannot be placed is rejected when the request is built, and nothing is sent. Two rejections concern the name of an input:

| Rejected | Write instead |
|---|---|
| A child or an optional input with the name of a method that its receiver already has (on a call: `send`, `request`, `pages`, `read_response`, …), or a name that starts with `with_`; see [contracts](../reference/contracts.md) C1.6 for the full table. A required input is an argument, so `create(request: ChatRequest)` is fine; so is a provider's own `timeout`. | Another name. The name on the wire can stay: `"send" = send_now;`. |
| An input named after an owner it can refer to: `client`, or a scope above | Another name. |

**Less obvious behaviour:**

| | |
|---|---|
| A placeholder in the host | It fills one label of the host, or part of one (`"https://api-{label}.example"`): `{route}` can be `europe`, never `evil.example/x`. |
| Two hosts in one API | Write a `base` on the scope, or on the single endpoint, that lives on the other host: GitHub's `upload_asset` in [the block](#the-block). The nearest `base` starts the URL, and every `path` from above is still appended after it. A secret from above does not follow. |
| A self-hosted server | `base` can take one input of the type `concord::Base` instead of text: `client Gitlab(url: Base, token: Secret) { base url; path "api/v4"; … }`. A `Base` is a scheme, a host, an optional port and an optional path prefix: `"https://example.com/gitlab".parse()?`. |
| A template that reads a `None` is not sent at all | `"x-range" = "{from}-{to}";` sends nothing if `from` is `None`. |
| `-> Option<T>` | A 2xx response is read as a `T`, and returned as `Some`. So a `200` whose body is `null` does not quietly become `None`, and `Option` does not mean "a `404` returns `None`": an arm says that. |
| `-> Result<T, E>` | A 2xx response is read as a `T`, and returned as `Ok`. An arm produces each error that the endpoint declares as its own: `409 { body } => Err(body)`. |
| `Option` and `Result` as the return type | They are recognized by a fixed list of spellings ([contracts](../reference/contracts.md) C1.7). An alias of either is treated like any other type. |
| An endpoint with nothing to add | `GET list() -> Vec<Task>;` |
| Another HTTP method | `METHOD "PURGE" purge(key: String) { path "cache/{key}"; }` |

## Choosing the tree

Two authors could write two different trees for the same API. Four pieces of advice lead them to the same one. None of them is checked, because an author may have a reason to break one.

| # | Advice | Why |
|---|---|---|
| 1 | **The tree follows the URL.** A scope corresponds to part of the provider's URL: a path prefix, a host, or both. | The URL is the one structure that the provider has already chosen. |
| 2 | **A scope adds something visible:** an input, a `path`, or a `base`. | A scope that only shares a header adds a level to every call, for nothing the caller can see. |
| 3 | **A scope is worth it when it holds an input, or at least two endpoints.** A single endpoint under a fixed prefix writes the prefix in its own `path`. | A single call does not need a level of its own. |
| 4 | **Names are the provider's words:** the scope is the name of the resource in the provider's documentation, and the endpoint is what the documentation calls the operation. | Nobody has to guess. |

See [scope and limits](../design/scope.md) for two trade-offs and their escape hatches: an input is written twice, once in the signature and once in the block; and a suffix after a resource (`Messages.json`, `Messages/{id}.json`) is written in each endpoint, because a `path` adds whole segments.

## Middleware: what every request goes through

Some facts of an API are not values to place but processes that run on each request as it is sent: a signature, a token that is renewed, a quota that all calls share. The description names such a process on the client:

```rust
pub client Vault(key: Secret) {
    base "https://api.vault.example";
    read Json;
    middleware Signed::new(key.clone());        // every request is signed as it leaves

    GET balance() -> Balance { path "balance"; }
}

// plain Rust, beside the macro: a type with one method
impl concord::Middleware for Signed {
    async fn handle(&self, request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        /* sign the request, then */ next.send(request, info).await     // on to the transport
    }
}
```

```rust
let vault = Vault::new(key);                                   // it signs: there is no other way to make one
let test  = Vault::new("k").with_transport(concord::fake(…));  // the fake gets the signed request
```

`middleware` appears on the client only, and may be written several times. A request goes first through the middleware that the caller added, then through the description's middleware layers in the order written, and then to the transport. So the author's layers are the last before the wire, and the author writes the signing layer last, so that nothing changes a request after it is signed. The expression can use the required inputs of the client and nothing else: an optional input is rejected. The value is created once, in `new`, so it can hold state that all calls share, and it runs for every attempt and every page. Nothing below the client can remove it.

See the [grammar](../reference/grammar.md) for the syntax and [rule 26](../reference/rules.md) for what you cannot guess. A complete middleware layer is written out in [a middleware](credentials.md#a-middleware), and [a middleware](extending.md#a-middleware) shows how a caller adds one of their own.

See [the examples](../examples/README.md) for eight real APIs described in this language.
