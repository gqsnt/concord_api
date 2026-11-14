# Troubleshooting

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [When Concord does not cover what you need](extending.md) · next: [Concord on one page](../reference/overview.md)*

Use this page when a description does not compile or a call fails. Find the message that you see, then read why it happens and how to fix it.

## It does not compile

**A Concord message** is one error in three lines: what is wrong, what is at the location it points to, and what to write instead. It points at your own token, and the macro reports all its messages in a single build. Search [the messages](../reference/messages.md) for a few words of the first line: each of the 46 messages is listed with the case that causes it and its fix.

**A `rustc` message** reports a mistake inside your own Rust code, once the description is correct. It points at your token too, and one mistake can cause several errors: read the first one.

| You see | Why | How to fix it |
|---|---|---|
| The call `is not a future`, at `.await` | The endpoint has `paginate`, so its call cannot be awaited directly; this stops anyone from mistaking one page for the whole list. | `.send().await` for one page, `.pages()` for all of them ([pages](pages.md)). |
| ``the trait bound `NewTask: Clone` is not satisfied``, ``required by a bound in `__Types::__Input0` `` | The type of an input must be `Clone + Send + Sync + 'static`. | `#[derive(Clone)]` on the type ([a signature](describing-an-api.md#a-signature)). |
| ``cannot find value `Jsno` in this scope`` (E0425), at `read Jsno;` | What follows `read` is a Rust value, and it is misspelled or not imported. | `read Json;` and `use concord::Json;`. |
| `` `Bad` cannot read a `u32` ``, with the note `write read Json; with use concord::Json;, or give Bad a concord::Reader<u32>` | The value after `read` is not a reader for the return type. | Use a shipped reader, or implement `concord::Reader<T>` for your type ([reading a success](responses-and-errors.md#reading-a-success)). |
| `let bindings cannot shadow constants` (E0530), or `` `__response` is interpreted as a constant `` | A `const` or a `static` in the module where `api!` is invoked has a name that begins with `__` (those names are reserved for Concord), or the name of an input. | Rename the constant, or the input ([rule 4](../reference/rules.md)). |
| `a secret does not follow a change of base` | An entry that holds a `Secret` is inherited under another `base`. | `override` the entry, or `remove` it, under the new `base` ([a secret stays with its base](credentials.md#a-secret-stays-with-its-base)). |
| The return type does not implement `Deserialize`, although an arm builds it | An arm for one exact 2xx status (`200 { body, headers } => …`) leaves the other 2xx statuses to the simple rule, which reads them into the return type. | An arm for the whole class: `2xx { body, headers } => …` ([when a status means something](responses-and-errors.md#when-a-status-means-something)). |

## The call failed

**Read the error.** `{e}` prints one line, `<kind>: <what>, in <endpoint>[, after N tries]`, with no value in it. `e.kind()` is Build (nothing was sent), Transport (no response, or a body that broke off), Refused (the API rejected the request, by its status or by a problem that an arm returned) or Mismatch (the response does not match the description). For a Transport error, `e.class()` says connect, timeout, body, blocked or other. `e.details()` adds the values: the headers, the start of the body, the reader's own message. Print it only where it is acceptable for values to be seen ([what a call returns](calling.md#what-a-call-returns)).

| You see | Why | How to fix it |
|---|---|---|
| A panic: `there is no reactor running, must be called from the context of a Tokio 1.x runtime` (Tokio's words) | A call was sent outside a Tokio runtime. Building clients and calls needs no runtime; sending needs Tokio with its time and IO drivers. | Use `#[tokio::main]` or `#[tokio::test]`; for a runtime built by hand, call `enable_all()`. Without those drivers, Tokio says `… but timers are disabled` or `… but IO is disabled`. |
| `transport error: no answer for 30 s (with_idle_timeout changes it), in tasks.get` | The idle timeout: no head arrived for 30 seconds. | `with_idle_timeout(…)` on the call or the client; for an endpoint that is idle on purpose, `idle_timeout …;` in the description ([timeouts](calling.md#timeouts)). |
| `transport error: the body was silent for 30 s after status 200 (…), in files.download` | The same timeout, while the body was being read. | The same fix: `with_idle_timeout(…)`, or `idle_timeout …;` in the description. |
| `transport error: the call took longer than 60 s (with_timeout), in tasks.get` | The call timeout, the `timeout` that you set, expired and ended the whole call. | A longer `with_timeout(…)`. |
| `transport error: no connection after 30 s (the connect timeout of the HTTP client), in tasks.get` | Not one of Concord's timeouts: the HTTP client could not connect. | Check the host and the network. |
| `transport error: connection refused, in tasks.get` | Nothing is listening at that host and port. | Check the `base`, or the URL passed to `with_base`. |
| `transport error: name not found, in tasks.get` | The host name could not be resolved. | Check the `base`, any placeholder in the host (`{route}`), and `with_base`. |
| `refused: status 404, in tasks.get` | A status that no arm handles, and not a 2xx. | Read `e.status()` and `e.problem()`; if the status has a meaning for this endpoint, write an arm: `status { 404 => None }`. |
| `mismatch: status 200, items[3].kind is not what the type says, in tasks.list` | The body does not fit your type, at that location. | Fix the type: `Option` for a field that can be missing, `#[serde(other)]` for a set of values that can grow ([types that survive a growing API](responses-and-errors.md#types-that-survive-a-growing-api)). The decoder's own words are in `e.details()`. |
| `mismatch: status 204, the body is empty, in tasks.get` | `Json` rejects an empty body. | An arm, `204 => None`, or no return type. |
| `mismatch: status 200, the body is larger than the limit of the reader, in tasks.get` | A reader that reads the whole body stops at 10 MiB. | `read Json.limit(…);`, or `read Stream;` for a large body. |
| `build error: …` | A value could not be placed in the request. Nothing was sent. | Fix the input that it names ([the messages](../reference/messages.md#2-when-a-request-is-built-or-answered)). |

See [the messages](../reference/messages.md#2-when-a-request-is-built-or-answered) for every run-time message and the case that causes it.
