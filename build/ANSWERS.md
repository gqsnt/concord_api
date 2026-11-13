# Small answers of the build

Details that the paper left open and that a developer needed. Decided by the architect; each goes into the pages at the end of its milestone. A later answer wins over an earlier one.

## A1 to A11: how a request is built (asked by the writer of the C2 tests, M1-T1)

1. **A shape that cannot be written at a place**: one pattern for every place, `build error: input <name> cannot be written in a <path | host | query | header>, in <endpoint>`. (The page gave only "in a query".)
2. A `body` expression that gives `Err`: the existing text, `build error: the body could not be written, in <endpoint>`; the author's error is `source()`.
3. Percent-encoding writes **upper-case** hex (`%2F`).
4. A request with no query pair has **no `?`**.
5. An empty text as a query value writes **`k=`**.
6. **A name in a Build error** is the input for a hole and for a short entry, and **the key** for an entry written `"key" = value;` (the engine page, 2.1). The words are always `input <name>`. Contract C2.13 says "the value of query `labels`": the contract is wrong and will be corrected.
7. **A header value** may hold what `http::HeaderValue` takes from text: bytes 0x20 to 0x7E and a tab. A space inside is legal (`Bearer {token}`). A control character or a byte over 0x7E is the Build error `the value of header <name> is not a legal header value`. "Visible ASCII" in the contracts will be corrected.
8. `Draft::header` is given the name **as written**; it compares and stores without case.
9. **A fixed `path "x/"`** : how generated code tells the `Draft` that the last piece ends with `/` is the choice of the developer of the `Draft` (hidden interface); it is reported and then written into the engine page, section 2.2.
10. **A host label** is ASCII letters, digits and `-`. A name that is not ASCII is refused; an international name is given in its ASCII form. An empty value in a port or a host hole is the Build error of the base.
11. **A template as the value of a body entry** needs a hidden function that gives the finished text or nothing. It is added with the macro's last step (M4), not now. The fixed list of boundaries of `multipart` is the code's; tests check its properties.

## A12 to A16: the error (asked by the reader of M1-E0)

