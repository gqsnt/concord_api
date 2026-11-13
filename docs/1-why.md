# 1. Why

*Concord V5 design · chapter 1 of 9 · [index](../README.md) · next: [2. The model](2-model.md)*

Why Concord exists: the gap between what a REST API is and how code represents it.

## 1.1 The idea

1. A REST API is a tree: things apply from the root to the leaf.
2. A hand-written client must choose. Flat loses the domain. Scoped loses locality and costs a lot of code.
3. The problem is the representation, not REST. REST is a small, closed world.
4. So we write the API as a nested description, and a [macro](macro.md) generates the real structs and methods.
5. Because the macro reads the description, it can promise things for you: a secret is never shown, a value stays in its place.
6. The macro owns the tree and nothing else. What an API *does* beyond one request (pages, trying again, signing) is plain Rust, and the HTTP library stays in reach below.

Two levels follow. **Level 1** is the true representation of the API. **Level 2** is everything the system can assert and do because it can reason about level 1.

## 1.2 The friction

A REST API is a tree. The code that calls it is usually not. All the friction comes from that gap.

```text
https://{route}.api.riotgames.com        the key in a header, JSON everywhere
└─ lol/match/v5/matches                  route: americas | asia | europe | sea
   ├─ by-puuid/{puuid}/ids               GET → list of match ids
   └─ {id}                               GET → one match
```

The key, the host, the route and the JSON format are true for both endpoints. They are facts of the levels above, not of the endpoints.

## 1.3 Two ways to write a client by hand

| | Flat: one function per endpoint | Scoped: one type per level |
|---|---|---|
| Domain knowledge | Lost. The tree lives only in your head. | Kept in the code. |
| Reading or writing one endpoint | Easy. Everything is in one function. | Hard. The endpoint is spread over several types. |
| Adding an endpoint | Risky. Nothing says which rules apply. | The rules apply, but you must find your way through the levels. |
| Changing a shared rule | One edit per endpoint. Easy to miss one. | One edit. |
| Deleting | Free. | Costly, and an empty level stays behind. |
| Mental context | Fine while the client is small. | Bad. |
| Amount of code | The same facts in every leaf. | A lot of structural code. |

So people write flat clients, or a company pays a team to maintain an SDK. A builder is shorter than the types, but nothing is checked.

## 1.4 Why both lose

Flat code has **locality** (everything about one endpoint in one place) and no **domain** (the shape of the API in the code). Scoped code has the reverse. Hand-written code cannot have both, because in a programming language structure costs code: a type, a constructor, forwarding methods, copies of shared values.

In a nested text, structure costs indentation. An endpoint is a few lines inside the block of its scope, and everything that applies to it is in the blocks around it. The context is neither copied nor far away. It is around the endpoint.

## 1.5 The hypothesis

REST is not that complex. It has a fixed set of parts. If we understand them, we can represent them truthfully.

The web had the same problem with pages. A page is a tree; you can build it with calls, but people write the tree as a tree. Leptos, Vue and React all ended there. Concord asks the same question for REST: what is the text that has the shape of an API?

## 1.6 What the representation must give

| # | Requirement |
|---|---|
| R1 | The tree of the API is visible in the text. |
| R2 | One endpoint can be understood by reading it and its parents. |
| R3 | Adding an endpoint is a few lines, and the rules of its scope apply without any action. |
| R4 | A rule shared by many endpoints is written once. |
| R5 | An exception to an inherited rule is always written, never silent. |
| R6 | Deleting an endpoint or a level is deleting its lines. |
| R7 | Someone who only wants to use an endpoint reads its signature and nothing more. |
| R8 | Real structs and methods are generated behind, checked by the compiler. |
| R9 | The language is small enough to read from one example. |
| R10 | A user can always step down one layer: to a Rust value, to the HTTP library, to a function in place of the network. |

## 1.7 What Concord owns, and what it does not

The first version of this design tried to understand all of HTTP: how a body is read, how a redirect is followed, how a stream is cut into records. Each thing the macro understands must be defined against every other one, and the rules grew to 166. Three models that did not know the design were then asked to start again from this chapter. They found the same cure, each alone (the map).

| Owner | What |
|---|---|
| **Concord** | The tree. Where each input goes on the wire. What type comes back. And two waits, so that a silent server never holds a call ([chapter 4](4-usage.md), section 4.6). |
| **Rust** | The data types (plain `serde` structs). And every behaviour, as a value with one job: how to write a body, how to read an answer, how to get the next page, what a request goes through as it leaves. The macro knows none of them: what Concord ships (`json`, `Json`, `by_offset`) is written like yours, in a crate of its own. |
| **The HTTP library** (`reqwest`) | Connections, TLS, proxies. It stays visible below: you can give your own client, and take the built request out. |

So the description says the **facts** of the API: this key goes in that header, this endpoint has pages, this `POST` is safe to send twice. It does not hold a policy. How often to try again is a choice of the application, and it is made where the client is built.

A test for a new word in the language: does it say where something goes, or what comes back? Then it may belong. Does it say what to *do*? Then it is a Rust value, and the language only needs a place to name it.

## 1.8 The bar

> "Concord should feel like a project that has 40 years of living experience. It should feel like something you can't find a problem with. If you found one fast, it means you didn't think enough. If you find one after long, it means it's like a tradeoff, like `Option` that can be hidden: you don't have a perfect choice."

Two consequences for these docs:

- **A problem that a reader finds fast must already have its answer here.** Either the design handles it, or it is written down as a choice.
- **Every choice that has another side is written down with that other side, and with the door** for who needs it: [chapter 8](8-scope.md), section 8.2. A door is something you write in your own code. It costs lines, never a fork of Concord.

How the design was held to this bar, and how far it got, is in `experience/`.
