# Concord in ten minutes

*[Concord](../../README.md) · [docs](../README.md) · guide · next: [How Concord sees an API](model.md)*

This page is for a Rust developer who has never seen Concord. In ten minutes, it shows how to describe an API, call the generated client and test it: code first, one idea at a time.

## An API is a tree. Write it as one.

```rust
use concord::{json, Json};

concord::api! {
    pub client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;                                  // this API responds with JSON

        GET list(done?: bool) -> Vec<Task> {
            query { done; }
        }

        POST create(task: NewTask) -> Task {
            body json(task);
        }

        GET get(id: u64) -> Task { path "{id}"; }

        DELETE delete(id: u64) { path "{id}"; }
    }
}
```

Read only the lines that start with an HTTP method: together, they are the whole API as its caller sees it. The block below each one describes the request that goes on the wire.

```rust
let tasks = Tasks::new();

let open = tasks.list().done(false).await?;
let task = tasks.create(NewTask { title: "Write the docs".into() }).await?;
tasks.delete(task.id).await?;
```

`Task` and `NewTask` are your own plain structs. `json` and `Json` are ordinary Rust items, imported like any other. The language has no keyword for a data format, so a format you write yourself fits in the same place.

## What is shared is written once

```rust
pub client Riot(key: Secret) {
    headers { "x-riot-token" = key; }                  // the credential, on every request below
    read Json;                                         // how responses are read
    problem RiotProblem;                               // the type of this API's errors

    scope matches(route: MatchRoute) {
        base "https://{route}.api.riotgames.com";      // every request below
        path "lol/match/v5/matches";

        GET ids(puuid: Puuid, count?: u32) -> Vec<MatchId> {
            path "by-puuid/{puuid}/ids";
            query { count; }
        }

        GET get(id: MatchId) -> Match { path "{id}"; }
    }
}
```

An endpoint contains only what it adds. A statement that holds for a whole group is written once, on the group, and every endpoint you add to the group inherits it without a line of its own. An input from a level above is named with its owner: `client.key`, `{matches.route}`.

## A call reads like the API

```rust
let riot = Riot::new(key);

let ids  = riot.matches(Europe).ids(&puuid).count(5).await?;
let game = riot.matches(Europe).get(&ids[0]).await?;
```

Each level of the tree is one method call. Required inputs are arguments. An optional input is marked with `?` in the description (`count?: u32`) and becomes a setter with the same name. `.await` sends the request. An input is converted to a string the way `serde` serializes it, so `Europe` becomes `europe` in the host.

## "Maybe" is `Option`. "No such thing" is `None`.

```rust
/// `None` when there is no such player.
GET by_riot_id(game_name: String, tag_line: String) -> Option<Account> {
    path "by-riot-id/{game_name}/{tag_line}";
    status { 404 => None }
}
```

```rust
match riot.account(Europe).by_riot_id("Faker", "KR1").await? {
    Some(account) => println!("{}", account.puuid),
    None => println!("nobody by that name"),
}
```

A status that means something specific for this endpoint gets one line, written like a `match` arm. Every other status is handled as before: a `200` is still read as an `Account` and returned as `Some`.

And "several" is a `Vec`: with `tags: Vec<String>` and `query { "tag" = tags; }`, the request carries `tag=sf&tag=classic`.

## The author says how the API paginates. The caller decides whether to loop.

```rust
GET ids(puuid: Puuid, start?: u32, count?: u32) -> Vec<MatchId> {
    path "by-puuid/{puuid}/ids";
    query { start; count; }
    paginate by_offset_limit(start, count);
}
```

```rust
let mut ids = riot.matches(Europe).ids(&puuid).pages().items();
while let Some(id) = ids.try_next().await? {
    println!("{id}");
}

let first = riot.matches(Europe).ids(&puuid).count(5).send().await?;      // one page, on purpose
```

