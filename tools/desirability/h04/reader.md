# H04 reader prompt (fresh session, no tools, no repository)

Instrument text for `tools/desirability/h04/runner.py`. One session answers the four tasks of one SDK
for one arm. The runner fills the five placeholders (`{CRATE}`, `{CHEATSHEET}`, `{TYPES}`, `{DOCS}`,
`{TASKS}`) and sends everything below the line. The example values are neutral (an imaginary
`ExampleClient`, `items`, `ExampleError`) and match no task. The code rules are the I3 R3
answer contract (`probes/score.py` `call_form`, form `value`): one expression, statements only
inside a closure body, no `?` (I3's final-`?` compatibility form is not offered here).

---

You are an engineer about to use a Rust SDK built with Concord, a Rust DSL that generates HTTP
clients. You have no tools and no repository. Answer from the materials below only; do not explain.

Below are:
1. Concord's one-page cheat sheet;
2. the SDK's public types (crate `{CRATE}`), signatures only;
3. the SDK's generated documentation, as text;
4. four numbered tasks.

Reply with exactly one JSON object and nothing else:

```json
{"answers": [
  {"task": 1, "construction": "ExampleClient::new(http)", "url": "https://api.example.test/v2/items?limit=10", "confidence": "high"},
  {"task": 2, "code": "api.items().list().set_limit(10).send().await", "use": "value.items.len()", "confidence": "low"},
  {"task": 3, "send_returns": "Result<Page<Item>, ExampleError>", "error_type": "ExampleError", "status_expr": "e.status()", "confidence": "high"},
  {"task": 4, "choice": "C", "requests": 2, "confidence": "high"}
]}
```

Each task names its fields; give every field of every task. If you are unsure, give your best
answer and set `"confidence": "low"`.

Field formats:
- URL: the full URL as a string, with scheme, host, path and query.
- Choice: the option letter, as a string.
- Count: a JSON number.
- Name (a variant, a header): a string with just the name.
- Type: one Rust type written out in full as the compiler resolves it, spelled as the materials
  spell it (a type the SDK exports, or `concord::…`); no `_`, `impl`, `Self`, elided lifetimes or
  `<T as Trait>::Name` projections; do not invent a crate or module prefix.
- Code, as a string:
  - `code`: one expression whose value is the `send().await` result; it ends with `.send().await`,
    with no `?` on it and no trailing `;`;
  - `construction`: one expression whose value is the requested client or handle;
  - any other code field (`use`, `…_expr`): one expression over the variable the task names
    (`value` or `e`), of the stated type.
  Rules for all code: no `?` anywhere (use `.unwrap()` for a fallible step); no `;`, `let` or
  `use` statements, no other `.await`, no `return`, `break`, `continue`, loops, `async` blocks or
  `unsafe`, except inside a closure body called in place (`(|| { let mut o = T::default(); o.x = 1;
  o })()`), where statements are allowed; use only the `vec!` and `format!` macros; do not create
  a `reqwest::Client` or call `with_base_url` (use the variables a task gives, such as `http`),
  and do not touch files, processes, the network or the environment. Your code is
  compiled as an external caller of the crate, with `use {CRATE}::*;` in scope: the SDK's public
  names, `concord::…` and `std::…` paths resolve.

## 1. Cheat sheet

{CHEATSHEET}

## 2. Public types of `{CRATE}`

```text
{TYPES}
```

## 3. Generated documentation

```text
{DOCS}
```

## 4. Tasks

{TASKS}
