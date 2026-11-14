# Describe an API

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [How Concord sees an API](model.md) · next: [Call a client](calling.md)*

For the author of a client: how to write an API down in `concord::api!`, statement by statement. The signature is for the user; the block is for the wire.

## Principles

| # | Principle |
|---|---|
| P1 | **The text is the tree.** |
| P2 | **The signature is for the user. The block is for the wire.** One word of the wire leads the line, the method, because it also tells the user what the call does: `GET` reads, `POST` creates, `DELETE` removes. |
| P3 | **Declared in the signature, used in the block.** An input that is never used is an error. |
| P4 | **A small block.** It holds only what this level adds. |
| P5 | **Rust shapes where Rust has one.** A signature is a function signature, with one mark of its own: `?` on an input that may be left out. |
| P6 | **Exceptions are written.** |
| P7 | **Placement is Concord's words. Behaviour is a Rust value.** The language has no `if` and no loop. Where something must be *done*, the block names a Rust value that does it. |
| P8 | **Same statements at every level**, except the few that only one position can have ([one kind of node, three positions](model.md#one-kind-of-node-three-positions)). |

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

The types are plain Rust, beside the macro:

```rust
#[derive(Clone, Serialize, Deserialize)] pub struct Puuid(String);

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchRoute { Americas, Asia, Europe, Sea }     // written `europe` in the host
```

`Secret` is a type of the `concord` crate, named like any type. `Json` and `by_offset_limit` are ordinary items of it too, brought in by the same `use` line.

Hide the blocks, and what is left is exactly what a user needs:

```text
client Riot(key: Secret)
    scope account(route: AccountRoute)
        GET by_riot_id(game_name: String, tag_line: String) -> Option<Account>
        GET me(access_token: Secret) -> Account
    scope matches(route: MatchRoute)
        GET ids(puuid: Puuid, start?: u32, count?: u32, queue?: i32, start_time?: i64) -> Vec<MatchId>
        GET get(id: MatchId) -> Match
```

What `riot.matches(Europe).ids(&puuid).count(5)` puts on the wire:

```text
GET https://europe.api.riotgames.com/lol/match/v5/matches/by-puuid/<puuid>/ids?count=5
x-riot-token: <key>
```

Concord adds no header of its own to this request. (The HTTP library adds `accept: */*`.)

The smallest client, with no input and no scope, is the first example of [the tour](tour.md#an-api-is-a-tree-write-it-as-one).

## A signature

| Written | Meaning |
|---|---|
| `GET`, `POST`, … | The HTTP method of the endpoint. |
| `puuid: Puuid` | Required: an argument. |
| `queue?: i32` | Not required: a method `.queue(…)`. It is not sent when it was not set. |
| `labels: Vec<String>` | A list. In a query, the key is sent once for each item. `labels?: Vec<String>` is an optional one. |
| `key: Secret` | Text that is never printed. |
| `..page` | The inputs of a part, after the node's own ([a part](#a-part-the-same-inputs-on-many-endpoints)). |
| `-> Vec<MatchId>` | What the caller gets. |
| `-> Option<Account>`, `-> Result<Merged, Refused>` | A 2xx is read as the inner type, and given as `Some` or as `Ok`. The arms give the rest: `404 => None`, `409 { body } => Err(body)`. |
| no arrow | The caller gets `()`: the answer is the status. |

Apart from the method, nothing about the wire appears in a signature. An input takes `///`, and no other attribute; so does a part. `#[cfg]` is for a node, and `#[deprecated]` and `#[doc(hidden)]` for a scope or an endpoint ([message 14](../reference/messages.md)).

**The type of an input is `Clone + Send + Sync + 'static`**, and it cannot borrow: a call owns its inputs, and can be cloned, sent again and moved to another task ([contracts](../reference/contracts.md) C1.8). So the struct of a body derives `Clone` too: `#[derive(Clone, Serialize)] struct NewTask { … }`. Without it, `rustc` says ``the trait bound `NewTask: Clone` is not satisfied`` ([troubleshooting](troubleshooting.md#it-does-not-compile)).

**An optional input is marked on its name: `count?: u32`.** It is then not an argument: it is a method with its own name, on the call (or on the client or the handle, for an input of a client or a scope). The type is the type of the value when it is given, and in the block the name is a `&Option<u32>`. An alias changes nothing, since the mark says it and not the type. An optional input cannot fill a hole of a `path` or of a `base`.

**The mark and `Option` together have three states.** `milestone?: Option<u64>` is not set, or set to nothing, or set to a number. A JSON body of entries ([the block](#the-block)) writes the three as they are: the key left out, `null`, the number. In a query, a header or a form, the first two both send nothing. Without the mark, `x: Option<T>` is a required argument that may be `None`. What each setter takes is in [arguments](calling.md#arguments).

There is no written default. A filter that the provider calls optional is marked with `?`, also when its documentation names a default: that default is the provider's, and it can change. When nothing is given, nothing is sent.

**The name of an input** may be a word of the language: GitHub has `scope contents(path: String) { path "contents/{*path}"; }`, because those words are words only at the start of a statement. It may not be a Rust keyword: GitHub's `type` is `kind?: RepoType` in the signature, and the entry writes the key, `query { "type" = kind; }`. And an input hides a Rust item of the same name in the expressions of its node, as a variable does in Rust: with an input named `form`, the function is written with its path, `body concord::form(form);`.

## The block

**Placement: where something goes.**

| Statement | Says | Position |
|---|---|---|
| `base "https://{route}.api.example.com";` | Where the URL starts: the scheme, the host, and maybe the first part of the path (`"https://api.example.com/v1"`). An endpoint with no `base` in reach does not compile. | any |
| `path "a/{x}";` | The piece of path that this node adds. It may end with `/`: the URL ends with `/` when its last piece does. | any |
| `query { … }` | Query pairs for this node and everything below. | any |
| `headers { … }` | Headers for this node and everything below. | any |

**Behaviour: what to do. Each takes a Rust expression.**

| Statement | Says | Position |
|---|---|---|
| `body json(task);` | The request body: what the expression gives ([a body that is sent](responses-and-errors.md#a-body-that-is-sent)). | endpoint |
| `body json { chat_id; text; }` | The request body, made of entries: one key for each (below). | endpoint |
| `read Json;` | How the body of an answer is read ([reading a success](responses-and-errors.md#reading-a-success)). Written once on the client; an endpoint that differs says `read Text;`. | any |
| `paginate by_offset(start);` | This endpoint has pages ([pages](pages.md)). | endpoint |
| `status { 404 => None, … }` | What a status means here ([when a status means something](responses-and-errors.md#when-a-status-means-something)). | any |
| `middleware Signed::new(key.clone());` | What every request goes through as it leaves: a signature, a token that is renewed ([a middleware](#a-middleware-what-every-request-goes-through)). | client |

**Three facts.**

| Statement | Says | Position |
|---|---|---|
| `problem RiotProblem;` | The type of the API's own error body. It is read by the client's reader; `problem S3Problem read Xml;` names another one. | client |
| `idempotent;` | Sending this request twice changes nothing more. With nothing written, the method decides: a `GET`, a `HEAD`, an `OPTIONS`, a `PUT` and a `DELETE` are so already. | endpoint |
| `idempotent false;` | This `PUT` or `DELETE` must not be sent twice: GitHub's `PUT` that merges a pull request answers `405` the second time. What follows the word is any Rust expression that gives a `bool`, with the inputs as variables: `idempotent key.is_some();`. | endpoint |
| `idle_timeout Duration::from_secs(60);` | How long the calls below may be silent: a long poll, a stream that is quiet for minutes. The nearest one is used. | any |

**An entry** in `query { }` or `headers { }`:

| Written | Meaning |
|---|---|
| `count;` | The key is `count`, the value is the input `count`. |
| `"startTime" = start_time;` | The key as written, the value is an input. |
| `"v" = "2";` | Fixed text. |
| `"x-api-version" = "v{version}";` | A template. |
| `"labels" = labels.join(",");` | Any Rust expression: here a list, sent as one value. |
| `"labels" = labels.as_ref().map(\|l\| l.join(","));` | The same for `labels?: Vec<String>`: a comma list that sends nothing when it was not set. |
| `"x-api-version" = version.as_deref().unwrap_or("2026-01");` | A default that the description owns, for `version?: String`. |
| `override "k" = …;` | Replace an entry set above. |
| `remove "k";` | Stop sending an entry set above. |

An entry whose value is `None` writes nothing: in `query`, in `headers`, and in a body of entries (below). No `if` is needed.

Two things about an entry are checked, because a wrong guess would be silent ([rules](../reference/rules.md)):

- **In `headers`, a short entry whose name has `_` is refused.** `headers { user_agent; }` would send a header named `user_agent`: write `"user-agent" = user_agent;`. In `query` the short entry is as before. (`host` cannot be a key of `headers` either: the host is the `base`.)
- **A secret does not follow a change of `base`.** Under another `base` further down, a node writes an entry that holds a `Secret` again, or removes it, as `upload_asset` does below ([a secret stays with its base](credentials.md#a-secret-stays-with-its-base)).

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

**A body made of entries.** When the body of a request is its inputs, it is written like a `query`. Telegram's methods are all of this kind:

```rust
part formatted(parse_mode?: ParseMode, disable_notification?: bool) {
    body json { parse_mode; disable_notification; }
}

POST send_message(chat_id: ChatId, text: String, ..formatted) -> Message {
    path "sendMessage";
    body json { chat_id; text; "reply_markup" = markup_of(text); }
}
```

An entry is `name;` or `"key" = value;`, as in `query` (a template is a value here too), with no `override` and no `remove`. The name before the block is a path to a function, seen from where `api!` stands: `json`, `form`, or one of yours. It is called with the entries, and an input of the same name does not hide it. A part may hold such a body, as `formatted` does. The node that names it writes a body of entries with the same function, or no body, and the entries are put together, the part's first ([rule 25](../reference/rules.md)).

**A `None` writes nothing here too.** One level of `Option` is presence. What is inside is written as the format writes it:

| Written | What goes out |
|---|---|
| `parse_mode;` for `parse_mode?: ParseMode`, or `"k" = value;` where the value is an `Option` | left out, or the value |
| `"k" = Some(x);` with `x: Option<T>` | `null`, or the value: the way to write a `None` as `null` |

A form has no `null`: there, an inner `None` writes no pair either. How such a body is written is in the [grammar](../reference/grammar.md); the whole table, and what is silent, are in [rules](../reference/rules.md) 8 and 9.

**A `Secret` may be an entry of a body.** It is written as its text, as in a query. A token endpoint:

```rust
POST token(grant_type: String, client_id: String, client_secret: Secret) -> Token {
    path "oauth/token";
    body form { grant_type; client_id; client_secret; }
}
```

A struct that holds a `Secret` is still not written: `body json(task);` with a secret field does not compile.

**The silence that an endpoint needs.** A long poll holds a request on purpose, so its author says how long it may be silent. The expression is Rust with the inputs as variables, and gives a `std::time::Duration`:

```rust
GET get_updates(offset?: i64, timeout?: u32) -> Vec<Update> {
    path "getUpdates";
    query { offset; timeout; }
    idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
}
```

A caller's `with_idle_timeout` on the call still wins over it ([time](calling.md#time)).

**An arm** in `status { }` is written like an arm of a `match`: `404 => None`. An arm that needs the answer binds what it uses, `201 { body, headers } => …`. The fields are `body`, `headers` and `status`; naming `body` is what reads it, and `body: task` gives it another name ([when a status means something](responses-and-errors.md#when-a-status-means-something)). An endpoint with no arrow, under an arm in reach that reads a body, does not compile: it gets an arrow, or removes the arm.

`override` and `remove` also stand in front of an arm: `override 2xx => …,`, `remove 404,`. They are for what has a key, where a clash could be silent. An arm is written, overridden and removed by its key. What it takes decides when `override` is needed: a class arm from above (`4xx`) takes every status of its class, so an exact status under it is written `override 404 => None` ([rule 13](../reference/rules.md)). A statement that holds one value (`base`, `read`, `idle_timeout`) takes neither: the nearest one is used. A `path` only adds ([rule 10](../reference/rules.md)).

## A part: the same inputs on many endpoints

The tree shares downward. Some things repeat sideways: every list of Stripe has the same two inputs and the same way to page, and every `POST` takes an idempotency key. A **part** is a named group of inputs, with the statements that place them:

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

A part is written in the block of the client, after the statements and before the children. A node names it at the end of its inputs, with `..`, and then has the inputs and the statements of the part **as if it had written them**: `list` has `.limit(…)` and `.starting_after(…)`, and it has pages; `create` takes the key as its second argument, and may be sent again. Inside a part, a bare name is an input of the part, and `client.x` is an input of the client.

A part may hold a `base`, since a part exists to hold an input with the line that places it. Riot takes a route in the host of almost every scope:

```rust
part regional(route: Region) { base "https://{route}.api.riotgames.com"; }

scope matches(..regional) { path "lol/match/v5/matches"; … }
scope account(..regional) { path "riot/account/v1/accounts"; … }
```

A part holds inputs and statements, and nothing else of the tree: no `path`, no `problem`, no `middleware`, no child. It generates nothing by itself. The exact form is in the [grammar](../reference/grammar.md), the rule is [rule 25](../reference/rules.md), and the messages for a part that is wrong are 35 to 38 of [the messages](../reference/messages.md).

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
| Shared sideways | `part name(inputs) { }` |
| Templates | `"text {value}"`, `{client.value}`, `{*path}` |

Twelve statements and one declaration. That is all of it, and the [grammar](../reference/grammar.md) has it on one page.

## The small print

What the examples above do not show. [The rules](../reference/rules.md) have the lines to learn; [the contracts](../reference/contracts.md) have the exact behaviour, line by line.

**A template** is a string with `{name}` holes.

| Written | Meaning |
|---|---|
| `{puuid}` | An input of this node. |
| `{client.tenant}`, `{matches.route}` | An input of a node above, named with its owner. A bare `{tenant}` does not reach it. |
| `{id}.json`, `v{version}` | A hole may fill a piece of a path segment. Its value can never add a segment. |
| `{*key}` | A value that is itself a path (`src/lib.rs`): its `/` are kept, and each piece is encoded. A piece may be empty, so an object key like `folder/` or `a//b` is sent as it is. It stands in a path only. |
| `{{` and `}}` | A literal brace. |

**The text of a value.** A value is written by `serde`, in a body and also in a path, a query and a header. So a type has one wire form: an enum with `#[serde(rename_all = "lowercase")]` is `europe` in a host, in a query and in a JSON body. In a query or in headers, a `None` is not sent, and a list sends its key once for each item. In a path or a host, a value is one text. Anything else, a struct or a map, is refused when the request is built ([rules](../reference/rules.md) 6 and 8).

A type whose wire text is not what `serde` writes by default is a type of yours with its own `Serialize`. A date is the usual case: one API wants RFC 3339, another wants seconds.

**In a behaviour, an input is a variable that borrows the input.** In `body json(task);` the name `task` is a `&NewTask`. `"authorization" = basic(client.key, "");` uses an input from above by its owner, with the same spelling as in a template: `client.key` is a reference too, like a bare name. A body made of several inputs is a body of entries ([the block](#the-block)). When its shape is not flat, it is still written as the JSON it is, `body json(json!({ "filter": { "query": query } }));`, and there a value follows its type: an optional input that was not set is a `None`, and `json!` writes it as `null`. (`paginate` is the one place where the name of an input stands for its place on the call: see [the shipped pagers](pages.md#the-shipped-pagers).)

**The macro copies the expression as written**, and the compiler checks it against a trait of the core. So a reader, a writer or a pager of your own takes the place of a shipped one. When each expression runs, and what a `?` in it does: [when an expression runs](../reference/grammar.md#when-an-expression-runs).

**What is refused.** A description that is wrong does not compile, and the message says what to write: [the messages](../reference/messages.md) lists every one. A value that cannot be placed is refused when the request is built, and nothing is sent. Two refusals are about the name of an input:

| Refused | Write instead |
|---|---|
| A child or an optional input named like a method that its receiver already has (on a call: `send`, `request`, `pages`, `read_response`, …), or starting with `with_`: the table is [contracts](../reference/contracts.md) C1.6. A required input is an argument, so `create(request: ChatRequest)` is fine; so is a provider's own `timeout`. | Another name. The name on the wire stays: `"send" = send_now;`. |
| An input named like an owner in reach: `client`, or a scope above | Another name. |

**Less obvious behaviours:**

| | |
|---|---|
| A hole in the host | It is one label of it, or a piece of one (`"https://api-{label}.example"`): `{route}` can be `europe`, never `evil.example/x`. |
| Two hosts in one API | A `base` on the scope, or on the one endpoint, that lives on the other host: GitHub's `upload_asset` in [the block](#the-block). The nearest `base` starts the URL, and every `path` above still follows it. A secret from above does not. |
| A server that the user hosts | `base` can be one input of the type `concord::Base` instead of text: `client Gitlab(url: Base, token: Secret) { base url; path "api/v4"; … }`. A `Base` is a scheme, a host, maybe a port, and maybe a path prefix: `"https://example.com/gitlab".parse()?`. |
| A template that reads a `None` is not sent at all | `"x-range" = "{from}-{to}";` sends nothing if `from` is `None`. |
| `-> Option<T>` | A 2xx is read as a `T`, and given as `Some`. So a `200` whose body is `null` is not a silent `None`, and `Option` does not mean "a `404` gives `None`": an arm says that. |
| `-> Result<T, E>` | A 2xx is read as a `T`, and given as `Ok`. An arm gives each refusal that the endpoint owns: `409 { body } => Err(body)`. |
| `Option` and `Result` after the arrow | They are found by a closed list of spellings ([contracts](../reference/contracts.md) C1.7). An alias of one is read as it is, like any type. |
| An endpoint with nothing to add | `GET list() -> Vec<Task>;` |
| Another method | `METHOD "PURGE" purge(key: String) { path "cache/{key}"; }` |

## Choosing the tree

Two authors could write two trees for one API. Four pieces of advice make them write the same one. None is checked: an author can have a reason to break one.

| # | Advice | Why |
|---|---|---|
| 1 | **The tree follows the URL.** A scope is a piece of the provider's URL: a path prefix, a host, or both. | The URL is the one structure that the provider already chose. |
| 2 | **A scope adds something that is seen:** an input, a `path`, or a `base`. | A scope that only shares a header adds a level to every call, for nothing the caller can see. |
| 3 | **A scope is worth it when it holds an input, or two endpoints or more.** One endpoint under a fixed prefix writes the prefix in its own `path`. | No level for one call. |
| 4 | **Names are the provider's words:** the scope is the name of the resource in the provider's documentation, the endpoint is what the documentation calls the operation. | Nobody has to guess. |

Two tradeoffs, with their ways out, are in [scope and limits](../design/scope.md): an input is written twice; and a suffix after a resource (`Messages.json`, `Messages/{id}.json`) is written in each endpoint, because a `path` adds whole segments.

## A middleware: what every request goes through

Some facts of an API are not a value to place. They are a process that runs on each request as it leaves: a signature, a token that is renewed, a quota that all calls share. The description names it, on the client:

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
        /* sign the request, then */ next.send(request, info).await     // on to what really sends
    }
}
```

```rust
let vault = Vault::new(key);                                   // it signs: there is no other way to make one
let test  = Vault::new("k").with_transport(concord::fake(…));  // the fake gets the signed request
```

`middleware` stands on the client only, and may be written several times. A request goes first through the middlewares that a caller added, then through the ones of the description in the order written, then to the transport. So the author's are the last before the wire, and the author writes the one that signs last: nothing changes a request after it was signed. The expression sees the required inputs of the client, and nothing else: an optional one is refused. The value is made once, in `new`, so it can hold what all calls share; and it runs for every try and for every page. Nothing below takes it away.

The form is in the [grammar](../reference/grammar.md), and what a user cannot guess is [rule 26](../reference/rules.md). A whole middleware is written out in [a middleware](credentials.md#a-middleware); how a caller adds one of their own is in [a middleware](extending.md#a-middleware).

Eight real APIs are written in this language: [the examples](../examples/README.md).
