# Concord in ten minutes

*Concord V5 design · read this first · [index](../README.md) · then: [1. Why](1-why.md)*

For someone who has never seen it. Code first, one idea at a time.

## 1. An API is a tree. Write it as one.

```rust
use concord::{json, Json};

concord::api! {
    pub client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;                                  // this API answers in JSON

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

Read only the lines that start with a method. That is the whole API, as its user sees it. The block under each line is for the wire.

```rust
let tasks = Tasks::new();

let open = tasks.list().done(false).await?;
let task = tasks.create(NewTask { title: "Write the docs".into() }).await?;
tasks.delete(task.id).await?;
```

`Task` and `NewTask` are your own plain structs. `json` and `Json` are ordinary Rust items, imported like any other: the language has no word for a format, and one of yours takes the same place.

## 2. What is shared is written once

```rust
pub client Riot(key: Secret) {
    headers { "x-riot-token" = key; }                  // who you are, on every request below
    read Json;                                         // how this API answers
    problem RiotProblem;                               // how this API says no

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

An endpoint holds only what it adds. What is true for a whole group lives on the group, and a new endpoint gets it for free. An input of a level above is named with its owner: `client.key`, `{matches.route}`.

## 3. A call reads like the API

```rust
let riot = Riot::new(key);

let ids  = riot.matches(Europe).ids(&puuid).count(5).await?;
let game = riot.matches(Europe).get(&ids[0]).await?;
```

One level of the tree is one call. What is required is an argument. What is not required is marked with `?` (`count?: u32`), and is a method with its own name. `.await` sends. An input becomes text as `serde` writes it: `Europe` is `europe` in the host.

## 4. "Maybe" is `Option`. "No such thing" is `None`.

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

A status that means something for this endpoint is one line, written like a `match`. Everything else stays as it was: a `200` is still read as an `Account`, and given as `Some`.

And "several" is `Vec`: with `tags: Vec<String>` and `query { "tag" = tags; }`, the request has `tag=sf&tag=classic`.

## 5. A list that comes in pages

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

How an API pages is a fact of the API, so its author writes it, once. The caller only decides whether to loop. Now that `ids` has pages, it is not awaited directly, so that nobody takes one page for the whole list: `.pages()` gives them all, `.send()` gives one. (In `paginate`, an input names its place on the call, and `by_offset_limit`, an ordinary function, moves it. This one ends on a page shorter than `count`, as Riot promises.)

## 6. The same inputs on many endpoints

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

The tree shares downward. A `part` shares sideways: a named group of inputs, with the lines that place them. `..page` gives an endpoint all of it.

## 7. When the API says no

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

An error keeps what the server said: the status, the headers, the start of the body, and the API's own error as your type. Printing it (`{e}`) is always safe: it shows names, never a key, a URL or a body.

## 8. Endpoints that are not like the others

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

An arm names what it uses of the answer: `body`, `headers`, `status`. A success with no `location` ends the call with an error. `create` writes its arm for the class `2xx`: an arm for `201` alone would leave another 2xx to be read as a `Created`. These endpoints stand in a client whose key is a header, `"authorization" = "Bearer {client.key}"`: a secret does not follow a change of `base` by itself, so `upload` writes the header again under its own host ([rule 16](rules.md)).

## 9. Who you are

A credential is a value, so it is placed like any other value:

```rust
headers { "x-api-key" = key; }                         // a key in a header
headers { "authorization" = "Bearer {token}"; }        // a bearer token
query   { "api_key" = key; }                           // a key in the query
```

What makes it a credential is its type. A `Secret` can go on the wire, and nowhere else: it never shows in an error, a log or a `{:?}`. An endpoint that uses another credential says so in its own block:

```rust
headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
```

One kind of credential is not a value but a process: a signature computed from the request as it leaves, a token that your code renews. That is a `middleware`, on the client: every request goes through it.

```rust
pub client Vault(key: Secret) {
    middleware Signed::new(key.clone());      // every request is signed as it leaves
    // base, read, endpoints …
}
```

`Signed` is plain Rust beside the description: a type of the author's with `concord::Middleware`. `Vault::new(key)` signs, and there is no other way to make one.

## 10. Trying again, and time

```rust
let riot = Riot::new(key)
    .with_retry(Retry::standard());                     // off until you say so

let file = files.download(&id)
    .with_timeout(Duration::from_secs(600))             // this call may take ten minutes
    .await?;
```

An option of the API keeps its own name (`.count(5)`). A setting of Concord starts with `with_`. Concord's few verbs on a call (`send`, `pages`, `request`, …) are a closed list: no option can take one of those names, or start with `with_`.

`Retry::standard()` makes 3 tries at most, after a failure of the network or a `408`, `429`, `502`, `503` or `504`. With it, a request goes out again only when repeating it changes nothing more: a `GET`, `HEAD`, `OPTIONS`, `PUT` or `DELETE`, an endpoint whose author wrote `idempotent;`, or a status that the policy lists as "not applied". A `PUT` that must not go out twice says `idempotent false;`.

A server that says nothing for 30 seconds ends the call. A call as a whole has no limit until you give it one.

## 11. A test with no network

```rust
let riot = Riot::new("test-key").with_transport(concord::fake(|request| {
    assert_eq!(request.url().path(), "/lol/match/v5/matches/EUW1_1");
    Ok(concord::reply(200, r#"{ "id": "EUW1_1" }"#))
}));

let game = riot.matches(Europe).get(MatchId::from("EUW1_1")).await?;
```

Everything runs for real on that answer, the middlewares too. Only the network is replaced.

## 12. When it does not fit

Each layer has a way down, and each way down is plain Rust:

| You need | You write |
|---|---|
| Another way to read an answer, to write a body, to page | A value or a function of yours, named in the description. The shipped ones (`Json`, `json`, `by_cursor`) are written the same way, in a crate of their own. |
| A signature, a token that is renewed, a rate limit | A middleware: the author's in the description, a caller's with `with_middleware(…)` |
| To change one request by hand | `.edit_request(\|request\| …)` on the call |
| The raw answer | `.send_raw()` |
| What really sends: your own HTTP settings (a certificate, a proxy) | `with_transport(concord::Http::with(\|b\| b.add_root_certificate(cert))?)` |
| To send a request yourself and keep the typed answer | `call.request()?`, then `call.read_response(response).await?` |
| Something Concord has no word for | It is `reqwest` underneath: `into_request()` gives you the request |

## The whole thing, in two sentences

**For who uses a client:** go down a level, set an option, `.await`.

**For who writes one:** say where it is (`client`, `scope`, a method), what goes where (`base`, `path`, `query`, `headers`, `body`), and what it means (`read`, `status`, `problem`, `paginate`, `idempotent`, `idle_timeout`). What many endpoints share is a `part`; what every request goes through is a `middleware`.

The chapters say why each of these is the way it is. Start with [1. Why](1-why.md).
