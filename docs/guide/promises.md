# What Concord promises

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Testing a client](testing.md) · next: [When Concord does not say what you need](extending.md)*

For anyone who must trust a client: what Concord promises because it builds every request from the description, where each promise stops, and which contracts hold each line.

> Safe by default, and every default can be changed by something written: in the description, or where the client is built. A promise says what never happens silently.

"Held by" names the contracts that hold each line, in [the contracts](../reference/contracts.md). [The engine page](../internals/engine.md) says how the engine holds them.

## Three promises

By hand they must be kept true in each function. Here they are made once, for every endpoint.

| # | Promise | Held by |
|---|---|---|
| G1 | **Concord never prints a value.** Its errors, its logs and its `Debug` show names: the endpoint, the status, the kind, the try. Never a URL, a header value, a body, the API's own error. Never a text that came from your own code either: the cause that a middleware or a transport of yours gave, the message of a decoder. (The line for a body that did not decode names a place, never a value.) The cause of a network failure is the HTTP library's own error with its URL taken out. An error's `source()` never holds a value of the answer: Concord makes it, from the place and the kind of failure, and no reader, shipped or yours, hands it a cause. The decoder's own message, which can quote the body, is behind `details()`. The generated documentation never prints the text of a `middleware` expression: it could hold a key. Every `Debug` of Concord's own types prints names and no value: `Headers` prints the names of its headers, `Answer` its status and those names. A `Secret` also hides itself from `Debug`, and marks its header sensitive for the HTTP library. | C6.2 and C6.3 (what an error prints), C5.11 (a reader hands Concord no cause), C10.6 (the log), C10.7 (`Debug`), C2.7 (the sensitive mark), C1.19 (the text of a `middleware` is never printed). |
| G2 | **A value stays in its slot.** A value given by the caller cannot change the structure of the request around it. A plain hole is one piece of one path segment. A `{*key}` hole is a run of segments and nothing else: a piece of it may be empty (`folder/`, `a//b`), and none may be `.` or `..`. A hole in the host is one label, or a piece of one; the finished label is checked, and the host stays the host that was written. | C2.3 (a value in a path, and `{*key}`), C2.1 (a value in the host), C1.15 (the fixed text of a path). |
| G3 | **Nothing that the description declares goes to another origin, and a secret does not follow a change of `base`.** Concord's own client follows no redirect: a `3xx` is the answer, with its `Location`. Every page of one `.pages()` goes to the origin of its first request, whatever the pager did: another origin is a Build error. `at_url` gives a path and a query, never an origin, in pages and outside them. And a secret written for one `base` does not reach another `base` further down unless that node writes it again: [a secret stays with its base](credentials.md#a-secret-stays-with-its-base). A caller's `with_base` moves everything: it is the caller's own word. | C7.1 and C7.5 (no redirect), C8.4 (every page to the origin of the first), C8.6 (`at_url`), C8.9 (a secret and a `base`), C2.10 (`with_base`). |

## Safe defaults

Any careful client has them. Here they are on without being asked, and each has a way out.

