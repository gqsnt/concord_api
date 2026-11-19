# The macro: how a description becomes code

*[Concord](../../README.md) · [docs](../README.md) · internals · previous: [the engine](engine.md) · next: [engineering rules](engineering.md)*

This page is for a developer who wants to change `concord-macros`, or to know what `concord::api!` generates. It covers what the macro may know, the steps from a description to code, how the author's names and types reach the generated code, where a message points, and what the generated code costs.

The macro reads [the grammar](../reference/grammar.md) and reports [the messages](../reference/messages.md). Its code is in [`concord-macros/src`](../../concord-macros/src). The descriptions it reads for the test clients are in [`tests/tour-gen/src`](../../tests/tour-gen/src), and the same clients, written by hand, are in [`tests/tour/src`](../../tests/tour/src).

## What the macro may know

| It knows | It does not know |
|---|---|
| The shape of the tree: `client`, `scope`, `part`, an endpoint and its method. | Any format, authentication scheme or pager. `Json`, `json`, `by_offset`, `basic` are names like any other. |
| The words that start a statement, and `override` and `remove`. | Any item of `concord-extras`. |
| What a template is, and what a status key is (`404`, `4xx`). | What an expression means. It copies it. |
| The mark `?` on the name of an input (`count?: u32`): that input is optional. | The type of an input. An alias changes nothing. |
| After the arrow, the spelling of two types, from a closed list ([C1.7](../reference/contracts.md)): `Option<T>` and `Result<T, E>`; a 2xx is then read as `T`. Behind an alias, the whole type is read. It also recognizes a reference in an input (`&…`), which it rejects. | Whose `Option` it is. It reads the spelling and nothing else, and never looks inside a type. |
| The names that it generates, and the ones it reserves. | |

What it generates names only two things of Concord: the hidden module `::concord::__private`, and public items of the core through the facade (`::concord::Answer`, `::concord::Pages`, …). The dependency must be named `concord`, because the macro writes that name. **Every path that the macro or a helper macro of the core generates is a full path** (`::core::option::Option`, `::core::result::Result::Ok`, `::std::boxed::Box`): a client compiles in a module that shadows `Option`, `Result`, `Some`, `Ok`, `String`, `Vec`, `Box` and the names `concord`, `core` and `std`. A module under `#![no_implicit_prelude]` is not guaranteed to work: a method call on the author's own value (`.clone()`, `.into()`) is written as the author wrote it. (A renamed dependency is not supported. Nor is a description that another macro generates: its names would not have the author's hygiene.)

## The steps

