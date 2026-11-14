# Credentials and middlewares

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Read answers and errors, send bodies](responses-and-errors.md) · next: [Retries](retries.md)*

For the author and the user of a client: how a client says who you are. A key or a token is a value, placed like any value; a signature or a token that is renewed is a middleware.

The split is the same as for [retries](retries.md) and [pages](pages.md): the description says the fact of the API (where the credential goes, what signs each request), a Rust value does the work (a `Secret`, a middleware), and the application gives the credential itself, as an input.

## A credential is a value

**A credential that is a value is placed like any value.** A key, a bearer token, a user and a password: each is an input, and the description says where it goes, in the words it already has.

| The API wants | In the description |
|---|---|
| A key in a header | `headers { "x-riot-token" = key; }` |
| A bearer token | `headers { "authorization" = "Bearer {token}"; }` |
| HTTP Basic | `headers { "authorization" = basic(user, password); }` |
| A key in the query | `query { "api_key" = key; }` |
| A token in the address itself (Telegram: `https://api.telegram.org/bot<token>/`) | `base "https://api.telegram.org/bot{token}";`: a `Secret` in a hole of the `base` ([Telegram](../examples/telegram.md)). |
| A secret in the body (a token endpoint) | `body form { grant_type; client_id; client_secret; }`: a `Secret` may be an entry of a body, and is written as its text ([a body that is sent](responses-and-errors.md#a-body-that-is-sent)). |
| A client certificate | Nothing. It is a fact of the connection: set it on the HTTP client, `concord::Http::with(…)` ([the client](calling.md#the-client)). |
| A signature, a token that is renewed | `middleware Signed::new(key.clone());` ([a middleware](#a-middleware)). |

**What makes it a credential is its type.** The input is a `concord::Secret`, and a value that holds a `Secret` is secret wherever it is placed:

| | |
|---|---|
| In a header | The header is marked sensitive: `reqwest` and `http` do not print its value. |
| In a query, a path or a `base` | It is then part of the URL. Concord never prints a URL. A `reqwest::Response` that you take with `send_raw()` shows its URL in `{:?}`. |
| To a next URL | A pair of the query whose value is a `Secret` goes with the call to a next URL too (`by_link()`, `by_next_url(…)`): a server does not write your key in its links. |
| In an error, a log, `{:?}` | Never. A `Secret` prints as `Secret(…)` and has no `Display`. |
| In a template | `"Bearer {token}"` with a secret in a hole is a secret as a whole. |
| Under another `base` | It does not follow: [a secret stays with its base](#a-secret-stays-with-its-base). |

**The credential is an input.** When every call needs it, it is an input of the client: `new` asks for it. When one endpoint needs another one (the player's own token in `me`, in [a whole description](describing-an-api.md#a-whole-description)), it is an input of that endpoint, which writes its own entry and removes the one above: `headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }`.

| | |
|---|---|
| A key that changes | A `Secret` can be replaced: `key.replace("the new key")`. Every client and handle that holds a clone of it sends the new one in each request that is built from then on. A request is built once for a call, so a try that waited sends the key that the call started with. |
| Basic | `basic(user, password)` gives a `Secret`: `Basic` and the pair in base64 (RFC 7617), written as UTF-8. |
| A value that cannot be a header | A line break in a token: a Build error, and nothing is sent. |
| `request()` | It shows the request with these values in it: they are part of what the description builds. |

There is no `auth` statement and no list of schemes: a scheme would do nothing more than place a value, and `headers` and `query` already do that.

## A secret stays with its base

An entry of `headers` or `query`, or a piece of a `path`, whose value holds a `Secret` is bound to the `base` in reach where it is written: the `base` of its node, or the nearest one above. Where no `base` is in reach (a key on the client, above every `base`), it is bound to the first `base` below, on each branch: such a key follows to all of them. A secret in a hole of a `base` itself is part of that `base`: a `base` written further down replaces it whole, so the secret goes there only if that `base` writes it again (Telegram's second `base`, `"https://api.telegram.org/file/bot{client.token}"`). This follows from how a `base` is inherited: the nearest one is used ([C1.13](../reference/contracts.md#c1-the-description)).

Under any other `base` further down (an upload host, a second API), the description does not compile until that node writes the entry again or removes it:

- `override "authorization" = "Bearer {client.token}";` sends the key to the new host too, because you wrote it there;
- `remove "authorization";` sends nothing there.

GitHub's `upload_asset`, in [the block](describing-an-api.md#the-block), is written this way. A `path` has no `override` and no `remove`: a `path` that holds a secret is written under each `base` that needs it, not above them.

The refusal is a message of `rustc`, in Concord's words: `a secret does not follow a change of base`, with the note `write this entry again with override, or remove it, under the new base` (for a `path`: `a secret in a path does not follow a change of base`). See [troubleshooting](troubleshooting.md#it-does-not-compile).

A caller's `with_base(…)` moves everything, secrets too: it is the caller's own word. It also replaces the whole `base` ([C2.10](../reference/contracts.md#c2-building-a-request)), with a secret that was written in it. The rule is [rule 16](../reference/rules.md); the contract is C8.9 of [the contracts](../reference/contracts.md).

## A middleware

**A signature, a token that is renewed, a quota: a middleware.** They are not a value: they are a process that runs on each request as it leaves. The description names it on the client, with `middleware`, and the author writes it beside the macro ([rules](../reference/rules.md), rule 26).

```rust
pub client Vault(key: Secret) {
    base "https://api.vault.example";
    read Json;
    middleware Signed::new(key.clone());        // every request is signed as it leaves

    GET balance() -> Balance { path "balance"; }
}

// plain Rust, beside the macro
pub struct Signed { key: Secret }

impl concord::Middleware for Signed {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let text = format!("{} {}", request.method(), request.url().path());
        let signature = sign(&self.key, &text).map_err(|e| TransportError::blocked().with_source(e))?;
        request.headers_mut().insert("x-signature", signature);
        next.send(request, info).await          // on to the next middleware, then to what really sends
    }
}
```

```rust
let vault = Vault::new(key);                                   // it signs: there is no other way to make one
let test  = Vault::new("k").with_transport(concord::fake(…));  // the fake gets the signed request
```

| | |
|---|---|
| Where it stands | On the client, any number of times. |
| **The order** | A request goes first through the middlewares that a caller added, then through the ones of the description, in the order written, then to the transport. The answer comes back the same way. So the author's are the last before the wire, and **the author writes the one that signs last**: nothing changes a request after it was signed. A transport must not change it either. |
| When it is made | Once, in `new`. Every handle and every call of that client shares it, so it can hold what must be shared: a quota, a token that is renewed. Its expression sees the required inputs of the client, as references, and nothing else: the macro refuses an optional input that is named there. `new` cannot fail, so the expression has no `?`: what can fail is done in `handle`. |
| What a caller wants to read from it | A quota that the middleware keeps comes in as a required input of the client, and the caller keeps a clone. |
| When it runs | For **each** try and each page, on the request as it leaves: a request that is sent again is signed again. |
| What it gets | The request, a `TryInfo`, and `next`. `info.number()` is the number of the try. `info.endpoint()` is the name of the endpoint, and it is a promise: the names of the tree joined with `.` (`riot.matches.ids`), the same text as in a printed error. `next.send(request, info)` sends on. It may be called more than once, or not at all. |
| A middleware that waits | It is not a silent server: the clock for silence watches the transport and a body while it is read, not the middlewares ([time](calling.md#time)). A quota that holds a request back, or a token that is fetched, does not meet it. Only `timeout` bounds a middleware. With no `timeout` it may wait for ever: that is its author's affair. |
| The text of a secret | `self.key.expose()` gives the text as it is now, as a value that reads as `&str` (`&*text`, `text.to_string()`). Read it in `handle`, at each try, so a `replace` reaches the next try. Do not print it or put it in an error: the `&str` is plain text (the value itself prints only `Exposed(..)`). |
| When it cannot sign | It gives `TransportError::blocked()`, with its cause as the source, and does not call `next`. The tries end there, and the result is that blocked error. Your cause is in `e.source()`, and is never printed. |
| Nothing takes it away | Not a node below, not a caller. An API whose public endpoints must not be signed has a middleware that looks at the request, or at `info.endpoint()`. |
| What it changes | Its author's affair. The checks of pages (the origin, "the same request") and of retry (the method) look at the request as it was built, before any middleware. A middleware that sends a request elsewhere is like `.edit_request(…)`: allowed, and its author's word. |
| A caller's own | `with_middleware(…)` on a client: a log, a limit of their own ([a middleware](extending.md#a-middleware), with code). It comes before every middleware of the description, so it cannot change a request after it was signed. |
| `request()` | `request()` and `into_request()` give the request before any middleware: a signature belongs to one try. |

**A token that your code renews** is the same shape, and it calls `next.send` twice. Concord has no rule of its own for a `401`.

```rust
impl concord::Middleware for Renewed {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let again = request.try_clone();                      // nothing, for a body that is a stream
        self.write_token(&mut request);
        let answer = next.send(request, info.clone()).await?;
        match again {
            Some(mut copy) if answer.status() == 401 => {
                self.renew().await?;                          // yours: ask for a new token, and keep it
                self.write_token(&mut copy);
                next.send(copy, info).await
            }
            _ => Ok(answer),
        }
    }
}
```

Written before the signer (`middleware Renewed::new(…); middleware Signed::new(…);`), it is around it: the second send is signed too. The one that signs is written last.

## A middleware and a transport

Two things, with one job each.

| | Its job | How it is set |
|---|---|---|
| A middleware | What a request goes through on the way: a signature, a renewed token, a quota, a log. | `middleware m;` in the description; `with_middleware(…)` by a caller. |
| A transport | What really sends: `concord::Http`, a `reqwest::Client` of yours, a fake. What it does is counted by the clocks ([time](calling.md#time)). | `with_transport(…)` replaces it, **under** the middlewares. |

So a client with a setting of yours on its HTTP client (a client certificate, a proxy) is still signed: `with_transport(concord::Http::with(…)?)` changes only what is under the middlewares ([the client](calling.md#the-client)). A `reqwest::Client` that you bring is a transport too, and keeps its own settings. One that follows redirects sends the second request itself, below the middlewares: a signature that covers the path is no longer right. What `concord::Http` sets: [Concord's own HTTP client](../reference/crates.md#concords-own-http-client).

## What stays outside

| Outside | Why | The door |
|---|---|---|
| A signer; a token that is renewed; the flows that get a token (OAuth2 and the like) | A process on each request as it leaves, with state of its own; and requests to another API. Concord has the place for it, and ships none. | A middleware ([above](#a-middleware)): the author's, in the description. It sees every try and every answer. |

Why these are outside, and what else is: [scope and limits](../design/scope.md).