| # | Default | Way out | Held by |
|---|---|---|---|
| G4 | **The body of a refusal is never read as the success type.** A value of the arrow's type comes only from the simple rule for a 2xx, or from an arm that is written. | An arm. | C5.1, C5.2 |
| G5 | **A body read whole stops at 10 MiB**, counted as it arrives. Everything that an error keeps from an answer is bounded: 64 KiB of the body, in a buffer of its own, and 1 KiB of a reader's own message. A body taken as a stream has no limit. | `read Json.limit(…);` | C5.4 (the limit, counted as it arrives), C5.6 (what an error keeps), C5.11 (10 MiB for `Json`, `Text`, `Binary`) |
| G6 | **Same inputs, same request.** Concord adds no hidden state, no clock and no randomness to a request, and its own client keeps no cookies by default. The shipped writers keep the line: the boundary of a `multipart` body whose parts are all in memory is not random (it is random only with a part that is a stream, and such a body goes out once). This holds as long as your own code does the same: a middleware of yours can compute anything. (How long retry waits has a random part. The request does not.) | — | C2.16 (the boundary of `multipart`), C7.1 (no cookies). The line as a whole has no number of its own. |
| G7 | **No call waits forever on a silent server.** Concord waits at most 30 seconds without a sign of life from the transport or from a body being read (`idle_timeout`), whatever the HTTP client. It is not a total: a body that keeps coming is never cut, unless you set a `timeout`. What the clock watches, exactly: [time](calling.md#time). | `with_idle_timeout(…)`, or `idle_timeout …;` in the description for an endpoint that is silent on purpose; `with_timeout(…)` for a limit on the whole call. | C7.6 ([the engine](../internals/engine.md)). |
| G8 | **Concord sends a request again only when you asked, and only when repeating it changes nothing more.** Retry is off until set. Then a request goes out again only if it is idempotent (by its HTTP method: `GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`; or because its author wrote `idempotent;`), or if its status is one that the policy lists with `not_applied_on`; and never when its body is a stream. A stream goes out once: a second send of it (through a clone of the call, by the next page, or as a part of a body made of bodies, a `multipart` among them) is a Build error, and nothing is sent. A status proves nothing about what the server did, unless the policy is told to trust it: a `POST` that is not marked is not sent again after a `429` that the policy does not list, nor after a connection that failed. This is about what Concord itself hands on. What a middleware of yours sends again on its own (a second send with a renewed token) is your code, and what `reqwest` sends again by itself is below what Concord counts. | `with_retry(…)`, `idempotent;`, `idempotent false;`, `not_applied_on(status)` | C4.1 (the three conditions), C4.2 (the standard policy), C4.6 (a policy on a call), C3.2 (a stream is sent once), C2.15 (a stream already sent is a Build error) ([the engine](../internals/engine.md)). |
| G9 | **No arrow: the status is the answer.** A `DELETE` that got its `200` is done, and a broken body after it cannot make the call fail. There is no exception: an endpoint with no arrow, under an arm that would read a body, does not compile. | An arrow. | C5.2, C5.5 |
| G10 | **Pages never look whole when they are not.** A next page that cannot be asked is an error, not an end: another origin, a page that a pager refuses, an offset that cannot grow, a `Link` field that cannot be read, or the request that was just sent. (A longer cycle, or a list that never ends, is not seen: [scope and limits](../design/scope.md).) | A loop of your own. | C8.4 (another origin, the request just sent), C8.6 (the shipped pagers, a URL that does not parse) |

**A signed API is signed by construction.** Its signer is a `middleware` of the description. `new` makes it, every try and every page goes through it, and nothing takes it away: not a node below, not a caller, not `with_transport`, which replaces only what really sends ([a middleware](credentials.md#a-middleware)).

## What Concord logs

One event, `retry`, at the level debug: Concord is about to wait, and then to try again. It goes through the `tracing` crate, with the target `concord`, and nothing is printed unless the application listens. Its name and its fields are a contract: a tool that reads logs can rely on them.

| Event | Fields | When |
|---|---|---|
| `retry` | `endpoint`, `try`, `status` or `class`, `wait_ms` | Before another try. |

Names and numbers only. Everything else is yours to log: a middleware sees every try and every answer, and `request()` gives the request to print. C10.6 holds this event.

## Where the promises stop

The promises are about what **the description** builds. Four things are your own code, and what they do is yours:

| Your code | What it can do |
|---|---|
| `.edit_request(…)` on a call | Change the request after it was built: another URL, another header. G2 and G3 are not checked again, and the request already holds your credentials: they go where you send it. A header value that it marks `sensitive` is left out of "the same request" of pages, as a secret's is. |
| A pager of yours | Give any next call. One promise still holds, whatever the pager did: every page goes to the origin of the first request, and `call.at_url(…)` gives no other origin. |
| A middleware or a transport of yours, a `reqwest::Client` of yours | Anything: each has the request. A caller's middleware runs before the description's, so it cannot change a request after it was signed; a transport can, and must not. What a middleware changes is its author's affair: the checks of pages (the origin, "the same request") and of retry (the method) look at the request as it was built, before any middleware. A client that follows redirects is outside G3: its second request went through no middleware. |
| An answer that you got yourself | `call.read_response(response)` reads it, with the arms and the reader of the endpoint. Concord sent nothing: no retry, no clock. |

A middleware or a reader of yours that waits is not silence for `idle_timeout`; a transport of yours that takes its time is. Only `timeout` bounds the first two ([time](calling.md#time)).

**A drop.** Concord promises that it stops, and starts no other try. What the HTTP library does with the connection is its own: the request may have reached the server.

And three things are outside Concord:

- Your own code can print a key that you hold. `e.details()` and `request()` give you values on purpose.
- The HTTP library below has its own debug logs: a `Secret` header is marked sensitive for them, a value in a URL is not. A secret in a query or in a path is visible to whatever logs URLs on the way: a proxy, the server.
- A proxy between you and the server may read an encoded `/` (`%2F`) as a real one. For Concord it is one segment.

## Every refusal names its way out

When a rule refuses something, the message names the word that allows it, when one exists. The wording and the place of each message are in [the messages](../reference/messages.md).

| Refusal | What the message names |
|---|---|
| A key set above, written again | `override` or `remove`. |
| A child or an optional input whose name is already a method of its receiver (`send`, `pages`, `with_…`) | Another name; the name on the wire can stay. A required input is an argument, and may have any name. |
| Path text with a `?` | `query { }`, where query values go. |
| An endpoint with no arrow, under an arm that would read a body | An arrow, or `remove` for that arm. |
| A secret inherited under another `base` | For an entry: `override` to write it again, or `remove`, under the new base. For a piece of a `path`: to write that `path` under each `base` that needs it. |
| A short entry in `headers` whose name has `_` | The entry to write: `"user-agent" = user_agent;`. |
| A call with pages that is awaited directly | The message is `rustc`'s own. The `#[must_use]` text of the call says what to write: `.send()` for one page, `.pages()` for all. |
| A path value that is `..` | The input that gave it. Nothing is sent. |
| A body over the limit | That the limit is the reader's: `.limit(…)` changes it. |
| No answer in time | The clock that ended: `with_idle_timeout` or `with_timeout` changes it. |
| A next page somewhere else | That the URL is at another origin than the call. |
