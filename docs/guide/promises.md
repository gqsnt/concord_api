# What Concord promises

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Testing a client](testing.md) · next: [When Concord does not cover what you need](extending.md)*

This page is for anyone who needs to trust a client. It lists what Concord promises because it builds every request from the description, where each promise stops, and which contracts back each line.

> Safe by default, and every default can be changed explicitly: in the description, or where the client is built. A promise says what never happens silently.

The "Held by" column names the contracts that back each line, in [the contracts](../reference/contracts.md). [The engine page](../internals/engine.md) explains how the engine upholds them.

## Three promises

In a hand-written client, these must be kept true in every function. Here one implementation keeps them for every endpoint.

| # | Promise | Held by |
|---|---|---|
| G1 | **Concord never prints a value.** Its errors, its logs and its `Debug` output show names: the endpoint, the status, the kind, the attempt. Never a URL, a header value, a body, or the API's own error. Never a message that came from your own code either: the cause returned by your own middleware or transport, or a decoder's message. (The line for a body that did not decode names a location, never a value.) The cause of a network failure is the HTTP library's own error with its URL removed. An error's `source()` never holds a value from the response: Concord builds it from the location and the kind of failure, and no reader, shipped or your own, can supply a cause for it. The decoder's own message, which can quote the body, is available through `details()`. The generated documentation never prints the text of a `middleware` expression, because it could contain a key. Every `Debug` implementation of Concord's own types prints names and no values: `Headers` prints the names of its headers, and `Answer` its status and those names. A `Secret` also hides itself from `Debug`, and marks its header as sensitive for the HTTP library. | C6.2 and C6.3 (what an error prints), C5.11 (a reader passes Concord no cause), C10.6 (the log), C10.7 (`Debug`), C2.7 (the sensitive flag), C1.19 (the text of a `middleware` is never printed). |
| G2 | **A value stays where it is placed.** A value supplied by the caller cannot change the structure of the request around it. A plain placeholder stays inside one path segment. A `{*key}` placeholder is a sequence of segments and nothing else: a segment in it may be empty (`folder/`, `a//b`), and none may be `.` or `..`. A placeholder in the host is one label, or part of one; the finished label is checked, and the host stays the host that was written. | C2.3 (a value in a path, and `{*key}`), C2.1 (a value in the host), C1.15 (the fixed text of a path). |
| G3 | **Nothing that the description declares goes to another origin, and a secret does not follow a change of `base`.** Concord's own client follows no redirect: a `3xx` is the response, with its `Location`. Every page of one `.pages()` goes to the origin of its first request, whatever the pager did: another origin is a Build error. `at_url` sets a path and a query, never an origin, in pages and outside them. And a secret written for one `base` does not reach another `base` further down unless that node writes it again: [a secret stays with its base](credentials.md#a-secret-stays-with-its-base). A caller's `with_base` moves everything: that is the caller's explicit choice. | C7.1 and C7.5 (no redirect), C8.4 (every page to the origin of the first), C8.6 (`at_url`), C8.9 (a secret and a `base`), C2.10 (`with_base`). |

## Safe defaults

Any careful client has these. Here they are on by default, and each has an escape hatch.

