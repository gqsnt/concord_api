# How Concord sees an API

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Concord in ten minutes](tour.md) · next: [Describe an API](describing-an-api.md)*

This page is for the author of a description. It explains the few ideas behind the language: what an HTTP API is made of, and how each fact applies from the root of the tree down to the leaves. Read it once, and every statement of the language will have an obvious place.

## One request, one response

Concord describes one exchange: a request goes out, and a response comes back.

Some operations need several exchanges, such as a retry or fetching the next page. Each of these is the same request, sent again with a small change. So the description still describes one request, and a Rust value describes how the next one differs ([retries](retries.md), [pages](pages.md)).

## What an HTTP exchange is made of

| Part | Example | Nature |
|---|---|---|
| Method | `GET` | one word |
| Host | `europe.api.riotgames.com` | text |
| Path | `lol/match/v5/matches/{id}` | text, in segments |
| Query | `?count=5&queue=420` | text, key and value pairs |
| Headers | `x-riot-token: …` | text, name and value pairs |
| Request body | `{"title": "…"}` | **payload** |
| Status | `200` | a number |
| Response headers | `etag: "v7"` | text |
| Response body | `{"id": 7}` | **payload** |

That is the whole list.

## Bodies are payloads; everything else is text

Everything except the two bodies is text, or ends up as text.

| | Text places | Payloads |
|---|---|---|
| Where | host, path, query, headers | request body, response body |
| What a value needs | a type that `serde` can serialize as one string (or as a list of strings, in a query or in headers) | a Rust type that `serde` can serialize or deserialize |
| What Concord does | the escaping that each location requires | hands the value to the function that the `body` statement names, or the bytes to the reader |

A payload is **one value**. A body that is very large, or that never ends, is not a value: it is a stream of bytes, and the caller reads them as they arrive ([a body that is large, or that never ends](responses-and-errors.md#a-body-that-is-large-or-that-never-ends)).

So the language has two ways to refer to data: a **template** in a text place, and a **Rust type** for a payload. What they share is `serde`: it serializes both, so a type has one wire form wherever it goes.

## One kind of node, three positions

| Position | Word | Only this position can have |
|---|---|---|
| Root | `client` | the type of the API's own error; middleware; the parts ([below](#how-each-statement-applies-from-root-to-leaf)) |
| Middle | `scope` | nothing special |
| Leaf | a method: `GET`, `POST`, … | a body, a response type, pages, the `idempotent` mark |

Almost every other statement can be written at any position, so you learn each statement once.

## Placement is Concord's language; behaviour is Rust

The statements of the language are of two kinds.

| Kind | What it says | Its words | What follows the keyword |
|---|---|---|---|
| **Placement** | Where something goes on the wire. | `base`, `path`, `query { }`, `headers { }` | Text, a template, the name of an input. The value of an entry may also be a Rust expression; its place and its key are always plain text. |
| **Behaviour** | What to do: write a body, read a response, fetch the next page, give a status its meaning, sign a request as it is sent. | `body`, `read`, `paginate`, the arms of `status { }`, `middleware` | A Rust expression. Concord only needs the value to be of a known kind: a body, a value that implements one of its traits, a function. |

A body made of the inputs may also be written as a function followed by entries, like a `query`: `body json { text; cursor; }` ([the block](describing-an-api.md#the-block)). In all three kinds of entry block (`query`, `headers`, and such a body), an entry whose value is `None` is left out.

Three more statements are plain facts: `problem` (the type of the API's own error), `idempotent` (whether this request may be repeated, when its method does not already decide it) and `idle_timeout` (how long these calls may stay idle).

Placement is Concord's own small language. Behaviour is not a language at all: it is Rust.

## How each statement applies from root to leaf

| Statement | Going down the tree |
|---|---|
| `base` | The nearest one starts the URL. |
| `path` | The paths of each level are joined, root first. It is the only statement that accumulates. |
| Headers, query | Named entries. A child inherits all the entries of its parents, with one exception: an entry that holds a `Secret` does not follow a change of `base` ([a secret stays with its base](credentials.md#a-secret-stays-with-its-base)). |
| `read`, `idle_timeout` | One per request: the nearest one applies. |
| Arms of `status { }` | Entries, keyed by status. A child inherits the arms of its parents. A class arm (`4xx`) covers every status in its class, so an arm for an exact status below it is written with `override`. |
| `middleware` | Declared on the client only. Every request of the client passes through it as it is sent, and nothing below can remove it. |
| Body, pages, `idempotent`, the response type | Leaf only. |

> A key that is already set above (an entry of `query` or `headers`, or an arm) can be set again only with an explicit keyword: `override` to replace it, `remove` to drop it. For a statement that holds a single value (`base`, `read`, `idle_timeout`), the nearest one is used, and no keyword is needed.

**The tree shares settings downward; a part shares them across branches.** Inputs that recur on many endpoints with no common parent (the pagination inputs with the statement that paginates, an idempotency key with its header, a route with the `base` that it fills) are written once, as a `part` of the client. A node names the part in its signature (`..page`), and then has those inputs and statements as if it had written them itself. A part adds no level to the tree ([a part](describing-an-api.md#a-part-the-same-inputs-on-many-endpoints)).

"A tree" is a very good approximation, but not the exact truth. See [scope and limits](../design/scope.md) for where it stops holding.

## Four questions for every fact

To write down any fact about an API, the author answers four questions, and each answer has exactly one spelling.

| Question | Possible answers | How it is spelled | Written in |
|---|---|---|---|
| **Where is it true?** | The whole API, a group, one endpoint. | The position: `client`, `scope`, an endpoint. | The nesting |
| **Who decides it?** | The provider, or the caller. | The caller: an input, marked with `?` when it may be left out. The provider: fixed text. | The signature, or the block |
| **Where does it go?** | Host, path, query, header, body. | `base`, `path`, `query { }`, `headers { }`, `body`. | The block |
| **Is it secret?** | Yes or no. | The type `Secret`. | On the input |

And, for inherited settings only: **does this level differ from the levels above?** `override`, `remove`.

The signature holds the answers that matter to the caller: what do I pass, and what do I get back? The block holds the answers that matter to the wire.

| Fact from Riot's docs | The lines |
|---|---|
| Every request carries the API key. It is secret. | `client Riot(key: Secret)` and `headers { "x-riot-token" = key; }` |
| Match endpoints use a regional host. | `scope matches(route: MatchRoute)` and `base "https://{route}.api.riotgames.com";` |
| They live under `lol/match/v5/matches`. | `path "lol/match/v5/matches";` |
| `count` filters the list. | `count?: u32` and `query { count; }` |
| The list is paginated by `start`; a page shorter than `count` is the last one. | `paginate by_offset_limit(start, count);` |
| An unknown player is a `404`. | `status { 404 => None }` |
| `me` uses the player's token, not the key. | `headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }` |