How an API paginates is a fact about the API, so the author declares it once. Because `ids` is now paginated, its call cannot be awaited directly, so nobody mistakes one page for the whole list: `.pages()` yields every page, and `.send()` fetches one. (In `paginate`, an input names its parameter on the call, not its value, and `by_offset_limit`, an ordinary function, advances that parameter from page to page. This pager stops at a page shorter than `count`, as Riot promises.)

## A `part` shares inputs between unrelated endpoints

```rust
pub client Stripe(key: Secret) {
    base "https://api.stripe.com";
    headers { "authorization" = "Bearer {key}"; }
    read Json;

    part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate by_cursor(starting_after, List::next_cursor);
    }

    scope customers {
        path "v1/customers";
        GET list(email?: String, ..page) -> List<Customer> { query { email; } }
    }

    scope invoices {
        path "v1/invoices";
        GET list(..page) -> List<Invoice>;
    }
}
```

```rust
let page = stripe.customers().list().email("a@b.example").limit(50).send().await?;
```

Some inputs repeat across endpoints that have no common parent, like Stripe's pagination inputs. A `part` is a named group of such inputs, together with the statements that place them in the request, and `..page` in a signature adds the whole part to the endpoint.

## An error keeps what the server said

```rust
match riot.matches(Europe).get(&id).await {
    Ok(game) => show(game),
    Err(e) if e.is_status(404) => println!("no such match"),
    Err(e) => {
        if let Some(problem) = e.problem() {            // a RiotProblem, typed
            eprintln!("Riot says: {}", problem.status.message);
        }
        return Err(e.into());
    }
}
```

An error keeps the status, the headers, the start of the body, and the API's own error, decoded into your type. Printing it (`{e}`) is always safe: it shows names, never a key, a URL or a body.

## An endpoint that differs says so in its own block

```rust
/// Is it there?
HEAD exists(id: DocId) -> bool {
    path "{id}";
    status { 2xx => true, 404 => false }
}

/// The new document, and where it lives.
POST create(doc: NewDoc) -> Created {
    body json(doc);
    status {
        2xx { body, headers } => Created { doc: body, at: headers.require("location")?.to_owned() },
    }
}

/// Plain text, not JSON.
GET readme(repo: String) -> String {
    path "{repo}/readme";
    read Text;
}

/// A diff: a header of its own, and the bytes as they come.
GET diff() -> Bytes {
    headers { "accept" = "application/vnd.github.diff"; }
    read Binary;
}

/// A large file, as it comes.
GET download(id: FileId) -> concord::ByteStream {
    path "files/{id}";
    read Stream;                       // `use concord::read::Stream;`
}

/// An upload, on another host.
PUT upload(name: String, file: concord::Body) {
    base "https://uploads.example.com";
    headers { override "authorization" = "Bearer {client.key}"; }    // the key goes to this host too
    path "files/{name}";
    body file;
}

/// A body made of several inputs. A `cursor` that was not set is left out.
POST search(text: String, cursor?: String) -> Page {
    path "search";
    body json { text; cursor; }
}
```