| # | Default | Escape hatch | Held by |
|---|---|---|---|
| G4 | **The body of an error response is never read as the success type.** A value of the return type comes only from the simple rule for a 2xx, or from an arm that the author wrote. | An arm. | C5.1, C5.2 |
| G5 | **A body read whole stops at 10 MiB**, counted as it arrives. Everything an error keeps from a response is bounded: 64 KiB of the body, in a buffer of its own, and 1 KiB of a reader's own message. A body read as a stream has no limit. | `read Json.limit(…);` | C5.4 (the limit, counted as it arrives), C5.6 (what an error keeps), C5.11 (10 MiB for `Json`, `Text`, `Binary`) |
| G6 | **Same inputs, same request.** Concord adds no hidden state, no clock and no randomness to a request, and its own client keeps no cookies by default. The shipped writers follow the same rule: the boundary of a `multipart` body whose parts are all in memory is not random (it is random only with a part that is a stream, and such a body is sent only once). This holds only as long as your own code does the same: your own middleware can compute anything. (The wait before a retry has a random part; the request does not.) | — | C2.16 (the boundary of `multipart`), C7.1 (no cookies). The line as a whole has no contract number of its own. |
| G7 | **No call waits forever on a silent server.** Concord waits at most 30 seconds without a sign of life from the transport or from a body being read (`idle_timeout`), whatever the HTTP client. It is not a total limit: a body that keeps arriving is never cut off, unless you set a call timeout, a `timeout`. See [timeouts](calling.md#timeouts) for exactly what the idle timeout watches. | `with_idle_timeout(…)`, or `idle_timeout …;` in the description for an endpoint that is idle on purpose; `with_timeout(…)` for a limit on the whole call. | C7.6 ([the engine](../internals/engine.md)). |
| G8 | **Concord retries a request only when you asked it to, and only when repeating the request has no further effect.** Retries are off until you turn them on. Then a request is retried only if it is idempotent (by its HTTP method: `GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`; or because its author wrote `idempotent;`), or if its status is one that the policy lists with `not_applied_on`; and never when its body is a stream. A stream is sent once: a second send of it (through a clone of the call, for the next page, or as part of a combined body, a `multipart` among them) is a Build error, and nothing is sent. A status proves nothing about what the server did, unless the policy is told to trust it: a `POST` that is not marked is not retried after a `429` that the policy does not list, nor after a failed connection. This covers the requests that Concord itself sends. What your own middleware resends on its own (a second send with a renewed token) is your code, and what `reqwest` resends by itself happens below the level where Concord counts attempts. | `with_retry(…)`, `idempotent;`, `idempotent false;`, `not_applied_on(status)` | C4.1 (the three conditions), C4.2 (the standard policy), C4.6 (a policy on a call), C3.2 (a stream is sent once), C2.15 (a stream already sent is a Build error) ([the engine](../internals/engine.md)). |
| G9 | **No return type: the status is the result.** A `DELETE` that got its `200` is done, and a broken body after it cannot make the call fail. There is no exception: an endpoint with no return type does not compile under an arm that would read a body. | A return type. | C5.2, C5.5 |
| G10 | **Pages never look complete when they are not.** A next page that cannot be requested is an error, not an end: another origin, a page that a pager rejects, an offset that cannot grow, a `Link` field that cannot be parsed, or a repeat of the request just sent. (A longer cycle, or a list that never ends, is not detected; see [scope and limits](../design/scope.md).) | A loop of your own. | C8.4 (another origin, the request just sent), C8.6 (the shipped pagers, a URL that does not parse) |

**A signed API is signed by construction.** Its signer is a `middleware` of the description. `new` creates it, every attempt and every page goes through it, and nothing removes it: not a node below, not a caller, not `with_transport`, which replaces only the transport ([a middleware](credentials.md#a-middleware)).

## What Concord logs

Concord logs one event, `retry`, at the debug level, when it is about to wait and then retry. The event goes through the `tracing` crate, with the target `concord`, and nothing is printed unless the application subscribes. Its name and fields are part of the contract, so a tool that reads logs can rely on them.

| Event | Fields | When |
|---|---|---|
| `retry` | `endpoint`, `try`, `status` or `class`, `wait_ms` | Before another attempt. |

Names and numbers only. Everything else is yours to log: a middleware sees every attempt and every response, and `request()` returns the request for you to print. C10.6 covers this event.

## Where the promises stop

The promises cover what **the description** builds. Four things are your own code, and what they do is your responsibility:

| Your code | What it can do |
|---|---|
| `.edit_request(…)` on a call | Change the request after it was built: another URL, another header. G2 and G3 are not checked again, and the request already holds your credentials: they go wherever you send it. A header value that it marks `sensitive` is left out of the pagination check for "the same request", as a secret's value is. |
| Your own pager | Return any next call. One promise still holds, whatever the pager did: every page goes to the origin of the first request, and `call.at_url(…)` cannot set another origin. |
| Your own middleware or transport, your own `reqwest::Client` | Anything: each one has the request. A caller's middleware runs before the description's, so it cannot change a request after it was signed; a transport can, and must not. What a middleware changes is its author's responsibility: the pagination checks (the origin, "the same request") and the retry check (the method) look at the request as it was built, before any middleware. A client that follows redirects is outside G3: its second request went through no middleware. |
| A response that you obtained yourself | `call.read_response(response)` reads it, with the arms and the reader of the endpoint. Concord sent nothing: no retry, no timeout. |

Your own middleware or reader that waits does not count as silence for `idle_timeout`; your own transport that takes its time does. Only the call timeout, `timeout`, bounds the first two ([timeouts](calling.md#timeouts)).

**Dropping a call.** Concord promises that it stops, and starts no further attempt. What the HTTP library does with the connection is up to the library: the request may already have reached the server.

And three things are outside Concord's control:

- Your own code can print a key that you hold. `e.details()` and `request()` return values on purpose.
- The HTTP library below has its own debug logs: a `Secret` header is marked sensitive for them, but a value in a URL is not. A secret in a query or in a path is visible to anything on the way that logs URLs: a proxy, the server.
- A proxy between you and the server may decode an encoded `/` (`%2F`) as a real one. For Concord, it stays within one segment.

## Every error message names the fix

When a rule rejects something, the message names what to write to allow it (a keyword, a method, a setting), when there is one. See [the messages](../reference/messages.md) for the exact wording and location of each message.

| Rejected | What the message names |
|---|---|
| A key set above, written again | `override` or `remove`. |
| A child or an optional input whose name is already a method of its receiver (`send`, `pages`, `with_…`) | Another name; the name on the wire can stay the same. A required input is an argument, and may have any name. |
| Path text with a `?` | `query { }`, where query values go. |
| An endpoint with no return type, under an arm that would read a body | A return type, or `remove` for that arm. |
| A secret inherited under another `base` | For an entry: `override` to write it again, or `remove`, under the new base. For a placeholder in a `path`: to write that `path` under each `base` that needs it. |
| A short entry in `headers` whose name contains `_` | The entry to write instead: `"user-agent" = user_agent;`. |
| A paginated call that is awaited directly | The message is `rustc`'s own. The `#[must_use]` text of the call says what to write: `.send()` for one page, `.pages()` for all. |
| A path value that is `..` | The input that supplied it. Nothing is sent. |
| A body over the size limit | That the limit belongs to the reader: `.limit(…)` changes it. |
| No response in time | The timeout that expired: `with_idle_timeout` or `with_timeout` changes it. |
| A next page somewhere else | That the URL is at a different origin from the call. |
