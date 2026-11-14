# The macro: how a description becomes code

*[Concord](../../README.md) · [docs](../README.md) · internals · previous: [the engine](engine.md) · next: [engineering rules](engineering.md)*

For someone who wants to change `concord-macros`, or to know what `concord::api!` writes: what the macro may know, the steps from a description to code, how a name and a type of the author reach the generated code, where a message points, and what the generated code costs.

What it reads: [the grammar](../reference/grammar.md). What it says: [the messages](../reference/messages.md). The code: [`concord-macros/src`](../../concord-macros/src). What it writes: [`tests/tour-gen/src`](../../tests/tour-gen/src), and the same clients by hand in [`tests/tour/src`](../../tests/tour/src).

## What the macro may know

| It knows | It does not know |
|---|---|
| The shape of the tree: `client`, `scope`, `part`, an endpoint and its method. | Any format, scheme or pager. `Json`, `json`, `by_offset`, `basic` are names like any other. |
| The words that start a statement, and `override` and `remove`. | Any item of `concord-extras`. |
| What a template is, and what a status key is (`404`, `4xx`). | What an expression means. It copies it. |
| The mark `?` on the name of an input (`count?: u32`): that input is optional. | The type of an input. An alias changes nothing. |
| After the arrow, the spelling of two types, from a closed list ([C1.7](../reference/contracts.md)): `Option<T>` and `Result<T, E>`. A 2xx is then read as `T`. Behind an alias the whole type is read. And a reference in an input (`&…`), which it refuses. | Whose `Option` it is. It reads the spelling and nothing else. It never looks inside a type. |
| The names that it generates, and the ones it reserves. | |

What it writes names two things of Concord only: the hidden module `::concord::__private`, and public items of the core through the facade (`::concord::Answer`, `::concord::Pages`, …). The dependency must be named `concord`: the macro writes that name. **Every path that the macro or a helper macro of the core writes is a full path** (`::core::option::Option`, `::core::result::Result::Ok`, `::std::boxed::Box`): a client compiles in a module that shadows `Option`, `Result`, `Some`, `Ok`, `String`, `Vec`, `Box` and the names `concord`, `core` and `std`. A module under `#![no_implicit_prelude]` is not promised: a method call on the author's own value (`.clone()`, `.into()`) is written as the author wrote it. (A renamed dependency is not supported. Nor is a description that another macro writes: its names would not have the author's hygiene.)

## The steps