| Step | In | Out | It can reject with |
|---|---|---|---|
| 1. Parse | Tokens | The tree: a client, its middleware, its parts, its scopes, its endpoints; each with inputs and statements | [Messages 15 to 20](../reference/messages.md) (structure; 20 is for `problem` or `middleware` outside the client) |
| 2. Name | The tree | Every generated name, checked | [Messages 1 to 8](../reference/messages.md) |
| 3. Resolve | The tree | For each endpoint, one flat list: everything that applies to it, with the node that wrote each line | [Messages 21 to 24](../reference/messages.md) (inheritance), 33, 35 to 38 (parts), 39 (no arrow under an arm that reads), 43 (no `base`), 44 (`override 2xx` over an exact arm), 46 (an inherited 2xx arm that reads, under an `Option` or a `Result` after the arrow) |
| 4. Check | The flat lists | The same lists, known to be sound | [Messages 9 to 14](../reference/messages.md) (inputs), 25 to 32 (paths, headers, `HEAD`), 34, 40 to 42 (an optional input in `middleware`; `_` in a short header entry; `host`), 45 (a status key) |
| 5. Write | The flat lists | Rust | Nothing: what is wrong now is in the author's Rust, and `rustc` reports it ([the messages that are `rustc`'s](#the-messages-that-are-rustcs)) |

**All the mistakes of one build are reported together.** Each step collects its errors and continues where it can. A node that does not parse is skipped up to its closing brace, and its siblings are kept. When a step rejects something, the macro still generates everything that it can (the client type, the handles, the endpoints that are sound), because a mistake in one endpoint must not take the names of the others away from the editor. Exactly what a mistake takes away: a mistake in a scope or an endpoint takes that node and everything below it; one in a part takes every node that names the part; one in a statement of the client takes every child (the client type stays, since that statement applies to every child); one in the header of the client (its name, its inputs) or in its `problem` type takes everything, since every item names them. **Nothing is reported about what lies under damage:** below a node, a block or an arm that an earlier step marked as damaged or rejected, no message is raised whose truth depends on what was dropped. And a help line never leads to another message (one of the three rules for every message, in [the messages](../reference/messages.md)).

### Step 1: parse

The parser is `syn`. The words of the language are **contextual**: `query` starts a statement only at the start of a statement. So an input may have the name of any word of the language ([grammar](../reference/grammar.md)), and `query { query; }` is legal.

Rust inside a description is parsed by `syn` as Rust: a type (`syn::Type`), an expression (`syn::Expr`), a visibility, a doc comment, `#[cfg]`, `#[deprecated]`. These are the places where the grammar alone does not decide, and the choice made at each:

| Place | The choice |
|---|---|
| A value of an entry: a template, or an expression | Parse one whole `syn::Expr`. If it is a bare string literal, it is a template; anything else (`"a".to_owned()`, `x.join(",")`) is an expression. |
| `remove;` (an input named `remove`) versus `remove "key";` | The next token decides. The same holds for `override`. |
| `name` versus `owner.name` in a template placeholder or a `base` | Read the first name, then look for a `.`. |
| `idempotent;` versus `idempotent <expression>;` | A `;` right after the word means no expression. |
| `problem T read X;` | `syn::Type` stops before `read`; then comes the word, then an expression. |
| An arm's expression | `syn::Expr` stops at the `,` of the arm, even with closures and generics inside. |
| `4xx` | For Rust it is the number `4` with the suffix `xx`. The macro checks the spelling: one digit from 1 to 5, then `xx`; or three digits from 100 to 599. |
| `METHOD "VERSION-CONTROL"` | The string is checked here: it must be an HTTP token. [Message 19](../reference/messages.md). |
| `count?: u32` | A `?` between the name and the `:` marks an optional input. |
| `body json { a; b; }` versus `body json(x);` | After `body`, a path directly followed by `{` opens a block of entries. Anything else is an expression that ends with `;`. |
| A template | Read after Rust has decoded the string (a raw string is a template too). A placeholder holds a name, or an owner, a `.` and a name; no space, no comment. A `{*x}` placeholder is allowed in a path only. |
| `#[doc = "…"]` | The same as `///`. |

`#[cfg]` is allowed on a node only (a client, a scope, an endpoint). The structure is checked first, with the node in it; then the attribute goes on everything that is generated for that node. An input and a part take doc comments and no other attribute ([message 14](../reference/messages.md)): a `cfg` on one input would have to remove every line that uses it.

### Step 3: resolve

For each endpoint the macro walks from the client down to it and collects:

| What | Rule | Kept with it |
|---|---|---|
| A part named in a signature (`..page`) | Its inputs come after the node's own; its statements come before the node's own, as if the node had written them. A name, a key or a one-value statement that arrives twice this way is rejected ([message 38](../reference/messages.md)), and so is a part named twice. The `///` lines of the part's inputs come with them; the `///` lines of the part itself go nowhere. | The part, as the writer of those lines. |
| `base` | The nearest one, which may be the endpoint's own. If none applies, the description is rejected. | The node that wrote it. |
| `path` | Every one, root first. | The node, for each segment. |
| An entry of `query` or `headers` | By key (a header name ignoring case). A key set above can be set again only with `override`, and removed with `remove`. Within one node a key is written once. A key keeps the position where it was first written. **An entry, or a segment of a `path`, that is inherited under another `base` goes through one more check** ([the messages that are `rustc`'s](#the-messages-that-are-rustcs)): a value that holds a `Secret` stays with the `base` where it was written ([rule 16](../reference/rules.md#behaviour)). So the macro wraps an inherited value in the check when a `base` applied at the writer and one more is written below it on the way down, or when none applied there and two are written below. For a segment of a `path`, the wording of the error says to write the `path` under each `base`. A short entry in `headers` whose name contains `_` is rejected, and so is the key `host`. | The node. |
| `read` | The nearest one. One is needed only where a body is read: by the simple rule, or by an arm that binds `body` ([message 33](../reference/messages.md)). | The node. |
| An arm | By the statuses it matches: a class arm from above matches every status of its class. An exact status under an inherited class arm needs `override`; `override 2xx` over inherited exact arms of that class is rejected until each of them is overridden or removed. An exact status is matched before a class; in one node, a class and an exact status of it may both be written. `remove 4xx` removes the arm written `4xx` and nothing else. For a `HEAD`, the edits are resolved first, then an inherited arm that binds `body` is left out. An endpoint with no arrow is rejected when an arm that applies to it would read a body. | The node. |
| `problem`, and its reader | The client's. | |
| `body`, `paginate`, `idempotent` | The endpoint's own (or its part's). A `body` of entries in a part and one in the node, with the same function, are merged, the part's entries first. | |
| `idle_timeout` | The nearest one (the endpoint's, a scope's, the client's, or a part's). | The node that wrote it. |
| `middleware` | The client's, every one, in the order written. They are not part of an endpoint's list: `new` makes them ([what the macro generates](#what-the-macro-generates)). An optional input of the client named in one is rejected by the macro (it looks for the name among the tokens), so that Rust cannot resolve that name to another item. | The client. |

"The node that wrote it" matters for one reason: [the author's names in the author's Rust](#the-authors-names-in-the-authors-rust).

## Where the generated items live, and how they reach the author's types

A description is written in some module of the author's, where the author's names mean something: `Puuid`, `RiotProblem`, `Json`. The generated endpoint types live elsewhere: at their tree paths, in modules that the macro creates (`riot::matches::Ids`). The client's module holds one `Call<E>` and one `Endpoint` (`riot::Call<riot::matches::Ids>` is the type of the value that `Matches::ids` returns). Inside those modules, `Puuid` means nothing. This is the contract that joins the two.

| What is generated | Where | Why |
|---|---|---|
| The client type, the modules of the tree, the handle and endpoint types (their `struct` and `enum` lines), and `Call` and `Endpoint` in the client's module | In the module of the description, and in modules below it | So that `riot::matches::Ids` is a path. |
| The types of inputs and parent handles, and the problem type | Implementations of hidden Concord traits for `Call<ClientKey>`, in the module of the description | The authored type resolves where it was written. Each client's `Call<ClientKey>` is distinct, including for a description inside a function. Generated fields reach the type through the trait's associated type. |
| The `impl` blocks that use the author's Rust: the client and handle methods, both forms of an endpoint setter (including the associated function on the endpoint type, `Ids::count`), `__build_draft`, `__read_answer` and the pagers | In the module of the description, inside one `const _: () = { … };` | Every expression and every type of the author is copied there, and resolves as the author wrote it. The author's `use` lines are in scope. |
| `with_timeout`, `with_idle_timeout`, `request`, `into_request`, `edit_request`, `read_response` and `with_retry` without a `problem` line. Also the trait implementations for `Clone`, `Debug` and `concord::page::Call`. | In the client's module | They use the call, its options and the client's aliases without naming the author's types there. |
| `send`, `send_full`, `send_raw`, `send_if_none_match`, `pages`, `IntoFuture` and `with_retry` with a `problem` line | Inside the `const _: () = { … };` where the macro is invoked | The signatures use the authored problem type. If a name in a signature could take one of its names, the send methods use an alias scoped to their own block. |
| The views of [the author's names](#the-authors-names-in-the-authors-rust) and named storage for calls with more than twelve inputs | Inside anonymous constants, reached through associated types on `Call<ClientKey>` | The types have no name that a glob from a client or scope module can import. |
| The slots of `paginate` | A hidden type from the core, identified by the call type, the input's position, and the client's `Call<ClientKey>` | A slot is tied to one endpoint and input. |

The macro writes authored types where the description is written. A glob import from a client or scope module imports its documented items. Rust items cannot name a generic parameter of an enclosing function (E0401). Concord also rejects `Self` in a description ([message 10](../reference/messages.md)), because it would mean the type of the generated implementation.

**An input type must be at least as visible as the client.** Rust rejects a private type on a `pub client` (E0446). The rendered `riot::Error` and `riot::Retry` aliases show the projection through `ClientProblem`. The text under each names the authored type, with a link.

Paths: a path into `concord` or `std` starts with `::`. A path to a generated item is relative to the module of the description (`riot::matches::Ids`). So an endpoint may be named `copy` or `send_all`: its type `Copy` hides nothing that the macro generates.

## The author's names in the author's Rust

Rule 4 of [the rules](../reference/rules.md): a bare name is an input of the node that wrote the statement, and an input of a node above is named with its owner (`client.key`, `matches.route`).

The macro does not rewrite expressions. **It binds names, and then copies the expression as it is.** Before an expression that a node `N` wrote, the generated code has one local for each owner in scope, and one for each input of `N`. Every one of them is a reference.

```rust
// For an entry written by the client and inherited by matches.ids.
{
    let __entry = {
        let client = <riot::Call<ClientKey> as OwnerView<0>>::View::new((&__value.__parent.__parent.key,));
        let key = &__value.__parent.__parent.key;
        evaluate_entry("riot.matches.ids", "x-riot-token", || Ok({ key }))?
    };
    // The other header expressions run here.
    Draft::header_value(
        &mut __draft, "x-riot-token", "x-riot-token",
        AdaptPlacement::adapt_placement(&&__entry.0),
    );
}
```

(The paths into `::concord::__private` are shortened here.)

**An owner is a view.** The macro generates one hidden struct per client or scope. Its fields borrow the inputs of that level. A level with no inputs has an empty view. The generated code finds the view through the level's position in the tree, starting with 0 for the client. So `client.key` is a `&Secret`, exactly as the bare `key` is in the client's own lines. With a plain borrow of the handle in its place, `basic(client.key, )` would require another borrow.

**Every value goes through one adapter.** The macro calls `::concord::__private::AdaptPlacement::adapt_placement` with a reference to the value. Rust infers the writer from the value's type. The helper passes a serializable value to Serde and marks a secret value as sensitive.

| Where the expression appears | What the name of an input is bound to |
|---|---|
| A template, an entry, `body`, `read`, `idempotent` | A reference to the input. For an optional input (`count?: u32`) that is a `&Option<u32>`. |
| `paginate` | The input itself, as a slot: a `let` of a zero-sized value that is `Copy` and implements `concord::page::Slot` (get on a call, set on the next, and `input`: `InputId::new(n)`, with `n` the input's position among the inputs of the endpoint). An owner (`client.x`) is still a reference. What the expression returns is a pager, and must own what it holds, so a pager written in place as a closure is a `move` closure; without the keyword, `rustc` says to add it. |
| An arm | A reference to the input. Then the names of the arm's own pattern, which may shadow an input (that is Rust's shadowing; a rename, `body: task`, keeps both). |

`client.key` is then plain Rust: a field of a local variable. Because the bindings are references, the core accepts references where it accepts values: `Reader<T>` is implemented for `&R` (without it, `read r;` with an input `r` would not compile).

**An input shadows an item with its name.** In the lines of a node, `form` is the input `form` if the node has one, and the function `form` must then be written with its path (`concord::form(form)`). That is Rust's own rule. **One place is not an expression:** in `body f { … }`, `f` is a path to a function, and the macro calls it outside the block that binds the inputs, so an input named `f` does not shadow it (and the same spelling in a part and in the node is the same function). This is why the shipped pagers are named `by_offset`, `by_cursor`, `by_page`: `offset`, `cursor` and `page` are what APIs call those inputs. And a bare name that is not an input of the node is whatever Rust resolves it to there.

**The names of owners are kept free.** An input named `client`, or named like a scope above it, would shadow that owner. It is rejected with [message 7](../reference/messages.md), at both places.

**Hygiene.** The bindings that Concord provides to the author (the inputs, the owners, and in an arm `body`, `headers` and `status`) carry the span of the author's own identifier, so the author's expression sees them. **Every other binding that the macro generates in a function where an expression of the author's appears has mixed-site hygiene** (`Span::mixed_site()`: `__draft`, `__response`, `__status`, `__headers`, `__body`, `__info`, `__value`, …): the author's expression cannot read it, change it, or collide with it. **One thing hygiene does not do:** Rust reads `let __value = …` as a pattern when a `const` or a `static` named `__value` is in scope, and no hygiene on stable Rust prevents it (E0530). Rust applies the same constant-pattern rule to a binding for an input (E0530; [rule 4](../reference/rules.md)). Bindings that an expression does not use are allowed (`#[allow(unused_variables)]` on the block).

**What the macro reads in an expression:** only whether the identifier of an input appears among its tokens, for two checks: "an input is never used" ([message 9](../reference/messages.md)), and an optional input in a `middleware` expression. A field or a closure parameter that happens to have that name counts as a use, which is harmless. Inside a macro call, a name in a string counts too (`format!("{n}")` reads `n`). "An input of a node above needs its owner's name" ([message 12](../reference/messages.md)) is reported only where the macro reads the reference itself: in a template placeholder, in a `name;` entry, in a `base`. Inside an expression, a name that does not exist gets `rustc`'s message.

**How each expression is generated.** When each expression runs, and what `?` does in it, is a fact for the author, documented in [when an expression runs](../reference/grammar.md#when-an-expression-runs), in the grammar. This is what the macro generates around each one.

| Expression | What the macro generates around it |
|---|---|
| An entry of `query` or `headers`, `body` | A closure that returns `Result<_, Box<dyn Error + Send + Sync>>`, run when the request is built. An `Err` becomes the Build error: for a `body`, through the hidden helper `build_error`; for an entry, through `Draft::entry_error(endpoint, name, error)`. |
| An entry of a `body` | The same. The values go into one `concord::body::Entries`, which asks `serde` whether a value is a `None`, so the macro knows no type. A `Secret` goes through the adapter used for every placed value. |
| `idempotent` | A closure that is not `async`, run once per send, after `idle_timeout`. The hidden helper `idempotent(…)` takes what it returns, a `bool` or a `&bool`. `idempotent;` alone is `Idempotent::Yes`, and no `idempotent` is `Idempotent::ByMethod`. |
| `read`, and the reader of `problem` | A closure that is not `async`, run at most once per response, before the engine decides what it reads. The hidden helper `reader::<T, _>(…)` takes what it returns. |
| An arm | A closure whose parameters are the fields of its pattern, in their order: `headers: &Headers` and `status: StatusCode` are typed, `body` is not. Its expression is copied as `Ok({ EXPR })`. |
| `paginate` | The expression, with each input bound as its slot ([the table above](#the-authors-names-in-the-authors-rust)), in a closure run once, at `.pages()`, and handed to one of four entries, chosen by the form of the expression alone (no name is interpreted). **An expression certain to need no inference** goes to `pages_of_pager`, whose bound is `Pager<C>`. The rule, word for word as `certain_pager` states it in `concord-macros/src/write/call.rs`: "The pager path is taken only when the expression is certain to need no inference: after removing parentheses, it is a path, a call or a method call; or a plain block, an `if`/`else` or a `match` whose every value position is such a form again; and nowhere in the whole expression is there a closure literal or a macro invocation in a value position, a `return`, a `break` with a value, a loop, a label, or a `#[cfg]` or `#[cfg_attr]` attribute; and, at any depth (nested closures, `async` blocks and blocks included), no attribute of any kind, no macro invocation of any kind (also in a pattern or a type) and no item. Everything else takes the path of earlier releases, unchanged. No name is interpreted." The `return`s, `?`s, loops and labels of a nested closure or `async` block are its own. What [C8.6](../reference/contracts.md#c8-secrets-and-pages) promises an author is the form of this rule that can be read without the visitor. **A closure at the top** goes through a small generated function whose `FnMut` bound infers its parameters, then `pages_with_pager`, as in earlier releases. **A closure at another value position** (a tail of a block, a branch, an arm) goes to `pages_with_pager`, whose `FnMut` bound infers it. **Anything else** goes to `pages_inferred`: the same `FnMut` bound, through the hidden trait `InferredPager`, which only words the error for a value that is not a closure. |
| `idle_timeout` | A closure that is not `async` and returns a `std::time::Duration`, run once per send, before anything else. |
| `middleware` | The expression, in `new`, where only the client's required inputs are bound (an optional input is set after `new`). The macro rejects an optional input of the client there ([message 40](../reference/messages.md)); a name that does not exist gets `rustc`'s error. |

**`return` in a closure.** In `read { … }`, `idempotent { … }` and a pager, the macro applies its helper (`reader`, `idempotent`) to what the closure returns, outside it: so `return v` makes `v` the value of that expression. In a status arm and in the expression of `body`, the closure returns a `Result`: so `return` returns the same kind of value that `?` would.

**Two orders.** For each send (each page is one), `idle_timeout` runs first, then `idempotent`, then the request. The expressions of a request run by kind first (the base, the paths, the query, the headers, the body), and within a kind in tree order: the client first, and for a node the lines of its parts before its own. The entries go on the wire in the order in which each key was first written. The two orders differ after an `override` (it runs at the turn of the node that wrote it), so the generated code runs every expression of a kind first and writes its entries afterwards.

**An arm, exactly.** Its pattern holds `body`, `headers`, `status`, each at most once, each optionally renamed by the author. The expression is copied as `Ok({ EXPR })` into a function that returns `Result<A, concord::Fail<P>>` ([C5.3](../reference/contracts.md)). `body` is the read value, owned. Rust infers its type from the arm's expression, and it need not be the type after the arrow. `headers` is a `&concord::Headers`. `status` is an `http::StatusCode`. **The simple rule and the arrow.** For an arrow spelled `Option<T>` or `Result<T, E>`, a 2xx that no arm matches is read as `T` and wrapped (`Some`, `Ok`); the macro extracts `T` from the tokens (also for a path and for a generic `T`; behind an alias the whole type is read). When Rust cannot infer the type of `body` from an arm, the arm states it: `4xx { body } => { let odd: UploadError = body; Err(odd.into())? }`. A `HEAD` with an arrow has nothing to read: it needs at least one arm for a 2xx ([message 32](../reference/messages.md)), and for a 2xx that no arm matches, the generated code calls the hidden helper `no_arm(response, &info)`: a Mismatch that keeps the evidence, as an error response does. **How the status is matched:** `__read_answer` is one `match` on the status, with a guard for each arm that applies, exact statuses first (`_ if __status.as_u16() == 404`, and for a class `_ if __status.as_u16() / 100 == 4`); then the simple rule for a 2xx (for a `HEAD`, `no_arm`); then `refused` (or `refused_plain` with no `problem` line) for the rest.

## What the macro generates

For a client `Riot`, as the macro generates it from [`tests/tour-gen/src/riot.rs`](../../tests/tour-gen/src/riot.rs) (and as [`tests/tour/src/riot.rs`](../../tests/tour/src/riot.rs) writes it by hand):

| For | Items |
|---|---|
| The client | `pub struct Riot`; `Riot::new(…)`, which also makes each middleware of the description, in order, and hands it to the core; the settings (`with_retry`, `with_timeout`, `with_idle_timeout`, `with_base`, `with_transport`, `with_middleware`); one method for each child; a setter for each optional input of the client; a `Debug` that prints no input. The module `riot` with `riot::Call<E>`, the sealed `riot::Endpoint` trait, `riot::Error`, `riot::Result<T>` and `riot::Retry` (`concord::Retry<RiotProblem>`). |
| A scope | A handle type at its tree path (`riot::Matches`), with one method for each child, a setter for each optional input of the scope, and a `Debug`. The hidden view of [the author's names](#the-authors-names-in-the-authors-rust). |
| A part | Nothing. |
| An endpoint | The method on its parent, with the generated documentation. It returns the client's `Call<E>`, where `E` is the endpoint's type. The endpoint type at its tree path (`riot::matches::Ids`), with no value of its own. A setter on its call for each optional input, also available as an associated function on the endpoint type. `NAME`, its name (C6.4), the one public constant of its `Endpoint` implementation. `__build_draft` (the flat list of step 3, in order, on a `Draft`) in the endpoint's `Endpoint` implementation, with `__build_request(take)` on the client's `Call<E>`. One hidden function for the response, `__read_answer` (the arms, the reader, the problem), used by a send and by `read_response`. **`__prepare_inner`**, on `Call<E>`, the first half of a send: the `Run`, `idle_timeout`, `idempotent`, then `__build_draft` and `Draft::prepare`; it returns a `Prepared`, whose `send` is the second half. One set of shared methods on the client's `Call<E>` ([the hidden interface](engine.md#the-hidden-interface-method-by-method), on the engine page, has the table: every public method of a call, and what it is built on; `answered_from(url)`, which the pages state machine calls, is there too). For `GET` or `HEAD`, `__prepare_if_none_match(etag)` prepares a conditional request with `If-None-Match` set after the description's headers and before edits, and returns a `Prepared` that reads a `304` that no arm matches as `None` (C5.12). With `paginate`: `pages()`, and a separate slot for each input (`.items()` is a method of `Pages`, not of the call). A `Debug` for the call that names the endpoint and prints no input. |

**Generated code reaches its helpers through qualified paths.** A glob from a client or scope module leaves the helpers out. The author's unqualified names keep their ordinary meaning inside types and expressions. The stream problem check uses method syntax in its own module, which imports only the two Concord traits that choose the check.

**A call to a paginated endpoint has no `IntoFuture`.** `.send()` returns one page, `.pages()` yields them all, and `.pages().items()` each item. The code for an endpoint with `paginate` is required to supply its pager. The mistake `call.await` gets `rustc`'s own words (`is not a future`). The `#[must_use]` text appears only when a call is left unused ([C8.8](../reference/contracts.md#c8-secrets-and-pages)). The `Call` type itself also has `#[must_use]`. A call whose method is `GET` or `HEAD` also gets `send_if_none_match(etag)`, which uses the same preparation as `__prepare_if_none_match(etag)` and then calls `Prepared::send`.

**Attributes.** `#[cfg]` on a node goes on everything that depends on the node, including the associated types of the hidden trait. A use of a deprecated item inside generated code does not warn (`#[allow(deprecated)]` there); the caller still gets the warning. A relative visibility (`pub(super)`) is rewritten for each module depth, so that it means what the author wrote. `#[doc(hidden)]` and `#[deprecated]` on an endpoint go on what a user can name. That includes its type, the method that returns its call and both forms of each setter. `#[cfg]` also goes on its hidden items ([C1.12](../reference/contracts.md#c1-the-description)).

**The order of `__build_draft` is fixed** (the two orders of [the author's names](#the-authors-names-in-the-authors-rust)), and an expression that fails ends the building there. An author's expression may have side effects, so the order is part of the contract: [C10.3](../reference/contracts.md).

**The flat list is written out in each endpoint.** A header set on the client is one line in the `__build_draft` of every endpoint below. An authored type or expression can appear more than once in the generated code of one endpoint, so a Rust macro in that position must expand the same way each time. Rust infers the reader separately for each status arm that reads a body and for the fallback. This is the simple shape: an endpoint can be read on its own, and `override` and `remove` cost nothing at run time. The price is size: see [size and compile time](#size-and-compile-time).

**Reserved names.** A name that becomes a method (a child, or an optional input) cannot be a method that its receiver already has (on a call, that includes `at_url`: pagers use it), and cannot start with `with_`. Concord adds no name to a call, a handle or a client in a minor version unless it starts with `with_`. The one table is [C1.6](../reference/contracts.md). A required input is an argument; it creates no method and may have any name. [Message 3](../reference/messages.md) says so.

## Where a message points

The wording and the place of each message are in [the messages](../reference/messages.md). This is how the macro produces them on stable Rust.

- **What is fixed.** Of the three lines of each of the 46 messages, the first (the message) and the place it points at are those of the messages page, word for word, and the code follows them (for [messages 18 and 20](../reference/messages.md) the place is the keyword). The label and the help are what the macro prints for the case: a help is in words where no exact replacement is safe (one of the three rules for every message, on the same page). The page copies them from the compiler fixtures. A test of contract C1.18 (its name starts with `c1_18_`) runs each case of the page through the whole macro and compares the three lines.
- A message is a `compile_error!` at a span. `syn::Error::new_spanned(token, text)` creates it; errors are joined with `combine`, so that one build shows them all.
- **One message, three lines.** Stable Rust gives a macro no labels and no help lines. The three parts of a message on the page are the lines of one text: the error, then what the place is, then what to write.
- **Two places.** For a clash ("already declared", "is already set by an ancestor") the macro emits a second error at the earlier place, with a text that says what is there: `the first child is here`, `the inherited header is set here`, ``part `page` declares it here``.
- **A name suggested by [message 35](../reference/messages.md)** is the one part of the client nearest to the written name by edit distance, when no other part is as near, and when it is at most a third of the written name's length away (at least 1 edit, at most 3). Otherwise the help names no part.
- **Inside a string, the whole string.** Stable Rust gives no span inside a literal. A message about a placeholder points at its template, and names the placeholder in words.
- When `proc_macro::Diagnostic` is stable, the same texts will become real labels. Nothing else changes.

## The messages that are `rustc`'s

Everything inside the author's Rust is checked by `rustc`, and the message is its own: a name that does not exist (`read Jsno;`), a type that does not fit. **One mistake, one place, perhaps several errors.** For a mistake that only `rustc` can see (an author's type that lacks a bound), the first error points at the author's token. `rustc` may report the same missing bound several times, once for each generated item that needs it (see the compiler fixture for `problem Rc<String>`). The macro's own 46 messages are each reported once. Three things make those messages good.

1. **The author's tokens keep their spans.** The error appears under what the author wrote, not under `api!`.
2. **Each expression goes through one function whose bound says what is wanted, and the call of that function has the span of the expression.** `read X;` becomes `::concord::__private::reader::<T, _>(X)`, a function with the bound `R: Reader<T>` that returns a small wrapper which carries `T` (so Rust infers the type that is read, also for the problem type, and `?` cannot be used on what it returns). (When only `X` has the author's span, the error covers the whole `api!`; when the call has it too, the error is at `X`.)
3. **The core's traits say it in Concord's words**, with `#[diagnostic::on_unimplemented]` (Rust 1.78). The words name the type, not the expression: the compiler provides no more.

| Trait | When it is not met, at the author's expression |
|---|---|
| `Reader<T>` | `` `BadReader` cannot read a `Vec<MatchId>` `` · note: `` the value of `read` must be a reader of the endpoint's type: one of `concord::read`, or a type that implements `concord::Reader<Vec<MatchId>>` `` |
| `error::CarriesProblem`, and the sealed `__private::SameProblem<P>` | `` the endpoint's type names the problem type `Other`, and the client's problem type is `Problem` ``, at the declared type, with the note `` a typed stream fails with the client's own error: … ``. `SameProblem` is implemented for each type with itself only, and is sealed: no author can implement it, so no implementation admits another problem type; the check requires it of `CarriesProblem::Problem`. Every endpoint has the check of its declared type in a function that is never called, in a module of its own where none of the author's names is in scope, inside an anonymous constant (`const _: () = { … };`), so that no name of the check can be written or met where the description is, and an application's method of the same name never runs nor makes a call ambiguous; the type is the endpoint's own `Value`, as the compiler resolved it (the author's tokens are written once), and it reaches the check, with the problem type, as associated types of a witness: `(&Output::<T>::new()).must_name_the_clients_problem_type::<P>()`: a type that implements `CarriesProblem` takes the path whose only bound is `CarriesProblem::Problem: SameProblem<P>`; every other type takes the path that checks nothing |
| `IntoBody` | `` `Task` is not a body `` · note: `` a body expression returns a `concord::Body` or a `Result` of one, as `json(x)`, `form(x)` or your own function do `` |
| `__private::AdaptPlacement` | A value that is neither a supported secret nor `serde::Serialize` gets Rust's trait-bound error at the authored expression. |
| `page::Slot<C>` | `` `u32` is not an input of this endpoint `` · note: `` in `paginate`, pass the endpoint's inputs to the pager: `paginate by_offset(start);` `` |
| `page::IntoNext<C>` | `` a pager returns the next call: `Some(call)`, `None`, or a `Result` of either `` |
| `page::Pager<C>` | `` `u32` is not a pager for this endpoint `` · note: `` a pager is a closure `\|call, page\| …` that returns the next call, or a value such as `by_cursor(…)` `` (an expression taken as any `Pager`) |
| `__private::InferredPager<C, N>` | `` `paginate` reads this expression as a closure, and its value is `Paging<api::Call<List>>`, which is not a closure `` · label: `a closure is needed here` · two notes: `` if the value is a pager, write `.into_fn()` after a `Paging<…>` (`by_cursor(…).into_fn()`) or `Paging::from_pager(…).into_fn()` around a pager of yours; or write the pager as a call or a path, alone or at each tail of a plain block, `if` or `match` `` and `` if it is not, it is neither a closure nor a pager, and `paginate` cannot use it `` (an expression read as a closure, with no closure at a value position). The attribute cannot choose a note by the type of the value, so each note says for whom it is. |
| `Transport` | `` `X` cannot send a request `` · note: `` a `reqwest::Client`, `concord::Http`, a test transport, or a type that implements `concord::Transport` `` |
| `Middleware` | `` `X` is not a middleware `` · note: `` implement `concord::Middleware` with `handle(request, info, next)`; `next.send(request, info)` passes the request on `` |
| `page::IntoItems` | `` `Option<Vec<Task>>` is not a list of items `` · note: `` implement `concord::page::IntoItems` for your page type `` |
| `__private::InheritAcrossBase` | `a secret does not follow a change of base` · note: `` declare this entry again with `override`, or remove it, under the new `base` `` (at the author's entry) |
| `__private::InheritPathAcrossBase` | `a secret in a path does not follow a change of base` · note: `` declare this `path` under each `base` that needs it, not on an ancestor of those bases `` (the same gate, for a piece of a `path`: a `path` cannot be written again or removed) |

The macro still knows none of these items. The words belong to the traits.

## Size and compile time

Measured through the macro itself, with Rust 1.99, offline, with four jobs and the dependencies already built. Memory is the peak of one process.

| Shape | First `cargo check` | Peak memory |
|---|---:|---:|
| 500 endpoints | 3.67 s | 0.39 GiB |

- **Wide inputs share their bindings.** The facts of one origin share one set of bindings, so an entry does not repeat the bindings of every input.

The measurements above cover descriptions of up to 500 endpoints.

To measure the expansion yourself, run `just bench --expansion tests/tour-gen/src/tasks.rs` (that file is the default); it writes `target/expansion-measurement.json`. Pass another Rust file with a description to measure that client.

## The generated documentation

The documentation that the macro generates follows these rules. The macro can write each line from the description alone. The text of the shared call methods is in [`call_docs.rs`](../../concord-macros/src/write/call_docs.rs).

1. An endpoint's method documents, in this order and only when it applies: the author's lines; the shape of the call; the request line; the names of the headers that are written for it (names, never values); each input with what it accepts; each option; one line that names the statuses with a meaning of their own here (`404`, `2xx`), without the arm (what each means is for the author to say, in the lines above); a line when the endpoint may not be repeated; what `.await` returns (for a paged endpoint, what `.send().await` returns: one page); pages, with one sentence: `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item. The method links to the endpoint type's page. That page starts with the author's lines, then shows the request line. Under `Call type:` it shows the full call type and the method that returns it (`riot::Call<riot::matches::Ids>` and `Matches::ids`). It shows the inputs and the option signatures, says what a send returns in the same words as the method, and links to `Call` for the shared methods.
2. **A type named in the result text is a link where the macro can link to its page, written as a caller writes it:** `riot::Result<Vec<MatchId>>`, never `crate::riot::Result<Vec<crate::MatchId>>`. The type of the call and the method that returns it are shown as code that can be copied. An author's type that has the name of a handle is written with its path.
3. **A client with no `problem` line shows no `()`:** `Retry`, `Sending<Vec<Task>>`, `concord::Error`.
4. **A setter says what it accepts:** "Accepts a `u32`, a `&u32`, or an `Option<u32>`. `None` clears it.", with the type as the author spelled it and a link to `concord::arg::Arg` for text and lists. The macro does not guess what an alias stands for. An endpoint's setter starts with its endpoint ("Option `count` of `matches::ids`."). The endpoint type's associated function carries the documentation. The first argument is the call. The same option is an inherent method on its call.
5. **The author's example is on the client**, as written in the `///` lines above `client`. The macro writes no example of its own, because it has no real values.
6. What a caller never calls is hidden, including the blanket implementations of `concord::arg::Arg`, the traits that pagers use and the slot types. Signatures show the author's types. The editor may show a local alias when a problem type could meet a name in a signature. If that type is written with a macro, the alias refers to the type resolved outside its scope, so rustdoc shows a projection through `ClientProblem`. The client's `Error` and `Retry` aliases also show that projection, and their text names the authored problem type. `Endpoint` documents only `Value` and `NAME`, and `Call` documents the shared methods with plain parameter names and their bounds. The endpoint's setters appear on its type in rustdoc and on the call in the editor. The prose beside the `IntoFuture` implementation names its output `riot::Result<E::Value>`.
7. The words are the caller's. Generated text never says "the description" or "the arrow".
8. The inputs of a part are shown like the node's own. The name of the part is not shown.
9. The page of a client says that every request goes through a middleware, with the `///` lines that the author wrote above each `middleware`. The text of the expression is never printed: it could contain a literal key.
10. A paged endpoint says: `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.

## The editor

Editor support inside `api! { }` has not been verified: what `rust-analyzer` does there is not known.
What the macro does for an editor: the author's tokens keep their spans, the names of [the author's names](#the-authors-names-in-the-authors-rust) are real `let` bindings, and the macro generates what it can when some of the description is wrong ([the steps](#the-steps)).
Not known: whether hover, "go to definition" and completion (of a name, or of the statement words) work inside the braces. In code that calls the client, the editor offers the setters of the endpoint. It offers `pages()` and `send_if_none_match(etag)` only on calls that meet their bounds.
