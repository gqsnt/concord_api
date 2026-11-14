# How Concord sees an API

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Concord in ten minutes](tour.md) · next: [Describe an API](describing-an-api.md)*

For someone who will write a description: the few ideas behind the language. What an HTTP API is made of, and how each fact applies from the root of the tree to the leaf. Read it once, and every statement of the language has its place.

## The unit

Concord describes one exchange: one request goes out, one response comes back.

Some things need several exchanges: trying again, getting the next page. Each of them is the same request, sent again with a small change. So the description still says one request, and a Rust value says how the next one differs ([retries](retries.md), [pages](pages.md)).

## The parts

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

## Two natures

Everything except the two bodies is text, or ends up as text.

| | Text places | Payloads |
|---|---|---|
| Where | host, path, query, headers | request body, response body |
| What a value needs | a type that `serde` can write as one text (or as a list of texts, in a query or in headers) | a Rust type that `serde` can write or read |
| What Concord does | the escaping that the place requires | hands the value to the function that the `body` line names, or the bytes to the reader |

A payload is **one value**. A body that is very large or that never ends is not a value: it is bytes, and the caller reads them as they come ([a body that is large, or that never ends](responses-and-errors.md#a-body-that-is-large-or-that-never-ends)).

So the language has two ways to mention data: a **template** in a text place, and a **Rust type** for a payload. One thing is shared: `serde` writes both, so a type has one wire form wherever it goes.

## One kind of node, three positions

| Position | Word | Only this position can have |
|---|---|---|
| Root | `client` | the type of the API's own error; the middlewares; the parts ([below](#how-things-apply-from-root-to-leaf)) |
| Middle | `scope` | nothing special |
| Leaf | a method: `GET`, `POST`, … | a body, a response type, pages, the `idempotent` mark |

Almost everything else can be written at every position. You learn each statement once.

## Two kinds of statements

| Kind | What it says | Its words | What stands on the right |
|---|---|---|---|
| **Placement** | Where something goes on the wire. | `base`, `path`, `query { }`, `headers { }` | Text, a template, the name of an input. The value of an entry may also be a Rust expression; its place and its key are always plain text. |
| **Behaviour** | What to do: write a body, read an answer, get the next page, give a status its meaning, sign a request as it leaves. | `body`, `read`, `paginate`, the arms of `status { }`, `middleware` | A Rust expression. Concord only needs the value to be of one known kind: a body, a value with one trait, a function. |

A body that is its inputs may also be written as a function and entries, like a `query`: `body json { text; cursor; }` ([the block](describing-an-api.md#the-block)). In all three blocks of entries (`query`, `headers`, such a body), a value that is `None` writes nothing.

And three plain facts: `problem` (the type of the API's own error), `idempotent` (whether this request may be sent twice, when that is not what its method says) and `idle_timeout` (how long these calls may be silent).

Placement is Concord's own small language. Behaviour is not a language at all: it is Rust.

## How things apply from root to leaf

| Thing | Going down the tree |
|---|---|
| `base` | The nearest one is where the URL starts. |
| `path` | The pieces are joined, root first. It is the one thing that adds. |
| Headers, query | Named entries. A child has all the entries of its parents. One exception: an entry that holds a `Secret` does not follow a change of `base` ([a secret stays with its base](credentials.md#a-secret-stays-with-its-base)). |
| `read`, `idle_timeout` | One for a request: the nearest. |
| Arms of `status { }` | Entries, by status. A child has the arms of its parents. A class arm (`4xx`) takes every status of its class, so an exact status under it is written with `override`. |
| `middleware` | Written on the client only. Every request of the client goes through it on its way out, and nothing below takes it away. |
| Body, pages, `idempotent`, the response type | Leaf only. |

> A key that is already set above (an entry of `query` or `headers`, an arm) is set again only with a word: `override` to replace it, `remove` to take it away. For a statement that holds one value (`base`, `read`, `idle_timeout`), the nearest one is used, and no word is needed.

**The tree shares downward. A part shares sideways.** Inputs that come back on many endpoints with no common parent (the inputs of a page with the line that pages, an idempotency key with its header, a route with the `base` that it fills) are written once, as a `part` of the client. A node names the part in its signature (`..page`), and then has those inputs and those statements as if it had written them. A part adds no level ([a part](describing-an-api.md#a-part-the-same-inputs-on-many-endpoints)).

"A tree" is a very good approximation, not an exact truth. [Scope and limits](../design/scope.md) says where it stops.

## Four questions per fact

To write down any fact of an API, an author answers four questions. Each answer has one spelling.

| Question | Possible answers | How it is spelled | Written in |
|---|---|---|---|
| **Where is it true?** | The whole API, a group, one endpoint. | The position: `client`, `scope`, an endpoint. | The nesting |
| **Who decides it?** | The provider, or the caller. | The caller: an input, marked with `?` when it may be left out. The provider: fixed text. | The signature, or the block |
| **Where does it go?** | Host, path, query, header, body. | `base`, `path`, `query { }`, `headers { }`, `body`. | The block |
| **Is it secret?** | Yes or no. | The type `Secret`. | On the input |

And for inherited things only: **is this level different from the levels above?** `override`, `remove`.

The signature holds the answers that matter to the user: what do I give, what do I get. The block holds the answers that matter to the wire.

| Fact from Riot's docs | The lines |
|---|---|
| Every request carries the API key. It is secret. | `client Riot(key: Secret)` and `headers { "x-riot-token" = key; }` |
| Match endpoints use a regional host. | `scope matches(route: MatchRoute)` and `base "https://{route}.api.riotgames.com";` |
| They live under `lol/match/v5/matches`. | `path "lol/match/v5/matches";` |
| `count` filters the list. | `count?: u32` and `query { count; }` |
| The list has pages, by `start`; a page shorter than `count` is the last. | `paginate by_offset_limit(start, count);` |
| No such player is a `404`. | `status { 404 => None }` |
| `me` uses the player's token, not the key. | `headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }` |