| Step | In | Out | It can refuse with |
|---|---|---|---|
| 1. Parse | Tokens | The tree: a client, its middlewares, its parts, its scopes, its endpoints; each with inputs and statements | [Messages 15 to 20](../reference/messages.md) (structure; 20 is for `problem` or `middleware` outside the client) |
| 2. Name | The tree | Every generated name, checked | [Messages 1 to 8](../reference/messages.md) |
| 3. Resolve | The tree | For each endpoint, one flat list: everything that holds for it, with the node that wrote each line | [Messages 21 to 24](../reference/messages.md) (inheritance), 33, 35 to 38 (parts), 39 (no arrow under an arm that reads), 43 (no `base`), 44 (`override 2xx` over an exact arm), 46 (an inherited 2xx arm that reads, under an `Option` or a `Result` after the arrow) |
| 4. Check | The flat lists | The same, known to be sound | [Messages 9 to 14](../reference/messages.md) (inputs), 25 to 32 (paths, headers, `HEAD`), 34, 40 to 42 (an optional input in `middleware`; `_` in a short header entry; `host`), 45 (a status key) |
| 5. Write | The flat lists | Rust | Nothing: what is wrong now is in the author's Rust, and `rustc` says it ([the messages that are `rustc`'s](#the-messages-that-are-rustcs)) |

**All the mistakes of one build are shown together**. Each step collects its errors and goes on where it can. A node that does not parse is skipped up to its closing brace, and its siblings are kept. When a step refuses, the macro still writes everything that it could: the client type, the handles, the endpoints that are sound. A mistake in one endpoint must not take the names of the others away from the editor. What a mistake takes away, exactly: a mistake in a scope or an endpoint takes that node and what is below it; one in a part takes every node that names the part; one in a statement of the client takes every child (the client type stays, since that statement holds for every child); one in the header of the client (its name, its inputs) or in its `problem` type takes everything, since every item names them. **Nothing is said about what stands under damage:** below a node, a block or an arm that an earlier step marked as damaged or refused, no message is raised whose truth depends on what was dropped. And a help line never leads to another message (one of the three rules for every message, in [the messages](../reference/messages.md)).

### Step 1: parse

The parser is `syn`. The words of the language are **contextual**: `query` starts a statement only at the start of a statement. So an input may have the name of any word of the language ([grammar](../reference/grammar.md)), and `query { query; }` is legal.

Rust inside a description is parsed by `syn` as Rust: a type (`syn::Type`), an expression (`syn::Expr`), a visibility, a doc comment, `#[cfg]`, `#[deprecated]`. The places where the grammar alone does not decide, and the choice:

| Place | The choice |
|---|---|
| A value of an entry: a template, or an expression | Parse one whole `syn::Expr`. If it is a bare string literal, it is a template. Anything else (`"a".to_owned()`, `x.join(",")`) is an expression. |
| `remove;` (an input named `remove`) against `remove "key";` | The next token decides. The same for `override`. |
| `name` against `owner.name` in a template hole or a `base` | Read the first name, then look for a `.`. |
| `idempotent;` against `idempotent <expression>;` | A `;` right after the word means no expression. |
| `problem T read X;` | `syn::Type` stops before `read`; then the word, then an expression. |
| An arm's expression | `syn::Expr` stops at the `,` of the arm, also with closures and generics inside. |
| `4xx` | For Rust it is the number `4` with the suffix `xx`. The macro checks the spelling: one digit from 1 to 5, then `xx`; or three digits from 100 to 599. |
| `METHOD "VERSION-CONTROL"` | The string is checked here: an HTTP token. [Message 19](../reference/messages.md). |
| `count?: u32` | A `?` between the name and the `:` marks an optional input. |
| `body json { a; b; }` against `body json(x);` | After `body`, a path directly followed by `{` opens a block of entries. Anything else is an expression that ends with `;`. |
| A template | Read after Rust has decoded the string (a raw string is a template too). A hole holds a name, or an owner, a `.` and a name; no space, no comment. A `{*x}` hole stands in a path only. |
| `#[doc = "…"]` | The same as `///`. |

`#[cfg]` stands on a node only (a client, a scope, an endpoint). The structure is checked first, with the node in it. Then the attribute goes on everything that is written for that node. An input and a part take doc comments and no other attribute ([message 14](../reference/messages.md)): a `cfg` on one input would have to remove every line that uses it.

### Step 3: resolve

For each endpoint the macro walks from the client down to it and collects:

| Thing | Rule | Kept with it |
|---|---|---|
| A part named in a signature (`..page`) | Its inputs come after the node's own; its statements stand before the node's own, as if the node had written them. A name, a key or a one-value statement that comes twice this way is refused ([message 38](../reference/messages.md)), and so is a part named twice. The `///` of the part's inputs come with them; the `///` of the part itself go nowhere. | The part, as the writer of those lines. |
| `base` | The nearest one. It may be the endpoint's own. None in reach is refused. | The node that wrote it. |
| `path` | Every one, root first. | The node, for each piece. |
| An entry of `query` or `headers` | By key (a header name without case). A key set above is set again only with `override`, and leaves with `remove`. Inside one node a key is written once. A key keeps the place where it was first written. **An entry, or a piece of a `path`, that is inherited under another `base` goes through one more check** ([the messages that are `rustc`'s](#the-messages-that-are-rustcs)): a value that holds a `Secret` stays with the `base` where it was written ([rule 16](../reference/rules.md#behaviour)). So the macro wraps an inherited value in the check when a `base` was in reach at the writer and one more is written below it on the way, or when none was in reach and two are written below. For a piece of a `path` the words of the error say to write the `path` under each `base`. A short entry in `headers` whose name has `_` is refused; so is the key `host`. | The node. |
| `read` | The nearest one. One is needed only where a body is read: by the simple rule, or by an arm that binds `body` ([message 33](../reference/messages.md)). | The node. |
| An arm | By what it takes: a class arm from above takes every status of its class. An exact status under an inherited class arm needs `override`; `override 2xx` over inherited exact arms of that class is refused until each is overridden or removed. An exact status is asked before a class; in one node a class and an exact status of it may both be written. `remove 4xx` takes away the arm written `4xx` and nothing else. For a `HEAD`: the edits are resolved first, then an inherited arm that binds `body` is left out. An endpoint with no arrow that an arm in reach would read is refused. | The node. |
| `problem`, and its reader | The client's. | |
| `body`, `paginate`, `idempotent` | The endpoint's own (or its part's). A `body` of entries in a part and one in the node, with the same function, are put together, the part's entries first. | |
| `idle_timeout` | The nearest one (the endpoint's, a scope's, the client's, or a part's). | The node that wrote it. |
| `middleware` | The client's, every one, in the order written. They are not part of an endpoint's list: `new` makes them ([what is written](#what-is-written)). An optional input of the client named in one is refused by the macro (it looks for the name among the tokens), so that Rust cannot find another item of that name. | The client. |

"The node that wrote it" matters for one reason: [a name of the author, in the author's Rust](#a-name-of-the-author-in-the-authors-rust).

## Where the generated items live, and how they reach the author's types

A description is written in some module of the author, where the author's names mean something: `Puuid`, `RiotProblem`, `Json`. The generated call types live elsewhere: at their tree paths, in modules that the macro makes (`riot::matches::Ids`). Inside those modules, `Puuid` means nothing. This is the contract that joins the two.

| What is written | Where | Why |
|---|---|---|
| The client type, the modules of the tree, the handle and call types (their `struct` lines) | In the module of the description, and in modules below it | So that `riot::matches::Ids` is a path. |
| **One hidden trait for each client, `__Types`**, with one associated type for each input (`__Input0`, `__Input1`, …, numbered in the order of the tree, every level together), one for each node that is not an endpoint (the type of a `__parent` field: `__Node<n>`, where `n` is the place of the node in the tree, the client first, `__Node0`), and `__Problem` for the problem type (`()` with no `problem` line). It has the visibility of the client. | The trait in the client's module; **its one `impl` for `()`, in the module of the description** | In the `impl`, `type __Input3 = Puuid;` is written where `Puuid` means what the author meant. A field of a generated type is then `<() as __Types>::__Input3`, and the field that holds the level above is `__parent: <() as __Types>::__Node0`. (This compiles where a plain `Puuid` inside the module does not; it is how [`tests/tour`](../../tests/tour/src/declarations.rs) is written. It also works for a description inside a function, as in a doc test, where no `super::` path reaches the author's names. What `api!` writes are items: like every item of Rust they cannot name a generic parameter of an enclosing function (`rustc` says it, E0401), and `Self` in a type of a description would become `()` here, so the macro refuses it: [message 10](../reference/messages.md).) |
| Every `impl` block: the methods, the setters, `__build_draft` and `__build_request`, `__read_answer`, `__prepare_inner`, the line of `call_methods!`, the pagers; and the views of [a name of the author](#a-name-of-the-author-in-the-authors-rust) (`__View0`, …) | In the module of the description, inside one `const _: () = { … };` | Every expression and every type of the author is copied there, and resolves as the author wrote it. The `use` lines of the author are in reach. |
| The slot types of `paginate` | Hidden, in the client's module | |

So a type of the author is named in two kinds of place only: in the `impl` of the hidden trait, and in `impl` blocks at the level of the description. A wildcard import (`use super::*;`) is never written.

Two limits, both seen in `tests/tour`, which is written this way. **An input type must be at least as visible as the client:** a private type on a `pub client` is Rust's error E0446 (it would be an error on the public method too). And **the aliases show the hidden trait** (rule 6 of [the generated documentation](#the-generated-documentation)): `riot::Error` and `riot::Retry` are declared inside the client's module, so `rustdoc` prints `Error<<() as __Types>::__Problem>`. The text under each names the real type, with a link.

Paths: a path into `concord` or `std` starts with `::`. A path to a generated item is relative to the module of the description (`riot::matches::Ids`). So an endpoint may be named `copy` or `send_all`: its type `Copy` hides nothing that the macro writes.

## A name of the author, in the author's Rust

Rule 4 of the [rules](../reference/rules.md): a bare name is an input of the node that wrote the statement, and an input of a node above is named with its owner (`client.key`, `matches.route`).

The macro does not rewrite expressions. **It binds names, and then copies the expression as it is.** Before an expression that a node `N` wrote, the generated code has one local for each owner in reach, and one for each input of `N`. Every one of them is a reference.

```rust
// for the entry   "x-riot-token" = key;   written by the client, in an endpoint under `matches`
{
    let client = __View0 { key: &self.__parent.__parent.key };   // the owner: a view, one reference for each of its inputs
    let key = &self.__parent.__parent.key;                       // each input of the node that wrote the line: a reference
    let __value = ValueRef(key);                                 // the adapter, for a secret
    let __adapted = (&__value).adapt();
    let __Entry0 = &[Piece::Val("x-riot-token", &__adapted)];    // a name, for a Build error
    …                                                            // the other headers run here
    __draft.header("x-riot-token", __Entry0);                    // then all are written, in the order of the wire
}
```

(The paths into `::concord::__private` are cut short here.)

**An owner is a view.** For each level the macro writes one small hidden struct whose fields are references to the inputs of that level (`__View<n>`, numbered as the `__Node<n>` of [where the generated items live](#where-the-generated-items-live-and-how-they-reach-the-authors-types): `__View0` is the client's). A level with no inputs has a unit struct, bound as `let matches = __View4;`. So `client.key` is a `&Secret`, exactly as the bare `key` is in the client's own lines. (With a plain borrow of the handle in its place, `basic(client.key, "")` would not compile, and descriptions of real APIs write it.)

**Every value goes through one adapter**, `(&ValueRef(x)).adapt()` above: a few lines of the hidden module that pick, by the type of the expression, the way to write it. `Secret` and `Option<Secret>` (and references to them) are known first; everything else must be `serde::Serialize`. `Secret` itself is not serializable, so it cannot end in a body by accident. Each value also comes with a name, for the text of a Build error: the input for a hole and for a `name;` entry; the key for an entry written `"key" = value;`, also when the value is a bare input.

| Where the expression stands | What a name of an input is bound to |
|---|---|
| A template, an entry, `body`, `read`, `idempotent` | A reference to the input. For an optional input (`count?: u32`) that is a `&Option<u32>`. |
| `paginate` | The input itself, as a place: a `let` of a value with no size that is `Copy` and has `concord::page::Slot` (get on a call, set on the next). An owner (`client.x`) is still a reference. What the expression gives must own what it holds, so a pager written in place as a closure is a `move` closure; without the word, `rustc` says to add it. |
| An arm | A reference to the input. Then the names of the arm's own pattern, which may hide an input (that is Rust's shadowing; a rename, `body: task`, keeps both). |

`client.key` is then plain Rust: a field of a local variable. Because the bindings are references, the core takes references where it takes values: `Reader<T>` is implemented for `&R` (without it, `read r;` with an input `r` does not compile).

**An input hides an item of its name.** In the lines of a node, `form` is the input `form` if the node has one, and the function `form` is then written with its path (`concord::form(form)`). That is Rust's own rule. **One place is not an expression:** in `body f { … }`, `f` is a path to a function, and the macro calls it from a small function of its own that sees no input, so an input named `f` does not hide it (and the same spelling in a part and in the node is the same function). It is why the shipped pagers are named `by_offset`, `by_cursor`, `by_page`: `offset`, `cursor` and `page` are what APIs call those inputs. And a bare name that is not an input of the node is whatever Rust finds there.

**The names of owners are kept free.** An input named `client`, or named like a scope above it, would hide that owner. It is refused, with [message 7](../reference/messages.md), at both places.

**Hygiene.** The bindings that Concord gives to the author (the inputs, the owners, and in an arm `body`, `headers` and `status`) carry the span of the author's own identifier, so the author's expression sees them. **Every other binding that the macro writes in a function where an expression of the author stands has mixed-site hygiene** (`Span::mixed_site()`: `__draft`, `__response`, `__status`, `__headers`, `__body`, `__info`, `__value`, …): the author's expression cannot read it, change it, or collide with it. **One thing hygiene does not do:** Rust reads `let __value = …` as a pattern when a `const` or a `static` named `__value` is in reach, and no hygiene on stable Rust prevents it (E0530). So **the names that begin with `__` belong to Concord**: in a module that holds `concord::api!`, the author declares no item whose name begins with `__`. The same holds for an input: a `const` or a `static` in reach with the name of an input is not hidden by it, and the description does not compile (E0530; [rule 4](../reference/rules.md)). Bindings that an expression does not use are allowed (`#[allow(unused_variables)]` on the block).

**What the macro reads in an expression:** only whether the identifier of an input appears among its tokens, for two checks: "an input is never used" ([message 9](../reference/messages.md)), and an optional input in a `middleware` expression. A field or a closure parameter that happens to have that name counts as a use; that is harmless. Inside a macro call, a name in a string counts too (`format!("{n}")` reads `n`). "An input of a node above needs its owner's name" ([message 12](../reference/messages.md)) is said only where the macro reads the reference itself: in a template hole, in a `name;` entry, in a `base`. Inside an expression, a name that does not exist is `rustc`'s message.

**How each expression is written.** When each expression runs, and what `?` does in it, is a fact for the user: [when an expression runs](../reference/grammar.md#when-an-expression-runs), in the grammar. This is what the macro writes around each one.

| Expression | What the macro writes around it |
|---|---|
| An entry of `query` or `headers`, `body` | A closure that gives `Result<_, Box<dyn Error + Send + Sync>>`, run when the request is built. An `Err` becomes the Build error: for a `body`, through the hidden helper `build_error`; for an entry, through `Draft::entry_error(endpoint, name, error)`. |
| An entry of a `body` | The same. The values go into one `concord::body::Entries`, which asks `serde` whether a value is a `None`, so the macro knows no type. A `Secret` goes through the adapter of every placed value. |
| `idempotent` | A closure that is not `async`, run once for a send, after `idle_timeout`. The hidden helper `idempotent(…)` takes what it gives, a `bool` or a `&bool`. `idempotent;` alone is `Idempotent::Yes`, and no `idempotent` is `Idempotent::ByMethod`. |
| `read`, and the reader of `problem` | A closure that is not `async`, run at most once for each answer, before the engine decides what it reads. The hidden helper `reader::<T, _>(…)` takes what it gives. |
| An arm | A closure whose parameters are the fields of its pattern, in their order: `headers: &Headers` and `status: StatusCode` typed, `body` not. Its expression is copied as `Ok({ EXPR })`. |
| `paginate` | The expression, with each input bound as its place ([the table above](#a-name-of-the-author-in-the-authors-rust)); run once, at `.pages()`. |
| `idle_timeout` | A closure that is not `async` and gives a `std::time::Duration`, run once for a send, first of all. |
| `middleware` | The expression, in `new`, where only the required inputs of the client are bound (an optional input is set after `new`). The macro refuses an optional input of the client there ([message 40](../reference/messages.md)); a name that does not exist is `rustc`'s error. |

**`return` in a closure.** In `read { … }`, `idempotent { … }` and a pager, the macro applies its helper (`reader`, `idempotent`) to what the closure gives, outside it: so `return v` gives the value of that expression. In a status arm and in the expression of `body`, the closure gives a `Result`: so `return` gives what `?` would give.

**Two orders.** For each send (each page is one), `idle_timeout` runs first, then `idempotent`, then the request. The expressions of a request run by kind first (the base, the paths, the query, the headers, the body), and inside a kind in the order of the tree: the client first, and for a node the lines of its parts before its own. The entries go on the wire in the order in which each key was first written. After an `override` the two differ (it runs at the turn of the node that wrote it), so the generated code runs every expression of a kind first and writes its entries after.

**An arm, exactly.** Its pattern holds `body`, `headers`, `status`, each at most once, each with a name of the author's if wanted. The expression is copied as `Ok({ EXPR })` into a function that gives `Result<A, concord::Fail<P>>` ([C5.3](../reference/contracts.md)). `body` is the read value, owned. Its type is found by Rust from the arm's expression, and need not be the type after the arrow. `headers` is a `&concord::Headers`. `status` is an `http::StatusCode`. **The simple rule and the arrow.** For an arrow spelled `Option<T>` or `Result<T, E>`, a 2xx that no arm takes is read as `T` and wrapped (`Some`, `Ok`); the macro takes `T` out of the tokens (also for a path and for a generic `T`; behind an alias the whole type is read). When Rust cannot find the type of `body` from an arm, the arm says it: `4xx { body } => { let odd: UploadError = body; Err(odd.into())? }`. A `HEAD` with an arrow has nothing to read: it needs at least one arm for a 2xx ([message 32](../reference/messages.md)), and for a 2xx that no arm takes the generated code calls the hidden helper `no_arm(response, &info)`: a Mismatch that keeps the evidence, as a refusal does. **How the status is asked:** `__read_answer` is one `match` on the status, with a guard for each arm in reach, exact statuses first (`_ if __status.as_u16() == 404`, and for a class `_ if __status.as_u16() / 100 == 4`); then the simple rule for a 2xx (for a `HEAD`, `no_arm`); then `refused` (or `refused_plain` with no `problem` line) for the rest.

## What is written

For a client `Riot`, as the macro writes it from [`tests/tour-gen/src/riot.rs`](../../tests/tour-gen/src/riot.rs) (and as [`tests/tour/src/riot.rs`](../../tests/tour/src/riot.rs) writes it by hand):

| For | Items |
|---|---|
| The client | `pub struct Riot`; `Riot::new(…)`, which also makes each middleware of the description, in order, and hands it to the core; the settings (`with_retry`, `with_timeout`, `with_idle_timeout`, `with_base`, `with_transport`, `with_middleware`); one method for each child; a setter for each optional input of the client; `Debug` that prints no input. The module `riot` with `riot::Error`, `riot::Result<T>` and `riot::Retry` (`concord::Retry<RiotProblem>`), and the hidden trait of [where the generated items live](#where-the-generated-items-live-and-how-they-reach-the-authors-types). |
| A scope | A handle type at its tree path (`riot::Matches`), with one method for each child, a setter for each optional input of the scope, and `Debug`. The hidden view of [a name of the author](#a-name-of-the-author-in-the-authors-rust). |
| A part | Nothing. |
| An endpoint | The method on its parent, with the generated documentation. The call type at its tree path (`riot::matches::Ids`). A setter for each optional input. `__ENDPOINT`, its name. `__build_draft` (the flat list of step 3, in order, on a `Draft`) and `__build_request(take)`. One private function for the answer, `__read_answer` (the arms, the reader, the problem), used by a send and by `read_response`. **`__prepare_inner`**, the first half of a send: the `Run`, `idle_timeout`, `idempotent`, then `__build_draft` and `Draft::prepare`; it gives a `Prepared`, whose `send` is the second half. One line of the core's hidden macro `call_methods!`, which adds what every call has, each written on those, and for a `GET` or a `HEAD` the second first half `__prepare_if_none_match(etag)` ([the hidden interface](engine.md#the-hidden-interface-method-by-method), in the engine, has the table: every public method of a call, and what it is built on; `answered_from(url)`, which the machine of pages calls, is there too). With `paginate`: `pages()`, and a slot type for each input (`.items()` is a method of `Pages`, not of the call). A `Debug` that prints no input. |

**Every name that the macro makes for itself starts with `__`:** its private methods and constant (`__build_draft`, `__build_request`, `__read_answer`, `__prepare_inner`, `__ENDPOINT`), the fields that are not inputs (`__core`, `__parent`, `__options`), the hidden trait and its types (`__Types`, `__Input0`, `__Node0`, `__Problem`), the views (`__View0`), the slot types (`__Slot…`), and its locals. `call_methods!` adds `__prepare_if_none_match`. A name of the author cannot start with `_`, so an optional input can never take the name of a helper such as `__build_request`.

**A call to an endpoint with pages has no `IntoFuture`.** `.send()` gives one page, `.pages()` gives them all, and `.pages().items()` each item. The mistake `call.await` gets `rustc`'s own words (`is not a future`); the `#[must_use]` text of the call says what to write. A call whose method is `GET` or `HEAD` also gets `send_if_none_match(etag)`, which is `__prepare_if_none_match(etag)` and then `Prepared::send`.

**Attributes.** `#[cfg]` on a node goes on everything that depends on the node, the associated types of the hidden trait too. A use of a deprecated item inside generated code does not warn (`#[allow(deprecated)]` there); the caller still gets the warning. A visibility that is relative (`pub(super)`) is written again for each depth of module, so that it means what the author wrote. `#[doc(hidden)]` on a scope or an endpoint hides what is generated for it.

**The order of `__build_draft` is fixed** (the two orders of [a name of the author](#a-name-of-the-author-in-the-authors-rust)), and an expression that fails ends the building there. An author's expression may have an effect, so the order is part of the contract: [C10.3](../reference/contracts.md).

**The flat list is written out in each endpoint.** A header set on the client is one line in the `__build_draft` of every endpoint below. This is the simple shape: an endpoint can be read alone, and `override` and `remove` cost nothing at run time. Its price is size: see [size and compile time](#size-and-compile-time).

**Reserved names.** A name that becomes a method (a child, or an optional input) cannot be a method that its receiver already has (on a call, `at_url` too: pagers use it), and cannot start with `with_`. Concord adds no name to a call, a handle or a client in a minor version unless it starts with `with_`. The one table is [C1.6](../reference/contracts.md). A required input is an argument; it makes no method and may have any name. [Message 3](../reference/messages.md) says so.

## Where a message points

The wording and the place of each message are in [the messages](../reference/messages.md). This is how the macro does it on stable Rust.

- **What is fixed.** Of the three lines of each of the 46 messages, the first (the message) and the place it points at are those of the messages page, word for word, and the code follows them (for [messages 18 and 20](../reference/messages.md) the place is the keyword). The label and the help are what the macro prints for the case: a help is in words where no exact replacement is safe (one of the three rules for every message, on the same page). The page copies them from the compiler fixtures. A test of contract C1.18 (its name starts with `c1_18_`) runs each case of the page through the whole macro and compares the three lines.
- A message is a `compile_error!` at a span. `syn::Error::new_spanned(token, text)` gives it; errors are joined with `combine`, so that one build shows them all.
- **One message, three lines.** Stable Rust gives a macro no labels and no help lines. The three parts of a message on the page are the lines of one text: the error, then what the place is, then what to write.
- **Two places.** For a clash ("already declared", "is already set above") the macro gives a second error at the earlier place, with a text that says what stands there: `the first child is here`, `the inherited header is set here`, ``the part `page` writes it here``.
- **A name proposed by [message 35](../reference/messages.md)** is the one part of the client nearest to the written name by edit distance, when no other part is as near, and when it is at most a third of the length of the written name away (at least 1 edit, at most 3). Else the help names no part.
- **Inside a string, the whole string.** Stable Rust gives no span inside a literal. A message about a hole points at its template, and names the hole in words.
- When `proc_macro::Diagnostic` is stable, the same texts become real labels. Nothing else changes.

## The messages that are `rustc`'s

Everything inside the author's Rust is checked by `rustc`, and the message is its own: a name that does not exist (`read Jsno;`), a type that does not fit. **One mistake, one place, perhaps several errors.** For a mistake that only `rustc` can see (a type of the author that lacks a bound), the first error points at the author's token. `rustc` may print the same lack several times, once for each generated item that needs the bound (`problem Rc<String>` gives 12). The macro's own 46 messages are each given once. Three things make those messages good.

1. **The author's tokens keep their spans.** The error is under what the author wrote, not under `api!`.
2. **Each expression goes through one function whose bound says what is wanted, and the call of that function has the span of the expression.** `read X;` becomes `::concord::__private::reader::<T, _>(X)`, a function with the bound `R: Reader<T>` that gives a small wrapper which carries `T` (so Rust finds the type that is read, also for the problem type, and `?` cannot be used on what it gives). (When only `X` has the author's span, the error covers the whole `api!`; when the call has it too, the error is at `X`.)
3. **The core's traits say it in Concord's words**, with `#[diagnostic::on_unimplemented]` (Rust 1.78). The words name the type, not the expression: the compiler gives no more.

| Trait | When it is not met, at the author's expression |
|---|---|
| `Reader<T>` | `` `BadReader` cannot read a `Vec<MatchId>` `` · note: `write read Json; with use concord::Json;, or give BadReader a concord::Reader<Vec<MatchId>>` |
| `IntoBody` | `` `Task` is not a body `` · note: `a body is what json(x), form(x) or a function of yours gives: a concord::Body` |
| `__private::Value` | Its words (`` `Filter` cannot be written in a path, a query or a header `` · note: `it needs serde::Serialize, and must give one text, a list of texts, or nothing`) are never printed. A value that is neither a `Secret` nor `serde::Serialize` gets `rustc`'s E0599 at the author's expression: `` the method `adapt` exists for reference `&concord::__private::ValueRef<&Filter>`, but its trait bounds were not satisfied ``, with notes that name `SecretInput` and `Serialize`. |
| `page::Slot<C>` | `` `u32` is not an input of this endpoint `` · note: `in paginate, a pager is given inputs: paginate by_offset(start);` |
| `page::IntoNext<C>` | `` a pager gives the next call: Some(call), None, or a Result of that `` |
| `Transport` | `` `X` cannot send a request `` · note: `a reqwest::Client, concord::Http, a fake, or a type with concord::Transport` |
| `Middleware` | `` `X` is not a middleware `` · note: `implement concord::Middleware with handle(request, info, next); next.send(request, info) sends on` |
| `page::IntoItems` | `` `Option<Vec<Task>>` is not a list of items `` · note: `implement concord::page::IntoItems for your page type` |
| `__private::InheritAcrossBase` | `a secret does not follow a change of base` · note: `write this entry again with override, or remove it, under the new base` (at the author's entry) |
| `__private::InheritPathAcrossBase` | `a secret in a path does not follow a change of base` · note: `write this path under each base that needs it, rather than above those bases` (the same gate, for a piece of a `path`: a `path` cannot be written again or removed) |

The macro still knows none of these items. The words belong to the traits.

## Size and compile time

Measured through the macro itself, with Rust 1.99, offline, four jobs, the dependencies already built. The clients mix six shapes of endpoint in turn: a `GET` with a path and a query, a `POST` with `body json(…)`, a paged `GET`, a `HEAD` with arms, a `Result` with an arm, a body template. "Expansion" is the macro's own run from tokens to Rust; "lines" are the lines that it writes. Memory is the peak of one process.

| Shape | Expansion | First `cargo check` | First debug build | Lines written |
|---|---:|---:|---:|---:|
| 100 endpoints | 0.08 s | 2.5 s · 339 MiB | 3.7 s · 549 MiB | 37,497 |
| 500 endpoints | 0.38 s | 12.8 s · 1.06 GiB | 18.3 s · 1.74 GiB | 187,849 |
| One endpoint with 200 inputs | 0.02 s | 0.37 s · 177 MiB | 0.45 s · 262 MiB | 4,416 |

- **The macro itself is cheap:** under half a second for 500 endpoints.
- **The lines grow in a straight line** with the endpoints: about 375 for each of these shapes.
- **So does the check, here:** five times the endpoints take 5.2 times as long.
- **Wide inputs stay small.** The facts of one origin share one set of bindings, so an entry does not repeat the bindings of every input: one endpoint with 200 inputs is 4,416 lines and a check of 0.37 s.

**Clients larger than 500 endpoints are not measured.**

To measure the expansion yourself, run `just bench --expansion tests/tour-gen/src/tasks.rs` (that file is the default); it writes `target/expansion-measurement.json`. Give another Rust file with a description to measure that client.

## The generated documentation

The rules that the documentation written by the macro follows. The macro can write each line from the description alone.

1. An endpoint's method says, in this order and only when it applies: the author's lines; the shape of the call; the request line; the names of the headers that are written for it (names, never values); each input with what it accepts; each option; one line that names the statuses with a meaning of their own here (`404`, `2xx`), without the arm (what each means is the author's to say, in the lines above); a line when the endpoint may not be repeated; what `.await` gives (for a paged endpoint, what `.send().await` gives: one page); pages, with one sentence: `.send().await` gives one page, and `.pages()` gives them all.
2. **A type named in generated text is a link, written as a caller writes it:** `riot::Result<Vec<MatchId>>`, never `crate::riot::Result<Vec<crate::MatchId>>`. A type of the author that has the name of a handle is written with its path.
3. **A client with no `problem` line shows no `()`:** `Retry`, `Sending<Vec<Task>>`, `concord::Error`.
4. **A setter says what it accepts:** "Accepts a `u32`, a `&u32`, or an `Option<u32>`. `None` clears it.", with the type as the author spelled it and a link to `concord::arg::Arg` for text and lists. The macro does not guess what an alias is.
5. **The author's example is on the client**, as written in the `///` lines above `client`. The macro writes no example of its own: it has no real values.
6. What a caller never calls is hidden (the blanket implementations of `concord::arg::Arg` too, or `rustdoc` lists them on every page of every client): the traits that pagers use, the slot types, the hidden trait of [where the generated items live](#where-the-generated-items-live-and-how-they-reach-the-authors-types). **Two lines show that trait:** `<client>::Error` and `<client>::Retry`, the aliases over the problem type of the client. Every other public signature names the author's type itself (`RiotProblem`), or nothing when the client has no `problem` line.
7. The words are the caller's. Generated text never says "the description" or "the arrow".
8. The inputs of a part are shown like the node's own. The name of the part is not shown.
9. The page of a client says that every request goes through a middleware, with the `///` lines that the author wrote above each `middleware`. The text of the expression is never printed: it could hold a literal key.
10. A paged endpoint says: `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.

## The editor

Editor support inside `api! { }` has not been verified: what `rust-analyzer` does there is not known.
What the macro does for an editor: the author's tokens keep their spans, the names of [a name of the author](#a-name-of-the-author-in-the-authors-rust) are real `let` bindings, and the macro writes what it can when a part is wrong ([the steps](#the-steps)).
Not known: whether hover, "go to definition" and completion (of a name, or of the statement words) work inside the braces, and how an expansion of 0.38 s for 500 endpoints feels on each key press.
