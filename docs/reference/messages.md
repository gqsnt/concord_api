# The messages: what Concord reports when it rejects a description or a call fails

*[Concord](../../README.md) · [docs](../README.md) · reference · previous: [the rules](rules.md) · next: [the crates](crates.md)*

This page is for you when Concord rejected your description, or a call failed: find the message, learn what it means, and see what to write instead.

To find a message, search this page for a few words of the text that you got. At compile time, a message starts with `error:`; here it has a number, **1** to **46**, and is listed under its theme ([at compile time](#1-at-compile-time)). At run time, a text starts with its kind (`build error:`, `transport error:`, `refused:` or `mismatch:`) and names the endpoint (`in tasks.get`); the tables are grouped by kind ([when a request is built or answered](#2-when-a-request-is-built-or-answered)).

When Concord rejects a description, the message points at your own token, says what is wrong, and says what to write. There are 46 messages at compile time, and 52 texts at run time. The errors of `concord-extras`, and the causes of a failed repeatable body, add 55 texts of their own, which appear in the chain of `source()` or are returned directly ([the errors of `concord-extras`](#2-when-a-request-is-built-or-answered)).

Some mistakes get no message from this page. A mistake inside your own Rust (`read Jsno;`, a type that does not fit, a struct that holds a `Secret` in `json(x)`, a generic parameter of an enclosing function in a type (E0401: `api!` generates items), `?` in `paginate`, `idempotent`, `idle_timeout` or `middleware`) is reported by `rustc`, in words that the traits of the core provide where they can (the macro, [The messages that are `rustc`'s](../internals/macro.md#the-messages-that-are-rustcs)). A line that is outside the grammar and that no message below names (`body client.f { … }`: the word before the block is a path) gets the parser's own message, at the token where it stopped (`expected ;`).

`rustc` reports three more kinds of mistake. The first uses Concord's words, and the other two use the compiler's own.

- **A secret that would follow a change of `base`** ([rule 16](rules.md)): `a secret does not follow a change of base`. At an entry, the note is `` declare this entry again with `override`, or remove it, under the new `base` ``. At a placeholder in a `path` (a `path` cannot be written again or removed), the message is `a secret in a path does not follow a change of base`, and its note is `` declare this `path` under each `base` that needs it, not on an ancestor of those bases ``.
- **A call that does not meet an endpoint's rule.**

  Awaiting a call to an endpoint with `paginate` ([rule 17](rules.md)) gives this error. Use `call.send().await` for one page or `call.pages()` for all pages.

  ```text
  error[E0277]: `api::Call<List>` is not a future
  help: remove the `.await`
  ```

  Calling `pages()` on an endpoint without `paginate` gives this error. Use `call.await` or `call.send().await` to read the response.

  ```text
  error[E0599]: the method `pages` exists for struct `api::Call<Get>`, but its trait bounds were not satisfied
  note: trait bound `concord::call::NoPaginate: concord::call::DeclaresPaginate` was not satisfied
  ```

  Calling `send_if_none_match(etag)` on a `POST` gives this error. Use `call.send().await` to send it. For a conditional read, use `send_if_none_match(etag).await` on a `GET` or `HEAD` call. Other methods that cannot be sent conditionally show `NotGetOrHead` in place of `Post`.

  ```text
  error[E0599]: the method `send_if_none_match` exists for struct `api::Call<Create>`, but its trait bounds were not satisfied
  note: trait bound `concord::call::Post: concord::call::IsGetOrHead` was not satisfied
  ```

  In these examples, both E0599 notes point at the client's name in `pub client Api`. When you call the client from another crate, the errors still name the endpoint's call type, and the E0599 errors still name the unsatisfied bound.

  The `#[must_use]` text appears only when a call is left unused. The method that returns a call and each setter of its endpoint carry the endpoint's text. Shared methods that return a call use the general text on `Call` ([C8.8](contracts.md#c8-secrets-and-pages)).

- **A pager that borrows an input.** The pager must own what it keeps. Copy or clone the needed value before the `move` closure. If the pager only needs `client.root.len()`, compute the length before the closure and move that into it.

  ```text
  error[E0716]: temporary value dropped while borrowed
  note: requirement that the value outlives `'static` introduced here
  ```

## 1. At compile time

Each message below is given with the case that causes it, the token that it points at (**Point**), and its three lines as the macro prints them, word for word: the message, the label and the help. A help shows code only where it is safe to type as shown; elsewhere it says in words what to do ([the three rules](#3-three-rules-for-every-message)). How the macro emits these messages on stable Rust (one text of three lines, a second error at the earlier place, the whole string for a placeholder) is described in the macro, [Where a message points](../internals/macro.md#where-a-message-points).

### Names

**1.** Two siblings say `GET get();`.  
Point: second `get`.

```text
error: duplicate child name `get`
label: already declared in this block
help: give this endpoint a distinct name, or remove the duplicate declaration
```

The same for two parts with one name, and for an input written twice in one signature.

**2.** An optional input and a child both use `get`.  
Point: child’s `get`.

```text
error: two declarations generate method `get`
label: conflicts with the optional input
help: rename this child to `fetch_get`
```

**3.** An optional input is named `send`: a name that becomes a method, and that the call already has. The same for a child, for every reserved name of [rule 24](rules.md) (including `at_url`), and for a name that starts with `with_`. A required input is an argument and may have any name.  
Point: `send`.

```text
error: `send` is reserved by Concord
label: reserved for generated methods
help: rename this input to `send_value` and update its references in the existing `query` placements and expressions; keep the wire keys and fixed text that spell `send`
```

`timeout` and `retry` remain legal. Reservations depend on the receiver. A child named `endpoint` directly under the client is reserved for the generated endpoint trait. Rename it to `endpoint_item`.

**4.** An input uses `type` or `r#type`.  
Point: offending identifier.

```text
error: keywords and raw identifiers cannot name inputs
label: a Rust keyword or raw identifier
help: rename this input to `type_value` and update its references in the existing placements and expressions, retaining their wire keys
```

**5.** A scope, endpoint or input uses `get__item`.  
Point: `get__item`.

```text
error: invalid API name
label: expected lower snake case
help: rename it to `get_item` and update its references, retaining existing wire names
```

**6.** The client is `tasks`, `Self` or `Type`.  
Point: client name.

```text
error: invalid client name
label: expected upper camel case with a non-keyword module name
help: write `client Tasks`
```

**7.** A scope is named `client`, or repeats an ancestor’s name. Or an input has the name of an enclosing owner: `client`, or a scope above it.  
Point: offending name; the owner is labelled too.

```text
error: name conflicts with an input owner
label: the client or an enclosing scope already has this name
help: choose a distinct scope name and update every qualified reference to this scope's inputs
```

For an input, the help proposes a new name that is free at every node that receives the input (a part's input reaches each node that spreads the part, with its children), and the rule that keeps its key on the wire: a short entry sends the input's name as its key, so each short entry `client;`, if any, becomes `"client" = client_value;`, in every block that has one. Placeholders and expressions take the new name; a key that is already written stays as it is. When none of the names it tries is free, the help proposes none and still states the rule.

**8.** Two children of one level, `one2` and `one_2`, generate the same type `One2`.  
Point: `one_2`; `one2` is labelled too.

```text
error: `one2` and `one_2` both generate the type `One2`
label: generated names collide
help: rename `one_2` to `one_2_other`
```

### Inputs

**9.** `count: u32` is declared but unused.  
Point: declaration’s `count`.

```text
error: input `count` is never used
label: nothing in the description uses this input
help: use this input in the intended `path`, `base`, `query`, `headers`, `body`, or expression, or remove its declaration
```

A name inside a string of a macro call (`format!("{count}")`) counts as a use.

**10.** The signature declares `id: &str`.  
Point: `&str`.

```text
error: input types must own their data
label: borrowed input type
help: write `id: String`; callers may still pass references
```

A second case: `Self` in any type of a description (`-> Vec<Self>`).  
Point: `Self`.

```text
error: `Self` has no meaning in a description
label: `Self` is not the enclosing type here
help: replace `Self` with the path of the intended type
```

Concord cannot see a `Self` that the author's own macro produces inside an expression, and the generated type it names may change between versions, so do not rely on it.

**11.** `path "{idd}";` refers to an undeclared input.  
Point: literal containing `{idd}`.

```text
error: unknown input `idd`
label: no input with this name
help: correct this input name, or declare the intended input in this signature and use every declared input
```

The same for an input of a scope named inside a part: a part sees its own inputs and `client.x` only.

**12.** `{tenant}` refers to an input declared on the client.  
Point: literal containing `{tenant}`.

```text
error: ancestor inputs require their owner's name
label: `tenant` belongs to the client
help: write `{client.tenant}`
```

Reported only where the macro reads the name itself: a template placeholder, a `name;` entry, a `base`. Inside an expression, a name that does not exist is reported by `rustc`. For `client.x;` written as a short entry, the first line is `a short entry cannot name a qualified input`; the help then says `write "x" = client.x;`.

**13.** An optional input, `id?: String`, fills a placeholder in a path or a `base`.  
Point: offending placeholder; the `?` is labelled too.

```text
error: a URL input cannot be optional
label: an optional input cannot fill this placeholder
help: declare `id: String`, without `?`
```

The same for `base x;` when `x` is optional.

**14.** An input carries `#[serde(rename = "id")]`.  
Point: attribute.

```text
error: unsupported attribute on an input
label: an input takes doc comments only
help: remove this attribute; retain its intended wire name ("id") in the existing placement, updating that entry rather than adding another
```

The same message appears for every attribute that its place does not accept: an input and a part accept doc comments only; `#[cfg]` is for a client, a scope or an endpoint; `#[deprecated]` and `#[doc(hidden)]` are for a scope or an endpoint. The first line names the place: `unsupported attribute on a part`, `on the client`, `on a scope`, `on an endpoint`. A statement accepts no attribute (only `middleware` accepts `///`): a `///` above `read` is ``unsupported attribute on `read` ``, with the word of the statement.

### Structure and placement

**15.** An endpoint starts with `list()` alone.  
Point: `list`.

```text
error: expected an HTTP method before the endpoint name
label: endpoint method is missing
help: write `GET list();`
```

**16.** `read` follows a child declaration.  
Point: `read`.

```text
error: statements must precede children
label: statement appears after a child
help: move `read Json;` before the first part or child
```

For a part written after a child, the first line is `parts must precede children`: a part is written before the first child. A statement written after a part in the client's block is message 16 too, with its own first line: `statements must precede parts`.

**17.** A node repeats a statement or block, including `body`.  
Point: second statement keyword.

```text
error: duplicate `body` statement
label: this node already declares `body`
help: keep one `body` declaration with its intended value
```

The message names the statement that is repeated. The same for a statement written twice inside one part.

**18.** A client or scope contains `body`, `paginate` or `idempotent`.  
Point: misplaced keyword.

```text
error: `paginate` requires an endpoint
label: `paginate` cannot be declared on a scope
help: move `paginate` to an endpoint, and combine it with any `paginate` declared there or supplied by a part
```

The message names the word and the kind of node. The same for a scope that names a part holding one of them, at the `..name`.

**19.** In `METHOD "PROP FIND" find();`, the string is not a method name.  
Point: the string.

```text
error: invalid method name
label: a method name is an HTTP token, with no space
help: write `METHOD "PROPFIND" find();`
```

**20.** A scope or endpoint declares `problem E;` or `middleware m;`.  
Point: `problem`.

```text
error: `problem` is allowed only on the client
label: problem types are declared on the client
help: keep one `problem` declaration, with the intended type, in the client block
```

The same for `middleware`, which also belongs on the client, and for a `part` declared anywhere but on the client. The message names the word.

### Inheritance

**21.** A child sets a header that is set above, without marking it.  
Point: the child’s entry.

```text
error: header `accept` is already set by an ancestor
label: replacement requires `override`
help: write `override "accept" = "…";`
```

The same for a query pair. For an arm, the first line is `` an inherited arm already matches `404` ``. An arm is compared by the statuses it matches: an exact `404` under an inherited `4xx` needs `override` too. (`base`, `read` and `idle_timeout` need no keyword: the nearest one is used.)

**22.** An `override` has no inherited target, including after a removal.  
Point: `override`.

```text
error: no inherited header `accept` to override
label: no inherited value for this key
help: write `"accept" = "…";`
```

**23.** `remove 404` has no inherited arm.  
Point: `remove`.

```text
error: no inherited status arm `404` to remove
label: this arm is absent
help: delete `remove 404`
```

The same for an entry of `query` or `headers`. In a body of entries, `override` and `remove` always produce messages 22 and 23: a body inherits nothing from a level above. The same message for `remove 404` when only a `4xx` is set above: `remove` removes the arm written with that key, and nothing else. To take `404` from the class arm, the help says to write an `override` arm for `404`.

**24.** A description says `override path "items";`.  
Point: `override`.

```text
error: `override` and `remove` cannot be used on `path`
label: each path adds segments
help: write `path "items";` and change the ancestor's path if needed
```

The same for `override` or `remove` before every other statement that is not an entry or an arm: `base`, `read`, `idle_timeout`, `body`, `paginate`, `idempotent`, `problem`, `middleware`, and before a whole block (`override headers { … }`): the two words appear inside a block, before an entry or an arm. The message names the statement. For a non-block statement such as `idempotent`, the help is ``declare `idempotent` with the intended value, without `override` or `remove`; change ancestor declarations if needed``.

### Paths and headers

**25.** `path "/a//b";` has a leading slash and an empty segment.  
Point: path literal.

```text
error: path segments must be nonempty
label: empty path or a leading or doubled slash
help: remove leading or repeated slashes while keeping the path's segments and placeholders; omit an empty path statement
```

A path may end with `/`.

**26.** `path "a/../b";` contains a dot segment.  
Point: path literal.

```text
error: dot segments are forbidden in paths
label: `..` is a dot segment
help: write the intended path without dot segments, preserving the other segments and placeholders
```

**27.** Fixed path text contains a space, `#` or `%`.  
Point: path literal.

```text
error: forbidden character in fixed path text
label: this character is not allowed in fixed text
help: move the forbidden fixed text into an input placeholder at this position; keep the other path text and placeholders
```

The same for a `\` and for an ASCII control character (a tab, a line end) in the fixed text of a `path`, after Rust has decoded the string: the URL would read the first as `/` and lose the second, with no message. In a `base`, the first line is `forbidden character in fixed base text`.

**28.** `path "items?count=5";` embeds a query.  
Point: path literal.

```text
error: a path cannot contain a query
label: query begins here
help: write `path "items"; query { "count" = "5"; }`
```

**29.** `path "files/{*key}.json";` puts text beside a `{*x}` placeholder.  
Point: path literal.

```text
error: a `{*…}` placeholder must be a whole path segment
label: `{*key}` shares its segment with `.json`
help: write `path "files/{key}.json";`; a `{key}` placeholder may share its segment
```

For a `{*x}` placeholder outside a path (in a `base`, or in a template of an entry), the first line is `` a `{*…}` placeholder is allowed only in `path` ``.

**30.** Headers contain `"bad header" = "v";`.  
Point: header-name literal.

```text
error: invalid header name
label: expected a nonempty ASCII HTTP token
help: choose the intended nonempty ASCII HTTP header name, preserving this entry's value or removal
```

### HEAD responses

**31.** A `HEAD` status arm binds a body: `2xx { body } => …`.  
Point: the word `body`.

```text
error: a HEAD response has no readable body
label: this arm binds `body`
help: remove the `body` binding and update this arm's expression so it no longer uses the response body
```

The same for an arm of a part that binds `body`, on a `HEAD` that names the part: it is the `HEAD`'s own arm.

**32.** `HEAD exists() -> bool;` has no arm for a 2xx.  
Point: `-> bool`.

```text
error: a HEAD endpoint with a return type requires a success status arm
label: no arm matches a 2xx status
help: add a `status` block with a success arm that returns the declared type without binding `body`
```

A `HEAD` has nothing to read ([rule 11](rules.md)). One arm for a 2xx is enough, an exact status or the class. At run time, a 2xx that no arm matches is then a Mismatch ([at run time](#2-when-a-request-is-built-or-answered)).

### Readers and arms

**33.** `GET list() -> Vec<Task>;` reads a body, with no `read` on the endpoint or above it.  
Point: what reads: `-> Vec<Task>` for the simple rule; the field `body` in an arm that binds it.

```text
error: no reader applies to this response body
label: no `read` is declared here or inherited
help: declare the intended reader with `read` on this endpoint or an ancestor
```

An endpoint that reads no body needs no reader: `HEAD exists() -> bool { status { 2xx => true, 404 => false } }`. The same message, pointing at the type, for `problem E;` on a client that has no `read` of its own and no `read` on that line.

**34.** An arm binds a field that does not exist: `200 { bodi } => …`.  
Point: the field.

```text
error: unknown field `bodi` in a status arm
label: expected `body`, `headers`, or `status`
help: write `200 { body: bodi } => bodi`
```

For a field written twice, as in `200 { body, body }`, the first line is ``duplicate field `body` in a status arm``. For two fields with the same binding, as in `200 { body: b, headers: b }`, it is ``duplicate binding `b` in a status arm``.

**A mapped reader whose end is not chosen**, `read Sse::new(n).try_map(event);`, and **a stream whose error names another problem type than the client's**, `-> RecordStream<String, concord::Error<Other>>` under `problem Problem;`, are refused by `rustc`, with words of Concord. The first points at the reader: ``error[E0277]: the reader made by `try_map` does not say how its stream ends``, with the label ``finish this reader with `.until_last_record()` or `.until_end_of_body()` `` and one note for each method (``.until_last_record()`: a record ends the stream (`Step::Last` or `Step::End`), and a body that ends before it is an error``; ``.until_end_of_body()`: the end of the body ends the stream``); it follows an error with the note of a reader that does not fit, at the same place. The second points at the declared type, also behind an alias or inside an `Option`, a `Result` or a `Box`: ``error[E0277]: the endpoint's type names the problem type `Other`, and the client's problem type is `Problem` ``, with the label ``this type's error names `Other` `` and the note ``a typed stream fails with the client's own error: write the client's `Error` (`RecordStream<Event, client::Error>`), and give the mapping the client's problem type (`RecordFail<Problem>`)``, then ``required by a bound in `TypedStream::must_name_the_clients_problem_type` ``. A mapping whose problem type differs from the stream's is the error of a reader that does not fit: `` `impl Reader<…>` cannot read a `RecordStream<…>` ``, with the note `` the value of `read` must be a reader of the endpoint's type: one of `concord::read`, or a type that implements `concord::Reader<…>` ``.

### Parts

**35.** A node names `..pages`, and the client declares `part page`.  
Point: `pages` after `..`.

```text
error: no part named `pages`
label: not declared in the client block
help: use `..page`, the closest declared part; check its inputs and statements first
```

**36.** The client declares `part keyed(…)`, and no node names `..keyed`.  
Point: the part’s name.

```text
error: part `keyed` is never used
label: no scope or endpoint names `..keyed`
help: apply this part to a compatible scope or endpoint, or remove its declaration
```

**37.** A part holds `path "v1";`.  
Point: `path`.

```text
error: a part cannot contain `path`
label: `path` is declared on the client, a scope, or an endpoint
help: move `path` to the node that needs it, and combine it with any `path` already there
```

The same for `problem`, `middleware`, a child, a `..name` in the signature of a part, and a `..name` on the client. The message names the word.

**38.** A part holds `paginate by_link();`, and an endpoint that names it writes its own `paginate`.  
Point: the endpoint’s `paginate`; the part’s is labelled too.

```text
error: `paginate` is declared both in part `page` and in this endpoint
label: the statements of a part count as this node's own
help: keep one declaration with the intended value or body function and every input placement; use compatible parts, or remove inputs that become unused
```

The same for `base`, `read`, `body`, `idempotent` and `idle_timeout`, and for a key (an entry, an arm) that the part writes and the node writes again: inside one node a key is written once, and `override` is for a key from a level above, not for a part. Two parts named by one node that write the same key or the same one-value statement: the same message, naming both parts. The same also for a key written twice in one block, for an input of a part with the name of an input of the node, and for a part named twice by one node (`..page, ..page`). A body of entries is the one exception: the entries of the parts (in the order in which the node names them) and then the node's own are merged when all call the same function, written the same way (a path, resolved from where `api!` is invoked: [grammar](grammar.md), sentence 18); another function, or the other form of `body`, produces this message, and so does a key that two of them write.

### The arrow

**39.** `DELETE archive();` has no arrow, under a client whose arm `2xx { body } => Reply::into_result(body)?` would read its body.  
Point: the endpoint's name; the arm is labelled too.

```text
error: a status arm reads the body, but this endpoint has no return type
label: no return type (`->`) is declared
help: declare the intended return type with `->`, or stop the arms that apply here from binding `body`
```

### Middleware

**40.** `middleware PerSecond::new(per_second);`, where `per_second?: u32` is an optional input of the client and `PerSecond` a middleware of the author's own.  
Point: `per_second`.

```text
error: middleware cannot use the optional client input `per_second`
label: middleware is constructed in `new`, before optional inputs are set
help: declare `per_second: u32`, without `?`
```

### Entries, bases and keys

**41.** `headers { user_agent; }`: a short entry in `headers` whose name contains `_`.  
Point: `user_agent`.

```text
error: a short entry in `headers` cannot use a name with `_`
label: this entry would send a header named `user_agent`
help: write `"user-agent" = user_agent;`
```

**42.** `headers { "host" = h; }`: `host` as a key of `headers`, in any case.  
Point: `"host"`.

```text
error: `host` cannot be set in `headers`
label: the host comes from the `base`
help: remove this host header and put its intended host in the base
```

**43.** `GET list();` has no `base` on it or above it.  
Point: the endpoint's name.

```text
error: no `base` applies to this endpoint
label: no `base` is declared here or inherited
help: set the intended base URL on this endpoint or an ancestor
```

**44.** `override 2xx => …` under an inherited `200 { body } => …` that is neither overridden nor removed.  
Point: `override 2xx`; the inherited `200` is labelled too.

```text
error: `override 2xx` leaves the inherited arm `200` in place
label: an inherited exact arm still matches `200`
help: also write `override 200 { body } => body`, or `remove 200`
```

**45.** A status key that is not one token of three digits from 100 to 599, or a digit from 1 to 5 and `xx`: `600 =>`, `2XX =>`, `20x =>`.  
Point: the key.

```text
error: invalid status key `600`
label: a key is 100 to 599, or 1xx to 5xx
help: choose the intended exact status from 100 to 599 or class from 1xx to 5xx
```

**46.** `GET find() -> Option<u64>;` under a client whose arm `2xx { body } => unpack(body)` would read its body.  
Point: the arrow; the arm is labelled too.

```text
error: an inherited arm reads the body for an `Option` return type
label: `null` in the body could become `None` without an explicit arm on this endpoint
help: add `status { override 2xx { body } => Some(unpack(body)) }` to this endpoint
```

The same for a `Result` after the arrow, and for an exact 2xx arm from above (`200 { body } => …`). An arm of the endpoint's own, or of a part that it names, is its author's explicit choice and is not rejected. A `HEAD` never meets this message: its inherited arms that bind `body` are left out before the check.

## 2. When a request is built or answered

One pattern, in Rust's own style: `<kind>: <fixed words>, in <endpoint>[, after N attempts]`. `, after N attempts` is printed only when N is more than 1. The name of the endpoint starts with the client. The status is the number of the response, never a value from it. No value of the request or of the response appears, and no text that your code provided. The length of a timeout prints in seconds, with a decimal part only when it has one (`30 s`, `1.5 s`, `0.125 s`). A Build error sends nothing.

**Building the request.** Every text starts with `build error:`.

| Failure | Printed text |
|---|---|
| A base that is not a URL that can be used: a fixed base or a `Base` whose host changed when it was parsed, or an empty value in a port placeholder | `build error: the base is not a usable URL, in tasks.get` |
| A string passed to `with_base` that is not a base | `build error: the base is not a usable URL (with_base), in tasks.get` |
| A host placeholder whose value is not one label, or is empty | `build error: input region cannot be used in this host label, in tasks.get` |
| An empty path value, or a finished segment that is `.` or `..` | `build error: input id is empty or produces a dot path segment, in tasks.get` |
| A shape that cannot be serialized at its place, or a value nested more than 128 levels deep. The place is `a path`, `a host or port`, `a query`, `a header` or `a body` | `build error: input filter cannot be serialized into a query, in tasks.get` |
| A number that is not finite | `build error: input ratio is not a finite number, in tasks.get` |
| A final header value with a byte that is not 0x20 to 0x7E or a tab; also the content type of a body (`Body::new(bytes, label)`, `Body::stream(…).content_type(label)`), which is the header `content-type` | `build error: the value of header x-note is invalid, in tasks.get` |
| A `body` expression that returns an `Err` (the author's error is `source()`), or a body of entries that cannot be written (in a form: a list or a struct as a value) | `build error: the body could not be constructed, in tasks.create` |
| An entry of `query` or `headers` whose expression fails (`?`); the author's error is `source()` | `build error: input labels could not be evaluated, in tasks.list` |
| `request()` on a call whose body is a one-shot stream, also inside a body made of bodies (`multipart`) | `build error: request() cannot take a one-shot body stream (use into_request), in files.upload` |
| A body that is a one-shot stream, sent a second time: through a clone of the call, or by the next page of a paginated call; also inside a body made of bodies, before anything goes out | `build error: the one-shot body stream cannot be used in this request, in files.upload` |
| A URL longer than 65,534 bytes, which the HTTP layer cannot hold: before any middleware sees the request, zero attempts | `build error: the URL is too long to be sent, in tasks.get` |
| A URL at another origin: a page that leaves the origin of the first request of its `.pages()`, or a URL passed to `at_url` that leaves the origin that the call builds | `build error: the URL leaves the origin of the call, in tasks.list` |
| A next URL, passed to `at_url`, that does not parse | `build error: the URL passed to at_url could not be parsed, in tasks.list` |
| A `content-length` written by the description that is not the length of a body in memory; for a `Body::repeatable`, a `content-length` that is not a number, or two that differ, before the body is opened | `build error: the content-length does not match the body length, in files.upload` |
| An edit (`edit_request`) that reads or removes the repeatable body's placeholder, or replaces it with a detected different kind of body; no attempt is made. Wrappers and look-alikes cannot always be detected; if building succeeds, the declared repeatable body is sent | `build error: an edit replaced or removed a repeatable body, in files.upload` |
| More header lines than `http::HeaderMap` can hold (32,768) | `build error: the request has too many headers, in tasks.get` |
| The default `concord::Http` cannot be built, at the first send ([C7.3](contracts.md#c7-concords-own-client-and-the-timeouts)) | `build error: the HTTP client could not be built, in tasks.get` |
| Generated code that passes the hidden `Draft` a header name or a fixed path text that the macro would reject | `build error: the description is not valid (a header name), in tasks.get`, and `(fixed path text)` |

The name in a Build error is `input <name>`: the input for a placeholder and for a short entry (`name;`), and the key for an entry written `"key" = value;`.

**Sending, and the timeouts.** Every text starts with `transport error:`.

| Failure | Printed text |
|---|---|
| A middleware or a transport that will not send (a request that `concord::sign::SigV4` would not sign; a token that `concord::auth::Renewed` or `concord::auth::Rotating` could not renew; a call that `concord::limit::Limit` refused). The reason, a `SignError`, a `RenewalError`, a `RotationError` or a `LimitError`, is `source()`, and the `find` of its type (`SignError::find(&e)`, `RenewalError::find(&e)`, `RotationError::find(&e)`, `LimitError::find(&e)`) returns it | `transport error: the request was blocked before it was sent, in vault.balance` |
| A connection that failed before a head was received | `transport error: connection refused, in tasks.get` |
| The connect timeout of the HTTP client, when its length is known (a `concord::Http` built with no `Http::with` closure) | `transport error: no connection after 30 s (the connect timeout of the HTTP client), in tasks.get` |
| The same, when its length is not known (`Http::with`, or the caller's own client) | `transport error: no connection (the connect timeout of the HTTP client), in tasks.get` |
| Silence for longer than `idle_timeout`, before the head of the response | `transport error: no response for 30 s (with_idle_timeout changes it), in tasks.get` |
| Silence while the body is read, after the head | `transport error: no body data for 30 s after status 200 (with_idle_timeout changes it), in files.download` |
| The whole call longer than `timeout` | `transport error: the call took longer than 60 s (with_timeout), in tasks.get` |
| A timeout that the transport reported (not one of Concord's timeouts) | `transport error: the transport reported a timeout, in tasks.get` |
| A body that breaks, or ends before its announced size | `transport error: the response body could not be read after status 200, in files.download` |
| A body that broke, and no head is known | `transport error: the body could not be read, in files.download` |
| A response head that arrived and cannot be read: not HTTP, too many or too large headers, two lengths. Class Other: the standard rule does not retry it | `transport error: the response status line or headers could not be parsed, in tasks.get` |
| The caller's own one-shot body stream that fails while it is sent; its error is in the chain of `source()`, below the error of the HTTP library. For a `Body::repeatable`, also a failure while it is opened, or a source that gives fewer or more bytes than the `content-length`, whatever the transport reported; its error is `source()` itself. Class Other: the standard rule does not retry it | `transport error: the body of the request failed, in files.upload` |
| Any other failure of the transport (class Other) | `transport error: the transport failed, in tasks.get` |

**The words of a failed connection** are a closed list, one text each:

- `transport error: connection refused, in tasks.get`: an `io::Error` of kind `ConnectionRefused` in the chain;
- `transport error: connection failed, in tasks.get`: one of kind `ConnectionReset`, `ConnectionAborted`, `BrokenPipe`, `NotConnected` or `UnexpectedEof`;
- `transport error: the host name could not be resolved, in tasks.get`: the DNS lookup of the host failed;
- `transport error: no connection, in tasks.get`: anything else.

**The hint of the idle timeout** says where its length is set. It is `with_idle_timeout changes it` when the length is the default, the client's or the call's, and `with_idle_timeout on the call changes it` when the description set it (`idle_timeout …;`):

- `transport error: no response for 120 s (with_idle_timeout on the call changes it), in files.download`;
- `transport error: no body data for 120 s after status 200 (with_idle_timeout on the call changes it), in files.download`;
- with a length that has a decimal part: `transport error: no response for 1.5 s (with_idle_timeout changes it), in tasks.get`.

**Reading the response.** A text starts with `refused:` or `mismatch:`, then the status.

| Failure | Printed text |
|---|---|
| A status with no arm, not a 2xx | `refused: status 404, in tasks.get` |
| The same, after retries | `refused: status 429, in riot.matches.ids, after 3 attempts` |
| A reader that enforces its content type | `mismatch: status 200, the content type is not the one the reader expects, in tasks.get` |
| A body larger than its limit | `mismatch: status 200, the body exceeds the reader's size limit, in tasks.get` |
| A body that does not decode, with the location that the reader reported | `mismatch: status 200, items[3].kind could not be decoded, in tasks.list` |
| A body that does not decode, with no location (at the root) | `mismatch: status 200, the body could not be decoded, in tasks.list` |
| An empty body that the reader rejects | `mismatch: status 204, the body is empty, in tasks.get` |
| A 2xx that no arm matches, under the arrow of a `HEAD` | `mismatch: status 204, no arm matches this status, in tasks.exists` |
| `mismatch(…)` or `require(…)` in an arm | `mismatch: status 201, the status arm rejected the response, in tasks.create` |
| A pager that rejects a page with `mismatch(…)`, or returns the request that was just sent as the next one | `mismatch: status 200, the next page cannot be requested, in tasks.list` |
| A pager that fails with the API's problem | `refused: status 200, in tasks.list` |
| An arm or a pager whose decoder's input, returned with `Fail::from_read(…)`, does not decode at its root | `mismatch: status 200, the decoder's input could not be decoded, in chat.info` |
| The same, with an empty input | `mismatch: status 200, the decoder's input is empty, in tasks.list` |
| The same, with the error of a reader that enforces its content type | `mismatch: status 202, the decoder rejected the content type of its input, in chat.info` |
| The same, with the error of a reader's limit | `mismatch: status 200, the decoder's input exceeds the decoder's size limit, in chat.info` |

**An arm or a pager that decodes a body itself** and returns `Fail::from_read(…)` produces, on a 2xx, texts said of the decoder's input, because Concord does not verify that the input was the body: a fault at a location prints as under a reader (`mismatch: status 200, name could not be decoded, in chat.info`), and the four other failures print the last four texts of the table, never the texts of a body. Their `source()` holds the same words, and none adds `(unexpected content type)`. A pager gives these texts too, not `the next page cannot be requested`. On another status it produces the text of a refusal (`refused: status 409, in tasks.create`). A `ReadError` of a body that broke while it was read keeps nothing of that body, and gives `mismatch: status 200, the status arm rejected the response, in chat.info` (in a pager, `the next page cannot be requested`).

**When the content type was not the reader's.** When a reader reports that the content type of the response was not its own, the three texts of a body that could not be read end with ` (unexpected content type)`: `mismatch: status 200, items[3].kind could not be decoded (unexpected content type), in tasks.list`, `mismatch: status 200, the body could not be decoded (unexpected content type), in tasks.list`, `mismatch: status 200, the body is empty (unexpected content type), in tasks.get`. An empty body gets it only when the response had a `Content-Type`.

The cause that `source()` returns holds no value of the response: a reader, shipped or your own, passes Concord no cause of its own; Concord builds the cause from the location and the kind of failure, and the reader's own message (at most 1 KiB, its mark included) is behind `details()`. A location prints names of the program and indexes only: a key of a map prints as `[key]`, a variant that was read as `[variant]` (`data[key][0].kind`). An error response whose body is broken is still a Refused error. An ignored body cannot fail the call.

**A typed stream**, `RecordStream<T, client::Error>` from `Sse::new(…).try_map(…)` or `JsonLines::new(…).try_map(…)` (C11.13), ends after its first error with the client's own error, in the pattern of section 2. An error about one record names it by its one-based position as the reader counts records: a line, or an event block (blocks that dispatch no event included); a refusal and the end before the last record name none. For the faults that the reader finds, `reason()` is `None`, and `details()` adds the decoder's line and column only for a record that does not decode, when the decoder gives them:

| Failure | Printed text |
|---|---|
| The mapping returned `RecordFail::problem(p)`: the API's own error, in `problem()` | `refused: status 200, in chat.stream` |
| The mapping returned `RecordFail::mismatch(reason)`; `reason()` and `details()` give the fixed reason, which the text, `Debug` and the cause never print. Also a `RecordError` passed on with `?` that is not a decoder's failure | `mismatch: status 200, record 7 does not fit the stream, in chat.stream` |
| The mapping's decoder failed (`?` on `SseEvent::json` or `JsonLine::json`); `details()` holds `reader: line 1, column 34` when the decoder gives a place | `mismatch: status 200, record 7 could not be decoded, in chat.stream` |
| A line, or a whole event block as it arrives, is larger than the bound | `mismatch: status 200, record 7 exceeds the reader's size limit, in chat.stream` |
| A record is not valid UTF-8 | `mismatch: status 200, record 7 is not UTF-8, in chat.stream` |
| The body ended cleanly before a record ended the stream, under `until_last_record()`, also an empty body | `mismatch: status 200, the body ended before its last record, in chat.stream` |
| The body could not be read, or a timeout expired | the engine's own line, from the tables above |

Each Mismatch has one cause, `source()`, in the same words. A reader of records of your own gives the same texts with `RecordBody::fail` and `RecordBody::refuse`.

**The cause behind a failed repeatable body.** When a `Body::repeatable` fails while it is sent (`transport error: the body of the request failed`), `source()` returns the author's error unchanged, one of the core's two `io::Error` failures below, or the multipart boundary scanner's `io::Error`:

| Failure | Printed text |
|---|---|
| The function given to `Body::repeatable`, or the stream that it returned, panicked; the panic payload is dropped. Fix the panic | `the repeatable body panicked` |
| An opening of the body yielded fewer or more bytes than the `content-length` of the request (kind `InvalidData`). Make each opening yield the same bytes, or leave out the `content-length` | `the content-length does not match the body length` |

The multipart boundary scanner returns an `io::Error` with the text `multipart boundary occurred in a stream part`, kind `Other` for form-data, `InvalidData` for mixed/related. It fails before the colliding bytes are yielded. Use different part bytes or a custom writer with a collision-free boundary.

**The errors of `concord-extras`.** The values of `concord-extras` report a failure with an error type of their own. Its text is fixed words, with at most a one-based position (`MIME part 2 …`), and never a value of the request, of the response or of your code; the cause that it keeps, if any, is its `source()`. It starts in lower case unless its first word is an acronym (`JSON`, `MIME`), it ends without a period, and it holds no semicolon: what to do is in the first column of its row, not after the text. Every type below but `RecordError` and `TransferError` has `find` (`LimitError::find(&e)`), which returns it from anywhere in the chain of `source()`. You meet such an error in one of three places:

- behind `transport error: the request was blocked before it was sent`: a middleware layer (`SigV4`, `Renewed`, `Rotating`, `Limit`) would not send the request;
- behind `build error: the body could not be constructed`: a body writer in a `body` expression failed; called in your own code, the writer returns the error itself;
- returned directly: by `concord::transfer::at`, by `try_next` on a `RecordStream`, by `SseEvent::json`, by a `RotationDriver` when it ends, and by `SigV4::sign` and `SigV4::sign_any_origin`, called in your own adapter.

**Signing a request**, `concord::sign::SignError`, behind a blocked request, or returned by `SigV4::sign` and `SigV4::sign_any_origin`. The two methods return the same errors as the middleware, for the region that they are given, except that `sign_any_origin` never returns the error of another origin:

| Failure | Printed text |
|---|---|
| The body is a stream, which cannot be hashed without reading it. An unsigned payload is accepted only for S3, so other services return this even when it is allowed. Send the body as bytes | `the request body is a stream and cannot be hashed` |
| An unsigned stream in a request whose URL is not HTTPS | `an unsigned request payload requires HTTPS` |
| The request goes to another origin than the one that the signer was made for (the middleware and `SigV4::sign`) | `the request leaves the signing origin` |
| A request head or a credential field is larger than its documented bound, or a region or a service is longer than 256 bytes | `the signing input exceeds its size limit` |
| The URL, the `Host`, the encoding of the path, the framing or the `Connection` options cannot be signed | `the request cannot be canonicalized for signing` |
| The access key ID, the region or the service is empty or holds a byte other than an ASCII letter, a digit, `-` or `_`; or the secret key or a session token is empty. Check the credentials | `the signing credentials or scope are invalid` |
| A header that the signer generates could not be built or inserted; its cause is `source()` | `a signing header could not be installed` |
| A header that is signed is not valid text | `a signed header is not valid text` |
| The signing time is outside the years 0001 to 9999 (UTC). Check the system clock | `the signing time is outside the supported range` |
| A proof that your own signing code computes failed, wrapped with `SignError::failed`; your error is `source()` | `the request proof failed` |

**A credential that is renewed**, `concord::auth::RenewalError`, behind a blocked request (also the `source()` of a `RotationError`):

| Failure | Printed text |
|---|---|
| Your renewal function returned an error, or the token could not be installed; the cause is `source()` (a failed call to the token endpoint is a `concord::Error` there) | `the credential could not be renewed or installed` |
| The shared renewal took longer than `renewal_timeout` | `the credential renewal exceeded renewal_timeout` |
| Your renewal function, or another of your callbacks, panicked; the panic payload is dropped | `a credential callback panicked` |
| The token cannot be written as the chosen header | `the credential could not be formatted as a header` |
| The initial token has expired, or a renewed token does not outlast `early_by`. Return a token that lives longer, or lower `early_by` | `the credential has expired or does not outlast early_by` |
| The request goes to another origin than the base of the credential. Attach the credential only to clients of that origin | `the request leaves the credential's origin` |
| `renewal_timeout` is zero, or the settings do not agree; also a renewal that waits for itself (the renewal must use a client without this middleware) | `the credential renewal policy is invalid` |
| A deadline cannot be represented | `the credential deadline cannot be represented` |
| Your renewal function returned `RenewalError::not_sent(…)`, or `RenewalError::from_call(…)` on a call to the token endpoint that failed on its only attempt with an error that states that nothing was sent (`error.request_not_sent()`: a blocked request, a connector failure that `concord::Http` recognizes, its connect timeout included, or a failure that a transport or middleware stated with `not_sent()`): it failed, and its request was not sent; the cause is `source()` | `the credential could not be renewed, and its request was not sent` |

**A refresh token that is replaced at each use**, `concord::auth::RotationError`, behind a blocked request, or returned by the `RotationDriver`:

| Failure | Printed text |
|---|---|
| The store's `load` failed; its error is in `source()` | `the credential could not be loaded` |
| The store's `save` failed; its outcome may be unknown, so the record is loaded again | `the credential could not be saved` |
| The exchange failed, a wait timed out, a callback panicked, or another failure of `RenewalError`, which is `source()` | `the credential could not be renewed or installed` |
| The refresh token is empty, or an exchange returned the token that it consumed | `the refresh token is empty or was not replaced` |
| No known refresh token is safe to use: the store holds a `Refreshing` marker or an empty refresh token, or an exchange failed or returned an invalid one (an exchange that failed with `RenewalError::NotSent` only when saving the previous pair again failed too, or when a panic destroyed the exchange's future: this instance stays blocked, while the store holds the marker or the restored pair, which a later owner exchanges). Authorize the account again with the provider | `the account requires reauthorization` |
| The driver was not polled while the call waited. Poll the `RotationDriver`, or spawn it | `the rotation driver was not polled during renewal_timeout` |
| The driver was dropped, or a panic in the store stopped the account | `the rotation driver has stopped` |
| Returned by the driver: a replacement could not be saved durably. Save the `pair` that the error holds | `the replacement credential must be saved by the application` |

**Limits shared by the calls of a client**, `concord::limit::LimitError`, behind a blocked request:

| Failure | Printed text |
|---|---|
| The settings are invalid: a bound, a key size, a window or a setting, including a cap of zero on calls in flight. Check the bounds, the keys and the windows | `the limit policy is invalid` |
| The keys already kept, across all buckets, plus the new keys of the request would exceed `max_keys`, even after the keys that hold nothing are forgotten: a request can be refused with fewer than `max_keys` keys of its own, when other keys are still in use. Raise `max_keys` | `limit keys exceed max_keys` |
| The cost of a bucket is larger than the capacity of a known window, so the request could never be sent | `limit cost exceeds window capacity` |
| The request would wait longer than `max_hold`. `MaxHold { required, .. }` holds the wait still needed, when it is known: for a call that is safe to repeat, wait for it and call again | `limit wait exceeds max_hold` |

**Body writers**, behind `build error: the body could not be constructed`, or returned by the writer:

| Failure | Printed text |
|---|---|
| `bracket_form` or `bracket_pairs`: a root, a key or an enum variant with data that bracketed keys cannot represent (`BracketFormError::Shape`). Called in your own code, the writer returns a `WriteError`, printed `the body could not be serialized`, with this error as its `source()` | `the bracketed form has an unsupported shape` |
| `gzip`: the body is a stream; it compresses only a body in memory (`GzipError::Stream`) | `the gzip body must be buffered` |
| `json_lines`: the whole body, line feeds included, would be larger than `max_bytes`. Split the batch, or raise the bound | `the JSON lines body exceeds its byte limit` |
| `json_lines`: one record, here the third, could not be serialized; the writer's error is `source()` | `JSON record 3 could not be serialized` |
| `multipart_mixed`, `multipart_related`: no parts | `the MIME body has no parts` |
| The same: a part, here the second, is a one-shot stream. Give it as bytes or as `Body::repeatable` | `MIME part 2 is a stream that can be sent only once` |
| The same: a part does not have exactly one valid `Content-Type` header | `MIME part 2 needs one valid content type` |
| `Multipart::finish`: a field name or a file name holds a control character | `invalid multipart field or file name` |
| `Multipart::finish`: the content type of a part is not a valid header value | `invalid multipart content type` |
| `Multipart::finish`: the operating system could not provide a random boundary for a body that streams | `multipart boundary randomness unavailable` |

**Records of a stream**, `concord::read::RecordError`, from `try_next` on a `RecordStream` (`JsonLines`, `Sse`) and from `SseEvent::json`. A `RecordStream` ends after its first error:

| Failure | Printed text |
|---|---|
| The body could not be read: the text is the engine's own line, from the tables above, and the engine's error is `source()` | `transport error: the response body could not be read after status 200, in events.watch` |
| A line, or a whole event block, is larger than the bound given to the reader (`JsonLines::new(…)`, `Sse::new(…)`) | `record exceeds its byte limit` |
| A record is not valid UTF-8 | `record is not UTF-8` |
| A JSON line, or the data of an event given to `SseEvent::json`, does not decode as the type. The fields hold the line and the column, never the record | `record does not decode as the requested type` |


**A URL that the API returned**, `concord::transfer::TransferError`, returned by `concord::transfer::at` before anything is sent:

| Failure | Printed text |
|---|---|
| The URL cannot be parsed: a malformed host or port, a space or a control character in the host; the parser's cause is `source()` | `transfer URL could not be parsed` |
| The URL parsed, but the HTTP URI parser rejects it (a backtick in the host); its cause is `source()` | `transfer URL is not an HTTP URI` |
| The text has no `://` followed by a host, or it is relative, or it cannot be a base | `transfer URL must have an absolute authority` |
| The scheme is neither `http` nor `https` | `transfer URL must use HTTP or HTTPS` |
| The URL holds user information (`@` in the authority) or a fragment (`#`) | `transfer URL must omit user information and fragments` |
| Parsing would change the spelling of the URL: an uppercase scheme or host, removed dot segments, a space or a control character that would be encoded, and the like. The URL is used only when it would be sent exactly as it is written | `transfer URL spelling would change` |
| The query is present but every part of it is empty (`?`, `?&`), which `at_url` would drop | `transfer URL query would be discarded` |
| The text is longer than 65,534 bytes | `transfer URL exceeds 65,534 bytes` |

## 3. Three rules for every message

1. A message points at your own token. For a conflict, it shows both declarations.
2. It explains one mistake, and gives a valid, concrete replacement. **A help never lies:** a help line is built from your own tokens; typed as shown, it leads to no other message of the macro and does not change what the description says. **A help is checked against its context:** text is proposed only when, typed as shown in that description, it meets no other message (a key already set, a name already taken, a reader that is missing). Where the macro cannot build such a line, the help says in words what to do and shows no code.
3. It prints names and safe metadata. Values, and explanations of your own, are only in the details that you ask for (`e.details()`).
