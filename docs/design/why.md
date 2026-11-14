# Why Concord

*[Concord](../../README.md) · [docs](../README.md) · design · next: [alternatives](alternatives.md)*

This page explains why Concord exists, for a developer deciding whether to adopt it.

A REST API is a tree, and a hand-written client either loses that structure or keeps it at the cost of much structural code. With Concord, you describe the tree, and a macro generates the client. The macro handles the tree and nothing else: what an API *does* beyond one request (paging, retrying, signing) is ordinary Rust, and the HTTP library stays reachable underneath.

## A REST API is a tree

In a REST API, most facts do not belong to one endpoint: a key, a host or a format applies to everything below the level where it is set. In these two Riot API endpoints, the key, host, route and JSON format all come from the levels above:

```text
https://{route}.api.riotgames.com        the key in a header, JSON everywhere
└─ lol/match/v5/matches                  route: americas | asia | europe | sea
   ├─ by-puuid/{puuid}/ids               GET → list of match ids
   └─ {id}                               GET → one match
```

Client code is usually not a tree, and most of the difficulty of writing a client comes from that gap.

## Why hand-written clients fall short

There are two common ways to write a client by hand:

| | Flat: one function per endpoint | Scoped: one type per level |
|---|---|---|
| API structure | Lost: the tree exists only in your head. | Kept in the code. |
| Reading or writing one endpoint | Easy: everything is in one function. | Hard: the endpoint is spread over several types. |
| Adding an endpoint | Risky: nothing says which rules apply. | The rules apply, but you must find your way through the levels. |
| Changing a shared rule | One edit per endpoint, and one is easily missed. | One edit. |
| Deleting an endpoint | Free. | Costly, and an empty level is left behind. |
| Mental load | Light while the client is small. | Heavy. |
| Amount of code | The same facts repeated in every leaf. | A lot of structural code. |

So people write flat clients, or a company pays a team to maintain an SDK. A request builder used directly, as with `reqwest`, takes less code than a type for each level, but nothing checks that each request carries what its level requires, such as the key or the format.

Flat code has **locality**, everything about one endpoint in one place, but not the **structure** of the API. Scoped code has the structure but not locality. Hand-written code cannot have both, because in a programming language, structure costs code: types, constructors, forwarding methods and copies of shared values.

## Describe the API as a tree, and generate the code

In a nested description, structure costs only indentation. An endpoint takes a few lines inside the block of its scope, and everything else that applies to it is in the enclosing blocks: neither copied nor far away.

So the problem is the representation, not REST. REST is a small, closed world: methods, paths, query strings, headers, bodies and statuses.

Web pages had the same problem: a page is a tree, and although you can build it with function calls, people ended up writing it as a tree (Leptos, Vue, React). Concord does the same for REST: you write the API as a nested description, and a macro generates the real structs and methods (see [the macro](../internals/macro.md)). Because the macro reads the description, it can promise that secrets are never shown and values stay where the description puts them.

## The ten requirements for the description

| # | Requirement |
|---|---|
| R1 | The tree of the API is visible in the text. |
| R2 | You can understand one endpoint by reading it and its parents. |
| R3 | Adding an endpoint takes a few lines, and the rules of its scope apply automatically. |
| R4 | A rule shared by many endpoints is written once. |
| R5 | An exception to an inherited rule is always written out, never implied. |
| R6 | Deleting an endpoint or a level means deleting its lines. |
| R7 | To use an endpoint, a caller needs to read only its signature. |
| R8 | The macro generates real structs and methods from the description, and the compiler checks them. |
| R9 | The language is small enough to learn to read from one example. |
| R10 | You can always go down one layer: to a Rust value, to the HTTP library, or to a function that replaces the network. |

## What Concord handles, and what it leaves to Rust and the HTTP library

The macro handles the tree and nothing else. Each concept it understands must be defined in relation to all the others, so a macro that understood all of HTTP (how a body is read, how a redirect is followed, how a stream is split into records) would need a rule for every pair of them. Concord is therefore responsible for little.

| Who | What |
|---|---|
| **Concord** | The tree. Where each input goes on the wire. Which type comes back. And two timeouts, so that a silent server cannot hold a call forever (see [the two timeouts](../guide/calling.md#timeouts)). |
| **Rust** | The data types (plain `serde` structs). Every behaviour, as a value with one job: writing a body, reading a response, fetching the next page, processing a request on its way out (signing it, for example). The macro knows none of them: the ones Concord ships (`json`, `Json`, `by_offset`) live in a separate crate and are written like your own. |
| **The HTTP library** (`reqwest`) | Connections, TLS, proxies. It stays reachable underneath: you can supply your own client and get the built request. |

So the description states the **facts** of the API: this key goes in that header, this endpoint is paginated, this `POST` is safe to send twice. It contains no policy: the application chooses how often to retry when it builds the client.

This is also the test for a new statement in the description language. If it says where something goes in the request, or what type comes back, it may belong. If it says what to *do*, it belongs in a Rust value, and the language only needs a place to name that value.

Each tradeoff is documented with the alternative and an escape hatch for when you need it: see [the tradeoffs](scope.md#the-tradeoffs). An escape hatch requires code in your project, but never a fork of Concord.
