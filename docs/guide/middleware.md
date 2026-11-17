# Middleware

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Credentials](credentials.md) · next: [Retries](retries.md)*

This page shows what every request passes through as it is sent: the rules that every middleware layer follows, the order of the layers, what the layers that Concord ships have in common, and how a middleware differs from a transport.

## How a middleware layer works

**A signature, a token that must be renewed, a quota: each is a middleware layer.** Each runs on every request as it is sent. The description names its layers on the client with `middleware` ([rules](../reference/rules.md), rule 26). Concord ships four: `auth::Renewed` for [a token that is renewed](credentials.md#a-token-that-is-renewed), `auth::Rotating` for [a refresh token that rotates](credentials.md#a-refresh-token-that-rotates), `sign::SigV4` for [a signature](credentials.md#a-signature), and `limit::Limit` for [rate limits](limits.md).

| | |
|---|---|
| Where it goes | On the client, any number of times, in an order that matters ([the order of layers](#the-order-of-layers)). |
| When it is created | Once, in `new`. Every handle and every call of that client shares it, and so does every clone of the client, so it can hold shared state: a quota, a token that is renewed. Its expression can use the required inputs of the client, as references, and nothing else: the macro rejects an optional input named there. `new` cannot fail, so the expression has no `?`: whatever can fail is done in `handle`. |
| Reading its state from the caller | A quota that the middleware tracks is passed in as a required input of the client, and the caller keeps a clone. |
| When it runs | On **every** attempt and every page, on the request as it is sent: a request that is retried is signed again. |
| What it receives | The request, a `TryInfo`, and `next`. `info.number()` is the attempt number. `info.endpoint()` is the name of the endpoint, and its format is guaranteed: the names in the tree joined with `.` (`riot.matches.ids`), the same text as in a printed error. `next.send(request, info)` passes the request on. It may be called more than once, or not at all. |
| A middleware that waits | Its wait does not count as silence ([a layer that waits](#a-layer-that-waits)). |
| The text of a secret | `self.key.expose()` returns the current text, as a value that dereferences to `&str` (`&*text`, `text.to_string()`). Read it in `handle`, on each attempt, so that a `replace` reaches the next attempt. Do not print it or put it in an error: the `&str` is plain text (the value itself prints only `Exposed(..)`). |
| When it cannot sign | It returns `TransportError::blocked()`, with its cause as the source, and does not call `next`. No more attempts are made, and the result is that blocked error. Your cause is in `e.source()`, and is never printed. |
| Nothing removes it | Neither a node below nor a caller can remove it. An API whose public endpoints must not be signed needs a middleware that inspects the request, or `info.endpoint()`. |
| What it changes | That is up to its author. The pagination checks (the origin, "the same request") and the retry check (the method) look at the request as it was built, before any middleware. A middleware that sends a request elsewhere is like `.edit_request(…)`: allowed, and its author's responsibility. |
| A caller's own middleware | `with_middleware(…)` on a client: a log, a `concord::limit::Limit` ([rate limits](limits.md)), a layer of the caller's own ([a middleware](extending.md#a-middleware), with code). It runs before every middleware layer of the description ([the order of layers](#the-order-of-layers)). |
| `request()` | `request()` and `into_request()` return the request before any middleware, because a signature belongs to one attempt. |

## The order of layers

A request goes first through the middleware that a caller added with `with_middleware`, in the order of the calls, then through the description's middleware layers, in the order written, then to the transport. The response comes back the same way.

So the author's layers are the last before the wire, and **the author writes the signing middleware last**: nothing changes a request after it was signed. A caller's layer runs before every layer of the description, so it cannot change a request after it was signed either: a `Renewed` added by a caller places its token, and a signer written in the description then signs the request with that token in place. A transport must not change a signed request.

## A layer that waits

A middleware layer may wait before it passes a request on: for a token that is being renewed, for a rate limit, for a lock of its own. Its wait does not count as silence: the idle timeout watches the transport and a body while it is read, not middleware ([timeouts](calling.md#timeouts)). Only the call timeout, `timeout`, bounds a middleware. Without a `timeout`, a middleware may wait forever: that is its author's responsibility. Each layer that Concord ships bounds its own wait (below).

A `tower` layer used as a transport is different: it waits inside the transport, so the idle timeout counts its wait as silence ([a transport](extending.md#a-transport)).

## What the shipped layers have in common

`Renewed`, `Rotating`, `SigV4` and `Limit` follow the same rules, so a client can combine them.

| | |
|---|---|
| They never send a request twice | Each one passes a request on at most once. After a `401`, `Renewed` and `Rotating` return the response as it is; the rule `after_401` asks the engine for another attempt, under the rules of every retry ([what holds for every retry](retries.md#what-holds-for-every-retry)). |
| Their waits have an end | A call waits for a token at most `renewal_timeout`, 30 seconds by default, with `Renewed` and with `Rotating`. It waits for a `Limit` at most `max_hold`, 60 seconds by default. `SigV4` waits for nothing. The call timeout, `timeout`, bounds them all ([a layer that waits](#a-layer-that-waits)). |
| A refusal is a blocked error | When a layer refuses to send, the call is a Transport error of class blocked, and nothing is sent. Its `source()` says why: a `RenewalError`, a `RotationError`, a `SignError` or a `LimitError`. Each of these types has `find(&e)`, which returns it from the error of the call. |
| Configure it fully, then clone it | A clone of a shipped layer shares the state of the value it was cloned from: the token and the renewal of a `Renewed`, the account of a `Rotating`, the credentials and the clock of a `SigV4`, everything that a `Limit` has counted. The setting methods of `Renewed`, `Rotating` and `Limit` change that shared state, so a setting made on one clone applies to every clone; `SigV4::allow_unsigned_payload` applies only to the value that it returns. Either way, a setting made after cloning is rarely what was meant. The same holds for a `Secret`, whose `replace` reaches every clone, and for `concord::Http`, whose clones share one engine ([what a clone shares](#what-a-clone-shares)). |
| A call that is dropped | It lets go of what it held: its place under a limit, its turn in a renewal ([a call that is dropped](calling.md#a-call-that-is-dropped)). |
| They start no task | Concord starts no background task. The driver of a `Rotating` must be polled apart from the calls: in a long-lived program, the application starts a task for it and owns that task ([a refresh token that rotates](credentials.md#a-refresh-token-that-rotates)). |
| Their state lives in one process | Clones in one process share it; another process knows nothing of it. A limit or an account shared between processes needs a store of the application ([rate limits](limits.md#what-a-limit-does-not-do)). |
| They print no value | Their `Debug` output and their errors show no token, key, header value or URL. What your own functions put in their own errors is yours. |

## What a clone shares

Every setter takes the value and returns it (`fn(self, …) -> Self`), so its signature does not tell you whether a change reaches the clones. Some values share their state and their settings with every clone; the others are independent once cloned. The rustdoc of each type says the same in one sentence.

| Value | A clone shares | A setter on one clone |
|---|---|---|
| `Renewed` | the token, the renewal in progress and the settings | changes every clone (`with_token` too) |
| `Rotating` | the account, the renewal in progress and the settings | changes every clone |
| `Limit` | everything counted and learned, and the settings | changes every clone |
| `Credentials` | the set of credentials | `session_token` and `replace` change every clone |
| `SigV4` | the `Credentials` and the time anchor | `allow_unsigned_payload` changes only the value it returns |
| `AccessToken` | the value, a `Secret` | none; `value().replace(…)` changes every clone |
| `Secret` | the text | none; `replace` changes every clone |
| `Bucket` | only its functions, which cannot change | changes only the value it returns; what the bucket has spent belongs to its `Limit` |
| `Window` | nothing | changes only the value it returns |
| `Retry` | only its `when` and `unless` rules, which cannot change | changes only the value it returns |
| `TokenPair` | nothing that can change | none: a pair cannot be changed |

So configure a shared value fully, then clone it.

## Middleware or transport

They are two separate layers, each with one job.

| | Its job | How it is set |
|---|---|---|
| A middleware | What a request passes through as it is sent: a signature, a renewed token, a quota, a log. | `middleware m;` in the description; `with_middleware(…)` by a caller. |
| A transport | The layer that sends the request: `concord::Http`, your own `reqwest::Client`, a fake. The timeouts measure what it does ([timeouts](calling.md#timeouts)). | `with_transport(…)` replaces it, **beneath** the middleware. |

So a client whose HTTP client has your own settings (a client certificate, a proxy) is still signed: `with_transport(concord::Http::with(…)?)` changes only what sits beneath the middleware ([the client](calling.md#the-client)). A `reqwest::Client` that you pass in is a transport too, and keeps its own settings. If it follows redirects, it sends the second request itself beneath the middleware: the signature no longer covers that request, and `x-amz-security-token` may be forwarded to another host even when `Authorization` is removed. See [Concord's own HTTP client](../reference/crates.md#concords-own-http-client) for what `concord::Http` sets up.
