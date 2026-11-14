# The grammar: the whole language on one page

*[Concord](../../README.md) · [docs](../README.md) · reference · previous: [Concord on one page](overview.md) · next: [the rules](rules.md)*

For you when you need the exact form of a description: the grammar of `concord::api!`, where each statement may stand, what stands on its right side and when it runs, and nineteen sentences that the grammar cannot say.

The language has twelve statements and one declaration, `part`. The table at the end answers the questions that the grammar often raises: why a part of the language has its form.

## The grammar

```ebnf
api          = "concord::api!", "{", client, "}" ;

client       = client_meta, [visibility], "client", UpperName, [plain_inputs], "{", {statement}, {part}, {scope | endpoint}, "}" ;
part         = docs, "part", name, plain_inputs, "{", {statement}, "}" ;   (* no path, no problem, no middleware *)
scope        = meta, "scope", name, [inputs], block ;
endpoint     = meta, method, name, inputs, ["->", type], ( ";" | leaf ) ;

block        = "{", {statement}, {scope | endpoint}, "}" ;      (* statements first, then children *)
leaf         = "{", {statement}, "}" ;

method       = "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
             | "METHOD", string ;                               (* the string is an HTTP token *)
inputs       = "(", [ (input, {",", input}, {",", spread} | spread, {",", spread}), [","] ], ")" ;   (* a scope, an endpoint *)
plain_inputs = "(", [ input, {",", input}, [","] ], ")" ;                                          (* the client, a part *)
input        = docs, name, ["?"], ":", type ;                   (* "?": an optional input *)
spread       = "..", name ;                                     (* a part *)
docs         = { doc_comment } ;
client_meta  = { doc_comment | "#[cfg(…)]" } ;
meta         = { doc_comment | "#[cfg(…)]" | "#[deprecated…]" | "#[doc(hidden)]" } ;   (* a scope, an endpoint *)

statement    = base | path | pairs | body | read | status | problem | middleware | paginate | idempotent | idle_timeout ;

base         = "base", (template | ref), ";" ;
path         = "path", template, ";" ;                          (* the template may end with "/" *)
pairs        = ("query" | "headers"), "{", {entry}, "}" ;
entry        = name, ";"                                        (* the key and the input have one name *)
             | ["override"], string, "=", value, ";"
             | "remove", string, ";" ;
value        = expr ;                       (* a bare string literal is a template; anything else is an expression *)

body         = "body", expr, ";"
             | "body", rust_path, "{", {body_entry}, "}" ;      (* a body of entries: `body json { a; b; }` *)
body_entry   = name, ";" | string, "=", value, ";" ;            (* no override, no remove *)
read         = "read", expr, ";" ;
status       = "status", "{", [arm, {",", arm}, [","]], "}" ;
arm          = ["override"], key, [fields], "=>", expr   | "remove", key ;
fields       = "{", field, {",", field}, [","], "}" ;
field        = ("body" | "headers" | "status"), [":", name] ;   (* naming `body` reads it *)
key          = digit15, digit, digit | digit15, "xx" ;          (* one token: 100 to 599, or 1xx to 5xx *)
digit15      = "1" | "2" | "3" | "4" | "5" ;
problem      = "problem", type, ["read", expr], ";" ;
middleware   = docs, "middleware", expr, ";" ;                  (* on the client; any number *)
paginate     = "paginate", expr, ";" ;
idempotent   = "idempotent", [expr], ";" ;                      (* a bool or a &bool; none means true *)
idle_timeout = "idle_timeout", expr, ";" ;                      (* a std::time::Duration *)

ref          = name, [".", name] ;          (* an input here, or `owner.input` for an input of a node above *)
template     = '"', { text | "{{" | "}}" | "{", ["*"], ref, "}" }, '"' ;
```