An arm binds the parts of the response it uses: `body`, `headers`, `status`. In `create`, a success without a `location` header ends the call with an error. `create` matches the whole class `2xx`, because with an arm for `201` alone, any other 2xx status would be read as a `Created`. These endpoints belong to a client that sends its key in a header, `"authorization" = "Bearer {client.key}"`. A secret does not follow a change of `base` unless you write it again, so `upload` declares the header again for its own host ([a secret stays with its base](credentials.md#a-secret-stays-with-its-base)).

## A credential is a value; a signature is middleware

You place a credential wherever the API expects it, like any other value:

```rust
headers { "x-api-key" = key; }                         // a key in a header
headers { "authorization" = "Bearer {token}"; }        // a bearer token
query   { "api_key" = key; }                           // a key in the query
```

What makes it a credential is its type. A `Secret` goes on the wire and nowhere else: it never appears in an error, a log or `{:?}` output. An endpoint that uses a different credential declares it in its own block:

```rust
headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
```

Some credentials are not a value but a process: a signature computed from each request as it is sent, or a token that your code renews. That is a middleware layer, declared on the client with `middleware`, and every request passes through it.

```rust
pub client Vault(key: Secret) {
    middleware Signed::new(key.clone());      // every request is signed as it leaves
    // base, read, endpoints …
}
```

`Signed` is the author's own type, written in plain Rust beside the description; it implements `concord::Middleware`. A client made with `Vault::new(key)` signs its requests, and there is no other way to make one.

## Retries are opt-in. A silent server cannot hold a call forever.

```rust
let riot = Riot::new(key)
    .with_retry(Retry::standard());                     // off until you turn them on

let file = files.download(&id)
    .with_timeout(Duration::from_secs(600))             // this call may take ten minutes
    .await?;
```

An API option keeps its own name (`.count(5)`); a Concord setting starts with `with_`. Concord's own methods on a call (`send`, `pages`, `request`, …) form a fixed list: no option may take one of those names or start with `with_`.

`Retry::standard()` makes at most 3 attempts, and retries after a network failure or a `408`, `429`, `502`, `503` or `504`. Even then, a request is retried only when repeating it has no further effect: a `GET`, `HEAD`, `OPTIONS`, `PUT` or `DELETE`; an endpoint whose author wrote `idempotent;`; or a status that the policy lists as "not applied" (`not_applied_on(429)`; see [retries](retries.md)). A `PUT` that must not be sent twice declares `idempotent false;`.

If the server sends nothing for 30 seconds, the idle timeout ends the call with an error. The call as a whole has no time limit until you set a call timeout ([timeouts](calling.md#timeouts)).

## A test needs no network

```rust
let riot = Riot::new("test-key").with_transport(concord::fake(|request| {
    assert_eq!(request.url().path(), "/lol/match/v5/matches/EUW1_1");
    Ok(concord::reply(200, r#"{ "id": "EUW1_1" }"#))
}));

let game = riot.matches(Europe).get(MatchId::from("EUW1_1")).await?;
```

Everything else runs for real on that response, middleware included; only the network is replaced. See [testing a client](testing.md) for a scripted transport on virtual time and a local test server.

## When you need more control

When Concord does not fit, you can go down one layer, and every layer below is plain Rust. Each option is shown, with code to copy, in [When Concord does not cover what you need](extending.md).

| You need | You write |
|---|---|
| Another way to read a response, write a body, or paginate | Your own value or function, named in the description. The shipped items (`Json`, `json`, `by_cursor`) are written the same way, in a crate of their own. |
| A signature, a token that is renewed, a rate limit | A middleware layer: the author's in the description, or the caller's with `with_middleware(…)` |
| To change one request by hand | `.edit_request(\|request\| …)` on the call |
| The raw response | `.send_raw()` |
| Your own HTTP settings for the transport (a certificate, a proxy) | `with_transport(concord::Http::with(\|b\| b.add_root_certificate(cert))?)` |
| To send a request yourself and still get the typed result | `call.request()?`, then `call.read_response(response).await?` |
| Something Concord has no keyword for | It is `reqwest` underneath: `into_request()` returns the request |

## The whole thing, in two sentences

**To use a client:** call one method for each level, set options, then `.await`.

**To write one:** declare where each endpoint is (`client`, `scope`, a method), where each value goes (`base`, `path`, `query`, `headers`, `body`), and what the response means and how the call behaves (`read`, `status`, `problem`, `paginate`, `idempotent`, `idle_timeout`). Inputs that many endpoints share go in a `part`; what every request passes through is a `middleware`.

## Where to go next

- [How Concord sees an API](model.md): the few ideas behind the language.
- [Describe an API](describing-an-api.md): every statement, with its rules.
- [Call a client](calling.md): arguments, options, the client's settings, timeouts.
- [Read responses and errors, send bodies](responses-and-errors.md).
- [Credentials and middleware](credentials.md), [retries](retries.md), [pages](pages.md).
- [Testing a client](testing.md).
- [What Concord promises](promises.md), [When Concord does not cover what you need](extending.md), and [troubleshooting](troubleshooting.md).
- Why it is this way: [why Concord](../design/why.md).
