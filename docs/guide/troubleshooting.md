# Troubleshooting

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Recipes](recipes.md) · next: [Concord on one page](../reference/overview.md)*

Use this page when a description does not compile or a call fails. Find the message that you see, then read why it happens and how to fix it.

## It does not compile

**A Concord message** is one error in three lines: what is wrong, what is at the location it points to, and what to write instead. It points at your own token, and the macro reports all its messages in a single build. Search [the messages](../reference/messages.md) for a few words of the first line: each of the 46 messages is listed with the case that causes it and its fix.

**A `rustc` message** reports a mistake inside your own Rust code, once the description is correct. It points at your token too, and one mistake can cause several errors: read the first one.

| You see | Why | How to fix it |
|---|---|---|
| ``error[E0277]: `api::Call<List>` is not a future``. `` help: remove the `.await` ``. | The endpoint has `paginate`. | Use `call.send().await` for one page or `call.pages()` for all pages. |
| ``error[E0599]: the method `pages` exists for struct `api::Call<Get>`, but its trait bounds were not satisfied``. ``note: trait bound `concord::call::NoPaginate: concord::call::DeclaresPaginate` was not satisfied``. | The endpoint has no `paginate`. | Use `call.await` or `call.send().await`. |
| ``error[E0599]: the method `send_if_none_match` exists for struct `api::Call<Create>`, but its trait bounds were not satisfied``. ``note: trait bound `concord::call::Post: concord::call::IsGetOrHead` was not satisfied``. | Only `GET` and `HEAD` calls can use that method. | Use `call.send().await` for this call. Use `send_if_none_match(etag).await` on a `GET` or `HEAD` call for a conditional read. |
| `error[E0716]: temporary value dropped while borrowed` at `paginate`. ``note: requirement that the value outlives `'static` introduced here``. | The pager borrows an input that it must own. | Copy or clone what the pager needs before the `move` closure ([a pager of your own](pages.md#a-pager-of-your-own)). |
| ``the trait bound `NewTask: Clone` is not satisfied``, ``required by a bound in `__Types::__Input0` `` | The type of an input must be `Clone + Send + Sync + 'static`. | `#[derive(Clone)]` on the type ([a signature](describing-an-api.md#a-signature)). |
| ``cannot find value `Jsno` in this scope`` (E0425), at `read Jsno;` | What follows `read` is a Rust value, and it is misspelled or not imported. | `read Json;` and `use concord::Json;`. |
| `` `Bad` cannot read a `u32` ``, with the note `` declare `read Json;` with `use concord::Json;`, or implement `concord::Reader<u32>` for `Bad` `` | The value after `read` is not a reader for the return type. | Use a shipped reader, or implement `concord::Reader<T>` for your type ([reading a success](responses-and-errors.md#reading-a-success)). |
| `let bindings cannot shadow constants` (E0530), or `` `__response` is interpreted as a constant `` | A `const` or a `static` in the module where `api!` is invoked has a name that begins with `__` (those names are reserved for Concord), or the name of an input. | Rename the constant, or the input ([rule 4](../reference/rules.md)). |
| `a secret does not follow a change of base` | An entry that holds a `Secret` is inherited under another `base`. | `override` the entry, or `remove` it, under the new `base` ([a secret stays with its base](credentials.md#a-secret-stays-with-its-base)). |
| ``warning: unused import``, after a Concord message | The macro leaves out what it refuses (an endpoint, or the whole client when its head is wrong), so an import used only there is unused, and your code that calls a refused endpoint cannot find it. | Fix the Concord message first: these follow from it. |
| ``input `page_size` is never used``, while an entry seems to use it | A name misspelled in an expression of the block: `"pageSize" = page_sise;`. The macro reports the input and refuses the endpoint, so `rustc` never sees the misspelled name, and says nothing about it. | Look for the input's name in the expressions of that endpoint and its parts. |
| ``expected `SearchResource`, found `__Slot3__Input13` `` (E0308), in `paginate` | In `paginate`, every input of the endpoint is a slot on the call, not a value: also an input that is not the cursor ([the shipped pagers](pages.md#the-shipped-pagers)). | Pass the slot to your pager, and read the value with `slot.get(&call)` ([a pager of your own](pages.md#a-pager-of-your-own)). |
| `error[E0282]: type annotations needed` at `concord::Retry::standard().when(…)` | The rules of a `Retry` read the problem type, and a chain of closures does not tell `rustc` which one. | Start from the client's own alias: `kubernetes::Retry::standard()` ([retries](retries.md)). |
| ``the trait bound `Drive: serde::Deserialize<'de>` is not satisfied`` at the client's name | A type of the API has the client's name, and is brought in by a glob (`use types::*;`): the client's own `struct`, declared where `api!` is invoked, takes precedence over the glob, so `-> Drive` names the client. | Rename the type or the client (`SharedDrive`), or name the type by its path. |
| The return type does not implement `Deserialize`, although an arm builds it | An arm for one exact 2xx status (`200 { body, headers } => …`) leaves the other 2xx statuses to the simple rule, which reads them into the return type. | An arm for the whole class: `2xx { body, headers } => …` ([when a status means something](responses-and-errors.md#when-a-status-means-something)). |

In these examples, both E0599 notes point at the client's name in `pub client Api`. Both errors still name the call type and the bound when you call the client from another crate ([the messages](../reference/messages.md)).

## The call failed

**Read the error.** `{e}` prints one line, `<kind>: <what>, in <endpoint>[, after N attempts]`, with no value in it. `e.kind()` is Build (nothing was sent), Transport (no response, or a body that broke off), Refused (the API rejected the request, by its status or by a problem that an arm returned) or Mismatch (the response does not match the description). For a Transport error, `e.class()` says connect, timeout, body, blocked or other. `e.details()` adds the values: the headers, the start of the body, the reader's own message. Print it only where it is acceptable for values to be seen ([what a call returns](calling.md#what-a-call-returns)).

| You see | Why | How to fix it |
|---|---|---|
| A panic: `there is no reactor running, must be called from the context of a Tokio 1.x runtime` (Tokio's words) | A call was sent outside a Tokio runtime. Building clients and calls needs no runtime; sending needs Tokio with its time and IO drivers. | Use `#[tokio::main]` or `#[tokio::test]`; for a runtime built by hand, call `enable_all()`. Without those drivers, Tokio says `… but timers are disabled` or `… but IO is disabled`. |
| `transport error: no response for 30 s (with_idle_timeout changes it), in tasks.get` | The idle timeout: no head arrived for 30 seconds. | `with_idle_timeout(…)` on the call or the client; for an endpoint that is idle on purpose, `idle_timeout …;` in the description ([timeouts](calling.md#timeouts)). |
| `transport error: no body data for 30 s after status 200 (…), in files.download` | The same timeout, while the body was being read. | The same fix: `with_idle_timeout(…)`, or `idle_timeout …;` in the description. |
| `transport error: the call took longer than 60 s (with_timeout), in tasks.get` | The call timeout, the `timeout` that you set, expired and ended the whole call. | A longer `with_timeout(…)`. |
| `transport error: no connection after 30 s (the connect timeout of the HTTP client), in tasks.get` | Not one of Concord's timeouts: the HTTP client could not connect. | Check the host and the network. |
| `transport error: connection refused, in tasks.get` | Nothing is listening at that host and port. | Check the `base`, or the URL passed to `with_base`. |
| `transport error: the host name could not be resolved, in tasks.get` | the DNS lookup of the host failed. | Check the `base`, any placeholder in the host (`{route}`), and `with_base`. |
| `transport error: the request was blocked before it was sent, in tasks.get` | A middleware layer refused to send: one of your own, a `Renewed` or `Rotating` whose renewal failed or that was given a request to another origin, a `SigV4` that refused to sign the request, or a `Limit` that refused the call (a wait longer than `max_hold`, no room under `max_keys`, a cost larger than a window, a policy that is not valid). Nothing was sent. | Read `e.source()`: a `concord::auth::RenewalError`, `concord::auth::RotationError`, `concord::sign::SignError` or `concord::limit::LimitError` says which; each has `find(&e)` to return it ([a token that is renewed](credentials.md#a-token-that-is-renewed), [a refresh token that rotates](credentials.md#a-refresh-token-that-rotates), [AWS Signature Version 4](credentials.md#a-signature), [rate limits](limits.md)). |
| `refused: status 404, in tasks.get` | A status that no arm handles, and not a 2xx. | Read `e.status()` and `e.problem()`; if the status has a meaning for this endpoint, write an arm: `status { 404 => None }`. |
| `mismatch: status 200, items[3].kind could not be decoded, in tasks.list` | The body does not fit your type, at that location. | Fix the type: `Option` for a field that can be missing, `#[serde(other)]` for a set of values that can grow, or a variant that keeps the text when the value is sent back (`#[serde(other)]` drops it) ([types that survive a growing API](responses-and-errors.md#types-that-survive-a-growing-api)). The decoder's own words are in `e.details()`. |
| `mismatch: status 204, the body is empty, in tasks.get` | `Json` rejects an empty body. | An arm, `204 => None`, or no return type. |
| `mismatch: status 200, the body exceeds the reader's size limit, in tasks.get` | A reader that reads the whole body stops at 10 MiB. | `read Json.limit(…);`, or `read Stream;` for a large body. |
| `build error: …` | A value could not be placed in the request. Nothing was sent. | Fix the input that it names ([the messages](../reference/messages.md#2-when-a-request-is-built-or-answered)). |

See [the messages](../reference/messages.md#2-when-a-request-is-built-or-answered) for every run-time message and the case that causes it.