`type`, `expr`, `rust_path`, `visibility` and string escapes are Rust's. A type in a description is not `Self` (message 10: write the path of the type). `api!` writes items, so, like every item of Rust, a type cannot name a generic parameter of an enclosing function (`rustc` says it: E0401). A `name` is lower snake case, an `UpperName` upper camel case. A template is read after Rust has decoded the string, and a raw string is a template too. A hole holds a `ref` and nothing else: no space, no comment. A `{*x}` hole stands in a path only (message 29). `#[doc = "…"]` is the same as `///`. The string after `METHOD` is an HTTP token (`METHOD "VERSION-CONTROL"`). A string of the language (a template, a key, the string after `METHOD`) has no suffix: `"a"x` is refused with `a language string cannot have a suffix`. Where the grammar alone does not decide (`remove;` or `remove "key";`; `body json { a; }` or `body json(x);`), the parser's choice is in the macro, [step 1: parse](../internals/macro.md#step-1-parse).

**No word of the language stands on the right.** After `body`, `read`, `middleware`, `paginate`, `idempotent` and `idle_timeout` comes ordinary Rust, and so in an arm and in an entry that is not a template. In `body json { … }`, `json` is the path of a Rust function, and the block holds entries (sentence 18). `json`, `Json`, `by_offset` and the others are ordinary items of `concord`, written with the same public traits and helpers as yours, and brought in by an ordinary `use concord::{json, Json};`. The macro knows none of them. A name of your own wins by Rust's own rules.

**The limits of the parser.** A description may nest 64 scopes, and an expression 64 levels (a block in a block, a call inside the arguments of a call). A long flat expression (a chain of calls, a sum of many terms) is not nesting: it has no limit of its own below 256 operators. Beyond, the message says that the expression is too long, not that it nests too deep.

## Where each statement may stand

Each statement is written at most once in a node, except `middleware`, which the client writes as often as needed. Entries and arms are written as often as needed.

| Statement | Client | Part | Scope | Endpoint |
|---|---|---|---|---|
| `base`, `query`, `headers`, `read`, `status`, `idle_timeout` | yes | yes | yes | yes |
| `path` | yes | no | yes | yes |
| `body`, `paginate`, `idempotent` | no | yes | no | yes |
| `problem`, `middleware` | yes | no | no | no |

The statements of a part count as the statements of the node that names it (sentence 11), so the column of that node holds for them too: a part with `paginate` is named by endpoints only.

## What stands on the right

| Where | What is written | What it must give |
|---|---|---|
| `base`, `path` | A template: text with holes. `base` may also name one input of type `concord::Base`. | Text. A hole is an input: one piece of a path (`{*x}`: several, sentence 13), one label of a host or a piece of one. |
| An entry of `query` or `headers` | A template, or a Rust expression. | Something that `serde` writes as nothing, one text, or a list of texts; or a `concord::Secret`. |
| `body` | A Rust expression. | A `concord::Body`, or a `Result` of one. `json(x)` and `form(x)` are two functions that give one. |
| `body f { … }` | The path of a function, and entries as in `query`. | `f` is called with one `concord::body::Entries` and gives what `body` gives (sentence 18). The value of an entry is a template, anything that `serde` can write, or a `concord::Secret` (written as its text). A `None` writes nothing ([rule 9](rules.md)). |
| `idle_timeout` | A Rust expression, with the inputs as variables. | A `std::time::Duration`. |
| `read`, on its own line or on the `problem` line | A Rust expression. | A value with `concord::Reader` for the type to read. `Json`, `Text`, `Binary` and `concord::read::Stream` are four of them. |
| `idempotent` | A Rust expression, or nothing. | A `bool` or a `&bool`. Nothing means `true`. |
| `middleware` | A Rust expression, in which the required inputs of the client (`key` or `client.key`) are the only variables. | A value with `concord::Middleware`. |
| An arm | A Rust expression. | A value of the type after the arrow. |
| `paginate` | A Rust expression, in which an input of the endpoint names its place on the call. | A function from the call and its answer to the next call. `by_offset`, `by_offset_limit`, `by_cursor`, `by_page`, `by_link` and `by_next_url` are six functions that give one. |

**Wherever Rust is written, an input is a variable.** A bare name is an input of this node; `client.x` or `scope_name.x` is an input of a node above. The variable is a reference to the input (`&Option<u32>` for `count?: u32`). (`paginate` is the one place where it stands for something else: see sentence 8.) A bare name that is not an input is whatever Rust finds in your module, and an input hides an item of its name in the lines of its node ([rule 4](rules.md)).

## When an expression runs

Each expression of a description is ordinary Rust, which the generated code runs at a fixed moment. None is awaited: none may be `async`. Every expression sees the inputs and the owners of the node that wrote it, an arm and `read` too.

| Expression | Runs | `?` in it |
|---|---|---|
| An entry of `query` or `headers`, `body` | Once, when the request is built. | Ends the call with a Build error, with the failure as its `source()`: any error that `Box<dyn Error + Send + Sync>` takes. The text is `the body could not be written` for a `body`, and `input <name> could not be written` for an entry. |
| An entry of a body of entries | Once, when the request is built. A value that is a `None` is left out ([rule 9](rules.md#the-request)). | As for an entry: a Build error. |
| `idempotent` | Once for a send, after `idle_timeout` and before the request is built. | Does not compile, and nor does `.await`. |
| `read`, and the reader of `problem` | At most once for each answer, before Concord decides what it reads. So it may run for an answer that is then not read: for a refused answer whose body is cut, the reader of the problem is made and not used. An expression of `read` has no effect of its own to count on. | Not promised either way: it compiles only when the expression's own type is a `Result` that is itself a reader, which no shipped reader is. `.await` does not compile. |
| An arm | For each answer that it takes. | On the API's problem type: the call ends with it. On a `concord::Fail` (`concord::mismatch(…)`, `headers.require(…)?`): the call ends as a Mismatch. |
| `paginate` | Once, when `.pages()` is called. What it gives owns what it holds. | Does not compile. |
| `idle_timeout` | Once for a send, first of all. It gives a `std::time::Duration`. | Does not compile. |
| `middleware` | Once, in `new`. Only the required inputs of the client are there: an optional input is set after `new`, and naming one is [message 40](messages.md). | Does not compile: `new` cannot fail. |

For each send (each page is one), `idle_timeout` runs first, then `idempotent`, then the expressions of the request, in the order of [rule 10](rules.md#the-request).

**`return` in an expression.** In `read { … }`, `idempotent { … }` and a pager, `return v` gives the value of that expression, of the type that you would write there. In an arm and in the expression of `body`, `return` gives what `?` would give: `return Ok(17)`, `return Err(…)`. A plain value and `?` cannot both hold in one expression.

## Nineteen sentences that the grammar cannot say

1. **A node sets, a child inherits.** For a statement that holds one value (`base`, `read`, `idle_timeout`), the nearest one is used: an endpoint under a JSON client just says `read Text;`, and an endpoint on another host says its own `base` (the `path` lines above it still apply). Every endpoint has a `base` in reach. For what has a key (an entry of `query` or `headers`, an arm), a key that is set above is set again only with `override`, and taken away only with `remove`: there a clash could be silent. `path` only adds.
2. **Whether a request may be repeated is set by the method** (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`), or by `idempotent;`. `idempotent false;` takes it away from a `PUT` or a `DELETE`. What follows the word is a Rust expression that gives a `bool`, with the inputs as variables.
3. **An input is optional when its name is marked: `count?: u32`.** It is then a method, not an argument, and it is not sent when it was not set. Without the mark an input is required, whatever its type: `x: Option<T>` is an argument that may be `None`. `milestone?: Option<u64>` has three states ([rule 2](rules.md)). A `Vec<T>` input is a list; `tags?: Vec<String>` is an optional one.
4. **Every input is used**, at its node or below. A name inside a string of a macro call (`format!("{n}")`) counts as a use.
5. **An exact status wins over a class**, in one node and from above. An arm is written, overridden and removed by its key; what it takes decides only when `override` is needed: an exact status under a class arm from above is written with `override` ([rule 13](rules.md)). A 2xx that no arm takes is the success (sentence 12). No arm of a `HEAD` binds a body: the edits are resolved first, then an inherited arm that binds `body` is left out.
6. **An arm binds what it uses:** `200 { body } => Some(body)`. The fields are `body`, `headers` and `status`. `body` is read when it is named. A field takes another name with `body: task`. Fields are left out without `..`. A field, or a name, is written at most once.
7. **One client for one `api!`.** Names follow [contracts](contracts.md) C1.4 to C1.6.
8. **In `paginate`, an input names its place on the call, not its value.** A pager sets it on the next call: `by_offset_limit(start, count)`.
9. **An endpoint that reads a body has a reader in reach:** a body is read by the simple rule, or by an arm that binds `body`. `HEAD exists() -> bool { status { 2xx => true, 404 => false } }` needs none. There is no default format: a client says `read Json;`. The `problem` type is read by the reader written on the client, unless `read` on its line names another.
10. **A value that holds a `Secret` is secret wherever it is placed.** In a header, the header is marked sensitive. In a query, the pair goes with the call to a next URL too. In a body, it is written as its text. It stays with the `base` where it was written ([rule 16](rules.md#behaviour)).
11. **A part is a named group of inputs, with the statements that place them.** It is written in the client's block, after the statements and before the children. It may hold a `base` (`part regional(route: Region) { base "https://{route}.api.riotgames.com"; }`). It holds no `path`, no `problem`, no `middleware` and no child, and it names no other part. A scope or an endpoint names it at the end of its inputs, `..page`; several are written `..a, ..b`. A part that no node names does not compile. What the node then has is [rule 25](rules.md).
12. **After the arrow, `Option<T>` and `Result<T, E>` are read as `T`.** A 2xx that no arm takes is read into `T` and given as `Some` or `Ok`; the arms give the rest: `404 => None`, `409 { body } => Err(body)`. The spellings are the closed list of [contracts](contracts.md) C1.7, used for the arrow only. A `HEAD` with an arrow has nothing to read: it writes an arm for a 2xx, an exact status or the class (`2xx => true`), and a 2xx that no arm takes is a Mismatch. An endpoint with no arrow may have no arm in reach that reads a body ([rule 12](rules.md)). An arm from above that takes a 2xx and binds `body` does not reach an endpoint whose arrow is an `Option` or a `Result`: that endpoint writes the arm itself, or removes it ([messages](messages.md) 46). For a `HEAD` the arms from above that bind `body` are left out first, so a `HEAD` never meets that message.
13. **A `{*x}` hole is a whole segment of a path, and its value may hold `/`.** Each piece between two `/` is encoded, and a piece may be empty. A `{x}` hole is one piece, not empty. What is refused when the request is built is in [rule 6](rules.md).
14. **`?` in an expression ends the call.** In an entry or a `body`, with a Build error. In an arm, with the API's problem, or with a Mismatch on a `concord::Fail` (`headers.require("location")?`). It is not allowed in `paginate`, `idempotent`, `idle_timeout` and `middleware`: there `rustc` refuses it, with its own message. In `read` (on either line) it is not promised either way ([when an expression runs](#when-an-expression-runs)).
15. **An input may have the name of any word of the language** (`base`, `path`, `body`, `query`, `status`, `part`, `remove`) that is not a Rust keyword: a word is a word only where a statement or a declaration starts, so `query { query; }` is legal. It may not have the name of an owner in reach: `client`, or a scope above it ([rule 4](rules.md)).
16. **A middleware is made once, in `new`,** and every handle and every call of the client shares it. A request goes through a caller's middlewares, then through the description's in the order written, then to the transport; the answer comes back the same way. It runs for every try and for every page. Nothing takes it away: not a node below, not a caller. A `///` above it goes to the page of the client; the text of the expression is never printed. What a user must know is [rule 26](rules.md).
17. **A short entry `name;` is an input of this node**, and the key is its name: `client.x;` is not one (message 12; write `"x" = client.x;`). In `headers`, a short entry whose name has `_` is refused (`"user-agent" = user_agent;`), and `host` is not a key: the host is the `base`.
18. **A body may be made of entries:** `body json { chat_id; text; "reply_markup" = markup_of(text); }`. An entry is as in `query`, with no `override` and no `remove`. The entries are one value, `concord::body::Entries`, that `serde` writes as a map in the order written, and the name before the block is called with it: `body json { a; b; }` is `body json(entries)`. Any function that takes what `serde` can write fits, `json`, `form` or one of yours: the macro still knows no format. **That name is a path to a function, seen from where `api!` stands: an input of the same name does not hide it.** So the same spelling in a part and in the node is the same function. A part may hold a body of entries; the node that names it writes one with the same function too, or no body, and the entries are put together: the parts' first, in the order in which the node names them, then the node's own. What is left out, and what is `null`, is [rule 9](rules.md).
19. **`idle_timeout <expression>;`** gives the clock for silence of the calls below. The expression gives a `std::time::Duration`, with the inputs as variables: `idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);` for a long poll. How it meets the settings of a caller is [rule 19](rules.md).

## Why the language has this form

| You may ask | The answer |
|---|---|
| Why `idempotent false;`, and not `remove idempotent;`? | `remove` always takes away a key set above, and nothing above sets `idempotent`. The word takes any `bool` expression (sentence 2). |
| Why do entries use `=` and `;`, and arms `=>` and `,`? | An entry gives a key its value; an arm is a `match` on the status. |
| Why is `paginate` an expression in which an input names a place? | A pager is ordinary Rust, like every other right side; in it, an input names its place on the call (sentence 8). A pager may give `Some(call)` or `Ok(Some(call))`, as Rust accepts both from `main`. |
| Why does an entry value have no forms of its own, such as a `join`? | A value is a template or a Rust expression: `"labels" = labels.join(",");`. |
| Why `problem S3Problem read Xml;`? | The line names a type: it defines the error type of the client. `read` is the word that the language has for a reader. The line is rare: the problem follows the reader of the client (sentence 9). |
| Is `json!({ … })` a body? | No. A body is what a function gives: `body json(json!({ … }));`. |
| Why may a scope drop its parentheses, and an endpoint not? | A client or a scope with no input is a name; an endpoint is always a call. |
| Why is "optional" a mark on the name, and not the type? | `count?: u32` (sentence 3): so an alias of `Option` changes nothing. |
| What does "no arrow" mean? | The body is ignored, with no exception: an arm in reach that would read a body needs an arrow (sentence 12). |
| Why does an arm bind `body` in a pattern? | `200 { body } => …` (sentence 6): a word inside a Rust expression starts nothing, and a field can be renamed, so no input is hidden in an arm. |
| Why statements before children? | The facts of a node are read before what is inside it. |
| Why is a suffix after a resource (`.json`) written in each endpoint? | The language has no word for one family of APIs. |
| Why is a node under `#[cfg]` still checked? | A macro cannot know which features are on. |
| Why does the macro not know `bearer`, `Json`, `json` or `by_offset`? | The macro knows no format, no scheme and no pager. They are ordinary Rust, imported. |
| Why no `override` before `base` or `read`? | `override` and `remove` are for a key (an entry, an arm). A statement that holds one value (`base`, `read`) is the nearest one. |
| Where is `auth`? | There is none. A credential that is a value is a `Secret` in `headers` or `query`. A signature, a token that is renewed or a quota is a `middleware` on the client: what every request goes through (sentence 16). The description says it, `new` makes it, and `with_transport` does not replace it. |
| Why parts? | An API often writes the same inputs and pager on every list: a part writes them once (sentence 11). A part may hold a `base`, so a host that depends on an input (Riot's route) is written once. |
| Why may `base` stand on an endpoint? | An endpoint on another host (an upload) needs no scope around it only to hold a `base`. The nearest one wins. |
| Why are `Option<T>` and `Result<T, E>` after the arrow read as `T`? | So that a `200` whose body is `null` is not a silent `None`, and a `Result` arrow needs no arm for its success (sentence 12). |
| How are arms compared? | By what they take, not by how they are written (sentence 5). |
| Why are the shipped pagers named `by_…`? | So that an input does not hide the function: `paginate by_offset(offset);` compiles. |
| Why a body made of entries? | Each input of a body is written once, not again as a JSON key; one that is not set is left out, not sent as `null`; and a part can place an input in a body (sentence 18). |
| Why `idle_timeout` in the description? | A long poll, or a stream that is quiet for minutes, is a fact of an endpoint, and no caller should have to know it (sentence 19). |
| Does an entry do the same in a body as in `query`? | Yes: a `None` writes nothing in every block of entries, `"k" = Some(x);` writes `null`, and a `Secret` is written as its text ([rule 9](rules.md)). |
| Why is the `f` of `body f { … }` a path? | So that an input named `f` does not hide the function, and one spelling in a part and in its node calls one function (sentence 18). |
