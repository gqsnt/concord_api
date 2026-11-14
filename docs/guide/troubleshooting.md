# Troubleshooting

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [When Concord does not say what you need](extending.md) · next: [Concord on one page](../reference/overview.md)*

For you when a description does not compile, or a call failed: find the words that you see, then why, then what to write.

## It does not compile

**A message of Concord** is one error in three lines: what is wrong, what stands at the place it points to, and what to write. It points at your own token, and every message of the macro comes in one build. Search [the messages](../reference/messages.md) for a few words of the first line: each of the 46 has its case and its fix.

**A message of `rustc`** comes for a mistake inside your own Rust, once the description is right. It points at your token too, and one mistake can give several errors: read the first.

| You see | Why | Write |
|---|---|---|
| The call `is not a future`, at `.await` | The endpoint has `paginate`: its call is not awaited directly, so that nobody takes one page for the whole list. | `.send().await` for one page, `.pages()` for all ([pages](pages.md)). |
| ``the trait bound `NewTask: Clone` is not satisfied``, ``required by a bound in `__Types::__Input0` `` | The type of an input must be `Clone + Send + Sync + 'static`. | `#[derive(Clone)]` on the type ([a signature](describing-an-api.md#a-signature)). |
| ``cannot find value `Jsno` in this scope`` (E0425), at `read Jsno;` | What follows `read` is a Rust value: misspelled, or not imported. | `read Json;` and `use concord::Json;`. |
| `` `Bad` cannot read a `u32` ``, with the note `write read Json; with use concord::Json;, or give Bad a concord::Reader<u32>` | The value after `read` is not a reader of the type after the arrow. | A shipped reader, or `concord::Reader<T>` for your type ([reading a success](responses-and-errors.md#reading-a-success)). |
| `let bindings cannot shadow constants` (E0530), or `` `__response` is interpreted as a constant `` | A `const` or a `static` in the module of `api!` has a name that begins with `__` (those names are Concord's), or the name of an input. | Rename the constant, or the input ([rule 4](../reference/rules.md)). |
| `a secret does not follow a change of base` | An entry that holds a `Secret` is inherited under another `base`. | `override` the entry, or `remove` it, under the new `base` ([a secret stays with its base](credentials.md#a-secret-stays-with-its-base)). |
| The type after the arrow lacks `Deserialize`, though an arm builds it | An arm for one exact 2xx (`200 { body, headers } => …`) leaves the other 2xx to the simple rule, which reads them into the type after the arrow. | The arm for the class: `2xx { body, headers } => …` ([when a status means something](responses-and-errors.md#when-a-status-means-something)). |

## The call failed

**Read the error.** `{e}` is one line, `<kind>: <what>, in <endpoint>[, after N tries]`, with no value in it. `e.kind()` is Build (nothing was sent), Transport (no answer, or a body that broke), Refused (the API said no) or Mismatch (the answer is not what the description says). For a Transport error, `e.class()` says connect, timeout, body, blocked or other. `e.details()` adds the values: the headers, the start of the body, the reader's own message; print it only where values may be seen ([what comes back](calling.md#what-comes-back)).

| You see | Why | Write |
|---|---|---|
| A panic: `there is no reactor running, must be called from the context of a Tokio 1.x runtime` (Tokio's words) | A call was sent outside a Tokio runtime. Building clients and calls needs none; sending needs Tokio with its time and IO drivers. | `#[tokio::main]` or `#[tokio::test]`; a runtime built by hand calls `enable_all()`. Without them Tokio says `… but timers are disabled` or `… but IO is disabled`. |
| `transport error: no answer for 30 s (with_idle_timeout changes it), in tasks.get` | The clock for silence: no head came for 30 seconds. | `with_idle_timeout(…)` on the call or the client; for an endpoint that is silent on purpose, `idle_timeout …;` in the description ([time](calling.md#time)). |
| `transport error: the body was silent for 30 s after status 200 (…), in files.download` | The same clock, while the body was read. | The same. |
| `transport error: the call took longer than 60 s (with_timeout), in tasks.get` | The `timeout` that you set ended the whole call. | A longer `with_timeout(…)`. |
| `transport error: no connection after 30 s (the connect timeout of the HTTP client), in tasks.get` | Not a clock of Concord: the HTTP client found no connection. | Check the host and the network. |
| `transport error: connection refused, in tasks.get` | Nothing listens at that host and port. | Check the `base`, or the text given to `with_base`. |
| `transport error: name not found, in tasks.get` | The name of the host could not be resolved. | Check the `base`, a hole in the host (`{route}`), and `with_base`. |
| `refused: status 404, in tasks.get` | A status that no arm takes, and not a 2xx. | Read `e.status()` and `e.problem()`; where the status means something for this endpoint, an arm: `status { 404 => None }`. |
| `mismatch: status 200, items[3].kind is not what the type says, in tasks.list` | The body does not fit your type, at that place. | Fix the type: `Option` for a field that can be missing, `#[serde(other)]` for a value that can grow ([types that survive a growing API](responses-and-errors.md#types-that-survive-a-growing-api)). The decoder's words are in `e.details()`. |
| `mismatch: status 204, the body is empty, in tasks.get` | `Json` refuses a body of no bytes. | An arm, `204 => None`, or no arrow. |
| `mismatch: status 200, the body is larger than the limit of the reader, in tasks.get` | A body read whole stops at 10 MiB. | `read Json.limit(…);`, or `read Stream;` for a large body. |
| `build error: …` | A value could not be placed. Nothing was sent. | The input that it names ([the messages](../reference/messages.md#2-when-a-request-is-built-or-answered)). |

Every text at run time, with its case: [the messages](../reference/messages.md#2-when-a-request-is-built-or-answered).