12. **A `reqwest` error that is about a status or about decoding keeps no source.** (`is_status()`: its text holds the reason phrase of the answer. `is_decode()`: with `reqwest`'s `json` it can hold a value of the body.) The class is enough. Every other `reqwest` error keeps its chain with the URL taken out. C6.1's two sentences are then both true.
13. **Connect is "a connection failed", also after it was made**: a `reqwest` error whose chain holds an `io::Error` of kind `ConnectionRefused`, `ConnectionReset`, `ConnectionAborted`, `BrokenPipe`, `NotConnected` or `UnexpectedEof` is class Connect, whatever `is_connect()` says; the word printed comes from the closed list (`connection reset` for the last five but `ConnectionRefused`). A connection that the server closed with no `io::Error` in the chain stays Other: Concord names no item of `hyper`.
14. The name in a Build error is always `input <name>` (A6). No constructor for "the value of query …".
15. **An entry whose expression fails** (`?` in a `query` or `headers` entry) prints `build error: input <name> could not be written, in <endpoint>`, with the author's error as `source()`. Only a `body` prints "the body could not be written". This is a 26th text at run time.
16. A text that was already cut is not cut again (one mark).

## A17, A18: two disputes between the C2 tests and A7, A10 (M1-E1)

17. **A10 is corrected.** An empty value in a **host** hole is the Build error of a host hole, `input <name> is not one label of a host` (the tests were right, and the page's table says so). Only an empty **port** is the Build error of the base. The three disputed tests of C2.1 and of table 4 stand as written.
18. **A7 stands.** The test `c2_7_a_value_that_is_not_visible_ascii_is_a_build_error` was written from "visible ASCII" in the contract, which is wrong: a space and a tab are legal in a header value. By this decision its cases with a space or a tab are moved to "accepted"; its other cases stand.

## A19 to A24: one call (asked by the writer of the tests of C3 to C9, M1-T2)

19. Texts that the page did not give, now part of it (27 to 30): class Other, `transport error: the transport failed, in <endpoint>`; a timeout that a transport reported (not one of Concord's clocks), `transport error: a timeout was reported by the transport, in <endpoint>`; a body that broke with no head known, `transport error: the body broke, in <endpoint>`; a Mismatch at the root with the reader's label, `mismatch: status 200, the body is not what the type says, in <endpoint>`. And for `mismatch(…)` or `require(…)` in an arm: `mismatch: status <n>, the status arm refused the answer, in <endpoint>`.
20. A clock length prints in seconds, with a decimal part only when it has one (`30 s`, `1.5 s`, `0.125 s`).
21. `, after N tries` is printed only when N is more than 1.
22. The rules of `when` are asked in the order added and **stop at the first that gives a wait** (C4.7: "the first that gives a wait wins").
23. A `ByteStream` may read ahead of what the caller asked for; a test must accept both.
24. **`api/tour` gets a fifth client** for the shapes that the tests of a call need and the four clients do not have (the list is in the report of M1-T2). It is written by hand as the macro would write it, and it becomes part of the model that the macro must reproduce.

## A25 to A27: after the first run of the whole engine (M1-E2)

25. **A Mismatch at the root whose label was not the reader's either prints the page's row**: `mismatch: status 200, the body is not what the type says and its content type is not the reader's, in <endpoint>`. The label sentence is never left out (C5.11). A regression test of the answer package expected it left out at the root: that expectation was wrong and is corrected by this decision.
26. `c4_6_a_caller_keeps_the_authors_policy_by_passing_it_on` is disputed rightly: an unmarked `POST` is not sent again after a status that the policy does not list with `not_applied_on` (C4.1: three conditions, all needed). The test's writer rewrites it to what C4.6 says under C4.1.
27. **A13 is narrowed.** A failure while the body of an answer is read (the head was received) is class **Body**, whatever `io::Error` is in its chain: "the body broke after status N". Connect is for a failure before a head. (A body that ended early on the wire was classed Connect.)

## A28 to A31: the paper against the code (asked by the reader of C2 and C5)

28. **A template is always given to the `Draft` with a literal piece**, an empty one when the template is exactly one hole (`"{ids}"`): so a list in it is a Build error, as in every template. One `Val` alone is a whole value. Written into the engine page, 2.2.
29. **"Announced size" is the `Content-Length` header of the answer when it has one**, and else what the transport says of the body's size. A body that ends before its announced size is a Transport error of class Body, never a shorter value; an announced size over the reader's limit is a Mismatch before a byte is read. This holds for a `fake`, a `tower` service and a middleware that replays an answer.
30. "At most 1 KiB" is with its mark: details are cut so that the kept text and ` … [cut]` are 1024 bytes or less.
31. `Draft::segment` given only an empty literal does nothing.

## A32 to A38: the first caller (Opus `xhigh` on M1); with decisions D270 to D273 in `DECISIONS.md`

32. D270: `<client>::Retry`. D271: an arm on a non-2xx whose body does not read gives Refused. D272: a failed request with no head is Connect. D273: `StreamError`.
33. **The text of the clock for silence says where it was set and what it waited for.** Before a head: `no answer for 30 s (<hint>)`. After a head, while a body is read: `transport error: the body was silent for 30 s after status 200 (<hint>), in <endpoint>`. The hint is `with_idle_timeout changes it` when the length is the default or the client's, and `with_idle_timeout on the call changes it` when the description set it.
34. `kept()` is `None` when no byte of a body was kept (an announced size over the limit).
35. `details()` prints the state of the body in words: `body (complete, 18 bytes)`, `body (cut at 64 KiB)`, `body (interrupted, 120 bytes)`.
36. A body of no bytes that a reader refuses: `mismatch: status 204, the body is empty, in <endpoint>`. The label sentence ("and its content type is not the reader's") is printed only when the answer had a `Content-Type`.
37. A `with_base` that is no base: `build error: the base is not a URL that can be used (with_base), in <endpoint>`.
38. **`api/tour` is the tour's clients.** Where a client of `api/tour` differs from the same client in the guide (`docs/tour.md`, chapters 3 to 6), the guide is right: Riot has no client arm, and has `queue` and `start_time` on `matches.ids` as chapter 4 shows; `files.search` is `body json { text; cursor; }`. What `tour` needs beyond the guide for tests lives in `Shapes`.

## A39 to A41: the macro's helps (asked by the author reading of steps 2 to 4)

39. **A help never lies.** A help line is built from the author's own tokens. Typed as shown, it must not lead to another message of the macro, and must not silently change what the description says. Where the macro cannot build such a line, the help says in words what to do and shows no code. (Examples of what this decides: message 46 shows the author's own arm expression inside `Some(…)`; messages 3 and 7 name the place where the input really stands; message 24 before a block keeps the block's entries; message 29 keeps the text before the hole; message 35 suggests the nearest name by spelling, or none; messages 32 and 46 say "in this `status` block" when the endpoint has one.)
40. **Nothing is said about what stands under damage.** Below a node, a block or an arm that an earlier step marked as damaged or refused, no message is raised whose truth depends on what was dropped (22, 23, 9, 36).
41. `api/tour` may hold what a rule requires and an excerpt of the guide leaves out (A38 is about what differs, not about what is more): `files.upload` keeps `idempotent false;` and its `override "authorization"`; `files.download` keeps its `idle_timeout`. The guide's own examples that are not legal (`diff()` overriding a header that was never set; an `upload` with no override under a secret) are corrected in the guide at the end of the milestone.

## A42 to A48: the whole engine against the whole paper (Fable on M2)

42. **An answer with no URL of its own** (one that a transport of the caller, a `fake` or a `tower` service made from an `http::Response`: `reqwest` gives it a placeholder) **is taken to come from the URL of its request.** So a relative next URL is resolved against the request that was sent. The docs of `Transport` and `fake` say that an answer should carry its URL, and that this is what happens when it does not.
43. **A wait between two tries has one deadline**, taken once: if it is past the end of `timeout` the wait is not started; if the clock still cuts a wait that was started, the caller gets **the last outcome**, unchanged, not a timeout.
44. Through `impl Stream for ByteStream`, the clock for silence runs from the first poll that found nothing until the next piece, also between two polls (a dropped `next()` cannot be seen). Through `try_next()` it runs only while the call waits. The docs of the `Stream` form and cell 7.2 of the cases say so.
45. **`not_applied_on` opens the gate only for a refusal**: an outcome whose kind is Refused with a listed status. A body that broke, or a clock that ended, after a head with that status does not.
46. A next URL that is no URL: `build error: the next URL is not a URL, in <endpoint>`.
47. **An `edit_request` that moves the host and a pager that uses `at_url` do not go together**: `at_url` is held to the origin that the call builds, before the edits. To send a client elsewhere, `with_base`. (C8.4 and C8.6 each said a half.)
48. "A call costs two allocations for the engine" (the engine page) is not true of the code and was never measured: the sentence goes; the cost of a call is measured in M5.

## A49, A50: what the properties found (M5-E8)

49. **A value nested more than 128 levels deep is a Build error** (`input <name> cannot be written in a <place>`), at every place and in a body of entries: no input can overflow the stack while it is written. (A value of 16,294 nested `Some` did.)
50. **In `multipart`, a field name or a file name that holds a control character (a NUL, CR, LF, a tab) is the error of `finish()`.** A `"` is written `%22`; nothing else is changed. (Two names can still be written the same: see A55.)

## A51. `#![no_implicit_prelude]` around a generated client is not promised

A generated client compiles in a module that shadows `Option`, `Result`, `Some`, `Ok`, `String`, `Vec`, `Box` and the names of `concord`, `core` and `std` (D-hygiene, macro.md section 4): every path that the macro or a helper macro of the engine writes is a full path. A module under `#![no_implicit_prelude]` is not promised: method calls on the author's own values (`.clone()`, `.into()`) are written as the author wrote them. (Reading of P4.1 by Astra, fault 3 and its last paragraph.)

## A52. A local of the generated code cannot be seen by an expression of the author

Every binding that the macro writes inside a function where an expression of the author also stands has mixed-site hygiene: the author's expression cannot read it, change it, or collide with it, and a `const` or a `static` of the caller's module with the same spelling changes nothing. The names that the paper gives to the author (`body`, `status`, `headers`, the inputs, `self` where promised) are the only ones in reach. (Astra, fault 2.)

## A53. `return` in a statement's expression

An expression of the author that stands in a closure (`read { … }`, `idempotent { … }`, a `when`, a pager, a status arm) may use `return`: it returns the value of that expression, of the type the author would write, and the macro adapts it outside. (Astra, fault 4.)

## A54. Two lines of generated docs show the hidden trait, not one

`<client>::Error` and `<client>::Retry` are aliases over the problem type of the client, reached through the hidden trait of macro.md section 3. Both lines show it in `rustdoc`; every other public signature names the author's type itself (`RiotProblem`), or nothing when the client has no `problem` line. Rule 6 of section 9 says "the one line": it is two. (P4.3.)

## A55. Multipart names: two names can be written the same (corrects A50)

A50 stands for what is written: a control character in a name is an error of `finish()`, and `"` is written `%22`; `%` is written as it is, as browsers do (WHATWG, "multipart/form-data encoding algorithm"). So the names `"` and `%22` are written the same. The sentence of A50 that says distinct names stay distinct is wrong and goes. Concord does not check that the names of one body differ. (Astra, end of M2, finding 6.)

## A56. Each wait before a try draws its own random part

The random part of a wait (the standard rule's jitter) is drawn again for each wait. Two waits of one call are not tied to each other. A test fixes the source of randomness and shows two different draws. (Astra, mutation R11.)

## A57. Two clocks that end at the same instant: the outer one is reported

When the timeout of the call and the `idle_timeout` of a body end at the same instant, the error is the timeout of the call. The same rule everywhere: of two clocks that end together, the one that covers more is reported (the call's timeout over the connect timeout, over a wait, over the idle clock). (Astra, mutation R17.)

## A58. An error text names an input only when the macro or the engine wrote that name

"input <name>" in a Build error is always a name from the description. A key of a map of the user, the name of a type of the user or of a `serde` rename is data: it is never printed. A value that fails inside a map of an entry is reported with the name of the entry. (Astra, end of M2, finding 1; contract C6.2.)

## A59. The announced size never decides how much memory is reserved at once

A29 stands (the announced size is used only inside the caller's limit), and narrows: the engine never reserves more than what has arrived, doubled, with a floor of 64 KiB, however large the limit. An allocation that fails is an error of class Body, never an abort. (Opus, end of M2, finding 1.)

## A60. One copy of the call methods

The methods of a call (`send`, `send_full`, `send_raw`, `send_if_none_match`, `with_retry`, `with_timeout`, `pages`, …) are written by the engine's `call_methods!`, and the macro emits a call to it, as engine.md 2.2 and crate.md say. The macro does not write its own copy. (Opus, finding 3.)

## A61. `Entries` is written by `serde` as a plain map

C2.14 stands: any `serde` writer sees a map, with no wrapper and no hidden name. The engine learns that it is inside one of its own writers by another way than the shape. (Opus, finding 4; goes with A58.)

## A62. A writer of bodies of the author is not second class

What `json` and `form` use of the core to report "input <name> cannot be written in a body" is public and documented (`concord::body::Bounded` or what replaces it). `json()` and `form()` give the same error type, `concord::body::WriteError`, whose text is fixed and whose source is the format's error; no hidden function of `serde_json`. `MultipartError` is `#[non_exhaustive]` and shows no type of `getrandom`. (Opus, findings 5 to 7.)

## A63. The budget of a call is measured through the macro

The measure of record is a client that `concord::api!` wrote (`tour-gen`), on an endpoint with two path inputs and three `query` inputs: time and allocations added over the same `reqwest::Request` built by hand. Budget unchanged: under 2 microseconds and under 25 allocations added. The earlier "22 added" measured pairs added by `edit_request` on the hand-written client; through the macro the reader measured 30. (Opus, finding 2.)

## A64. Names that begin with two underscores belong to Concord (corrects A52)

A52 stands for expressions: an expression of the author cannot read or change a local of the generated code. It does not stand for a `const` or a `static` of the author's module: Rust reads a binding `let __x` as a pattern of the constant `__x` (E0530), and no hygiene on stable Rust prevents it. So: in a module that holds `concord::api!`, the author declares no item whose name begins with `__`. The guide says so in one line. (P4.4, probe `constants.rs`.)

## A65. `?` in the expression of `read`

`.await` does not compile in the expression of `read`. `?` is not promised either way: it compiles only when the expression's own type is a `Result` that is itself a reader, which no shipped reader is. The page drops the claim that it never compiles. (P4.4.)

## A66. A reader expression may be evaluated for an answer that is then not read

The expression of `read` and of `problem … read` is evaluated at most once for an answer, before the engine decides what it reads. For a refused answer whose body is cut, the reader of the problem is made and not used. An expression of `read` therefore has no effect of its own. (P4.4, probe `reader_factory`.)

## A67. One mistake in the author's Rust, one place, perhaps several errors

For a mistake that only `rustc` can see (a type of the author that lacks a bound), the first error points at the author's token. `rustc` may print the same lack several times, once for each generated item that needs the bound (`problem Rc<String>`: 12). The macro's own 46 messages are each given once. (P4.4; Opus's finding 1 of the reading of P4.1.)

## A68. What is fixed in a message of the macro, and what the page copies

Of the three lines of each of the 46 messages, the first (the message) and the place it points at are the page's, word for word; the code follows. The label and the help are what the macro prints for the case (A39: a help in words where no exact replacement is safe); the page copies them from the compiler fixtures, and a test runs each case of the page through the whole macro and compares the three lines. Where the page says where a message points (18 and 20: the keyword), the code follows. (Fable, end of M4, findings 3 and 10.)

## A69. `return` in a status arm and in `body` (narrows A53)

In a status arm and in the expression of `body`, the author's code runs where `?` works, so `return` gives what `?` would give: `return Ok(17)`, `return Err(…)`. In `read`, `idempotent`, a pager, `return v` gives the value (A53). (Fable, finding 5.)

## A70. A help is checked against what is in reach

A help that proposes text is given only when that text, typed as shown in that description, meets no other message of the macro (a key already set, a name already taken, a reader that is missing). Else the help is in words. (Fable, findings 2 and 4; A39.)

## A71. Limits of the parser

A description may nest 64 scopes and an expression 64 levels. A long flat expression (a chain of calls, a sum) is not nesting and has no limit of its own below 256 operators; beyond, the message says "too long", not "nesting". The grammar page states the three numbers. (Fable, finding 12.)

## A72. An honest body is held in a buffer of its own size (completes A59)

When the announced size is inside the limit, the buffer never grows beyond it: a body of 100 MiB that says so ends in a buffer of 100 MiB, not 128. The reservation still follows what has arrived (A59); only its ceiling is the announced size. (Opus, second reading, "A59 costs memory".)

## A73. The text of an error appears once in its chain

Walking `source()` from an error of Concord never prints the same sentence twice. An error of class Build whose cause is a `WriteError` has the format's own error as its source. (Opus, second reading, R2.)

## A74. A URL that cannot be sent is a Build error

A request whose URL the HTTP layer cannot hold (longer than 65,534 bytes) is refused before any middleware sees it: `build error: the URL is too long to be sent`, class Build, zero tries, nothing sent. (M6, misuse, F1.)

## A75. A head that cannot be read, a request body that fails (narrows D272)

D272 stands for a request that got no answer at all. Two cases leave it: when the server's head arrived and cannot be read (not HTTP, too many or too large headers, two lengths), the error is `transport error: the head of the answer could not be read`; when the caller's own body stream fails while it is sent, it is `transport error: the body of the request failed`, with the stream's error as the source. Both are class Other: the standard rule does not try them again (the first would fail the same way; the second has a body that cannot be sent twice). (M6, misuse, F2.)

## A76. The label of a body is a header value

`Body::new(bytes, label)` and `Body::stream(…, label)` with a label that is not a legal header value give the Build error of a header: `the value of header content-type is not a legal header value`. (M6, misuse, F3.)

## A77. Concord runs on Tokio

Sending a call needs a Tokio 1.x runtime with its time and IO drivers. Polled elsewhere, the send panics with Tokio's own message. Building a client and a call needs no runtime. The front page of `concord` says so under "Panics"; it is the one panic that the library documents. (M6, misuse, F4.)
