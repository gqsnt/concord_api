# Credentials and middleware

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Read responses and errors, send bodies](responses-and-errors.md) · next: [Retries](retries.md)*

This page shows how a client sends credentials. A key or a token is a value, placed like any other value; a signature, or a token that must be renewed, is middleware.

The work is split as it is for [retries](retries.md) and [pages](pages.md): the description states the fact about the API (where the credential goes, what signs each request), a Rust value does the work (a `Secret`, a middleware layer), and the application supplies the credential itself, as an input.

## A credential is a value

**A credential that is a value is placed like any other value.** A key, a bearer token, a username and a password: each is an input, and the description specifies where it goes, with the statements it already has.

| The API wants | In the description |
|---|---|
| A key in a header | `headers { "x-riot-token" = key; }` |
| A bearer token | `headers { "authorization" = "Bearer {token}"; }` |
| HTTP Basic | `headers { "authorization" = basic(user, password); }` |
| A key in the query | `query { "api_key" = key; }` |
| A token in the URL itself (Telegram: `https://api.telegram.org/bot<token>/`) | `base "https://api.telegram.org/bot{token}";`: a `Secret` in a placeholder of the `base` ([Telegram](../examples/telegram.md)). |
| A secret in the body (a token endpoint) | `body form { grant_type; client_id; client_secret; }`: a `Secret` may be an entry of a body, and is sent as its string ([a body that is sent](responses-and-errors.md#a-body-that-is-sent)). |
| A client certificate | Nothing. It belongs to the connection: set it on the HTTP client, with `concord::Http::with(…)` ([the client](calling.md#the-client)). |
| A signature, a token that must be renewed | `middleware Signed::new(key.clone());` ([a middleware](#a-middleware)). |

**Its type is what makes it a credential.** The input is a `concord::Secret`, and a value that holds a `Secret` stays secret wherever it is placed:

| | |
|---|---|
| In a header | The header is marked sensitive, so `reqwest` and `http` do not print its value. |
| In a query, a path or a `base` | It becomes part of the URL. Concord never prints a URL. A `reqwest::Response` that you get from `send_raw()` shows its URL in `{:?}`. |
| In a next-page URL | A query pair whose value is a `Secret` is also sent when the call moves to a next-page URL (`by_link()`, `by_next_url(…)`), because a server does not put your key in its links. |
| In an error, a log, `{:?}` | Never. A `Secret` prints as `Secret(…)` and has no `Display`. |
| In a template | `"Bearer {token}"` with a secret in a placeholder is secret as a whole. |
| Under another `base` | It does not follow: [a secret stays with its base](#a-secret-stays-with-its-base). |

**The credential is an input.** When every call needs it, it is an input of the client, and `new` takes it. When one endpoint needs a different one (the player's own token in `me`, in [a whole description](describing-an-api.md#a-whole-description)), it is an input of that endpoint, which writes its own entry and removes the inherited one: `headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }`.

| | |
|---|---|
| A key that changes | A `Secret` can be replaced: `key.replace("the new key")`. Every client and handle that holds a clone of it sends the new key in every request built from then on. A request is built once per call, so a retry after a wait sends the key that the call started with. |
| Basic | `basic(user, password)` returns a `Secret`: `Basic` followed by the pair in base64 (RFC 7617), encoded as UTF-8. |
| A value that cannot be a header | A line break in a token is a Build error, and nothing is sent. |
| `request()` | It shows the request with these values in it, because they are part of what the description builds. |

There is no `auth` statement and no list of authentication schemes: a scheme would do nothing more than place a value, and `headers` and `query` already do that.

## A secret stays with its base

An entry of `headers` or `query`, or a placeholder in a `path`, whose value holds a `Secret` is bound to the `base` that applies where it is written: the `base` of its own node, or the nearest one above. When no `base` applies there (a key on the client, above every `base`), it is bound to the first `base` below it, on each branch, so such a key follows to all of them. A secret in a placeholder of a `base` is part of that `base`: a `base` written further down replaces it whole, so the secret goes there only if that `base` writes it again (Telegram's second `base`, `"https://api.telegram.org/file/bot{client.token}"`). This follows from how a `base` is inherited: the nearest one applies ([C1.13](../reference/contracts.md#c1-the-description)).

Under any other `base` further down (an upload host, a second API), the description does not compile until that node writes the entry again or removes it:

- `override "authorization" = "Bearer {client.token}";` sends the key to the new host too, because you wrote it there;
- `remove "authorization";` sends nothing there.

GitHub's `upload_asset`, in [the block](describing-an-api.md#the-block), is written this way. A `path` has no `override` and no `remove`: a `path` that holds a secret is written under each `base` that needs it, not above them.

The error is a `rustc` message in Concord's words: `a secret does not follow a change of base`, with the note `write this entry again with override, or remove it, under the new base` (for a `path`: `a secret in a path does not follow a change of base`). See [troubleshooting](troubleshooting.md#it-does-not-compile).

A caller's `with_base(…)` moves everything, secrets included: that is the caller's explicit choice. It also replaces the whole `base` ([C2.10](../reference/contracts.md#c2-building-a-request)), including any secret written in it. See [rule 16](../reference/rules.md) for the rule, and C8.9 of [the contracts](../reference/contracts.md) for the contract.

## A middleware

**A signature, a token that must be renewed, a quota: each is a middleware layer.** None of them is a value: each is a process that runs on every request as it is sent. The description names it on the client, with `middleware`, and the author writes it beside the macro ([rules](../reference/rules.md), rule 26).

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
        next.send(request, info).await          // on to the next middleware layer, then to the transport
    }
}
```

```rust
let vault = Vault::new(key);                                   // it signs: there is no other way to make one
let test  = Vault::new("k").with_transport(concord::fake(…));  // the fake gets the signed request
```

| | |
|---|---|
| Where it goes | On the client, any number of times. |
| **The order** | A request goes first through the middleware that a caller added, then through the description's middleware layers, in the order written, then to the transport. The response comes back the same way. So the author's layers are the last before the wire, and **the author writes the signing middleware last**: nothing changes a request after it was signed. A transport must not change it either. |
| When it is created | Once, in `new`. Every handle and every call of that client shares it, so it can hold shared state: a quota, a token that is renewed. Its expression can use the required inputs of the client, as references, and nothing else: the macro rejects an optional input named there. `new` cannot fail, so the expression has no `?`: whatever can fail is done in `handle`. |
| Reading its state from the caller | A quota that the middleware tracks is passed in as a required input of the client, and the caller keeps a clone. |
| When it runs | On **every** attempt and every page, on the request as it is sent: a request that is retried is signed again. |
| What it receives | The request, a `TryInfo`, and `next`. `info.number()` is the attempt number. `info.endpoint()` is the name of the endpoint, and its format is guaranteed: the names in the tree joined with `.` (`riot.matches.ids`), the same text as in a printed error. `next.send(request, info)` passes the request on. It may be called more than once, or not at all. |
| A middleware that waits | Its wait does not count as silence: the idle timeout watches the transport and a body while it is read, not middleware ([timeouts](calling.md#timeouts)). A quota that holds a request back, or a token being fetched, does not trigger it. Only the call timeout, `timeout`, bounds a middleware. Without a `timeout`, a middleware may wait forever: that is its author's responsibility. |
| The text of a secret | `self.key.expose()` returns the current text, as a value that dereferences to `&str` (`&*text`, `text.to_string()`). Read it in `handle`, on each attempt, so that a `replace` reaches the next attempt. Do not print it or put it in an error: the `&str` is plain text (the value itself prints only `Exposed(..)`). |
| When it cannot sign | It returns `TransportError::blocked()`, with its cause as the source, and does not call `next`. No more attempts are made, and the result is that blocked error. Your cause is in `e.source()`, and is never printed. |
| Nothing removes it | Neither a node below nor a caller can remove it. An API whose public endpoints must not be signed needs a middleware that inspects the request, or `info.endpoint()`. |
| What it changes | That is up to its author. The pagination checks (the origin, "the same request") and the retry check (the method) look at the request as it was built, before any middleware. A middleware that sends a request elsewhere is like `.edit_request(…)`: allowed, and its author's responsibility. |
| A caller's own middleware | `with_middleware(…)` on a client: a log, a limit of the caller's own ([a middleware](extending.md#a-middleware), with code). It runs before every middleware layer of the description, so it cannot change a request after it was signed. |
| `request()` | `request()` and `into_request()` return the request before any middleware, because a signature belongs to one attempt. |

**A token that your code renews** has the same shape, and calls `next.send` twice. Concord has no built-in rule for a `401`.

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
                self.renew().await?;                          // yours: request a new token, and keep it
                self.write_token(&mut copy);
                next.send(copy, info).await
            }
            _ => Ok(answer),
        }
    }
}
```

Written before the signer (`middleware Renewed::new(…); middleware Signed::new(…);`), it wraps the signer, so the second send is signed too. The signing middleware is still written last.

## Middleware or transport

They are two separate layers, each with one job.

| | Its job | How it is set |
|---|---|---|
| A middleware | What a request passes through as it is sent: a signature, a renewed token, a quota, a log. | `middleware m;` in the description; `with_middleware(…)` by a caller. |
| A transport | The layer that sends the request: `concord::Http`, your own `reqwest::Client`, a fake. The timeouts measure what it does ([timeouts](calling.md#timeouts)). | `with_transport(…)` replaces it, **beneath** the middleware. |

So a client whose HTTP client has your own settings (a client certificate, a proxy) is still signed: `with_transport(concord::Http::with(…)?)` changes only what sits beneath the middleware ([the client](calling.md#the-client)). A `reqwest::Client` that you pass in is a transport too, and keeps its own settings. If it follows redirects, it sends the second request itself, beneath the middleware, so a signature that covers the path is no longer valid. See [Concord's own HTTP client](../reference/crates.md#concords-own-http-client) for what `concord::Http` sets up.

## What Concord does not ship

| Not shipped | Why | The escape hatch |
|---|---|---|
| A signer; a token that is renewed; the flows that obtain a token (OAuth2 and similar) | Each is a process that runs on every outgoing request, with its own state, and may send requests to another API. Concord provides the extension point, and ships none. | A middleware layer ([above](#a-middleware)), written by the author in the description. It sees every attempt and every response. |

See [scope and limits](../design/scope.md) for why these are left out, and what else is.
