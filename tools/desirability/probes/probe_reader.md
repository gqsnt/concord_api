# Probe reader prompt (fresh session, no tools, no repository)

Instrument copy of the mission pack's `prompts/probe_reader.md`, with a neutral result example
(the pack's example named `kraken::Error`, which invites guessing a crate prefix that the strict
error-owner scoring then marks wrong), and the R3 answer contract: a call answer is one expression
whose value is the `send().await` result (the scorer binds it to the declared result type and
checks it succeeded; README "Scoring", R3).

The runner packs the materials and sends this text. One session answers one SDK bank. Run the
session with no tools enabled; discard and re-run it if the transcript shows a tool call. Record
the requested and observed model.

---

You are reading an HTTP client declaration written with a Rust DSL called Concord. Below are:
1. the language's one-page cheat sheet;
2. one SDK's declaration (`concord::api! { … }`) and the Rust types it names;
3. {for R2/R3 banks: the SDK's generated documentation, as text};
4. numbered questions.

Answer from these materials only. Do not explain; do not hedge inside the JSON. If you do not
know, give your best prediction and set `"confidence": "low"`.

Answer with one JSON object:

```json
{
  "answers": [
    {
      "q": 1,
      "method": "GET",
      "url": "https://host/path?a=1&b=2",
      "headers": {"authorization": "<secret>", "accept": "application/json"},
      "body": null,
      "content_type": null,
      "confidence": "high"
    },
    {"q": 2, "send_returns": "Result<Vec<Item>, concord::Error>", "error_type": "concord::Error", "confidence": "high"},
    {"q": 3, "code": "api.account().balances(BalanceOptions::default()).send().await", "confidence": "low"}
  ]
}
```

Write type paths as the materials spell them (a type the SDK re-exports or names in its
documentation, or `concord::…`); do not invent a crate or module prefix.

Rules for call answers (`code`):
- `code` is one expression evaluating to the `send().await` result: it ends with `.send().await`,
  with no `?` on that outermost send and no trailing `;`.
- Chain client setters on `api` (`api.set_x(..).endpoint(..).send().await`) and write values
  inline: no `;`, no `let` or `use` statements, no other `.await` or `?`, and no `return`,
  `break`, `continue`, loops, `async` blocks or `unsafe`.

Rules for wire answers:
- Give query pairs in the order they are sent.
- Write secret header values as `<secret>`.
- List only headers that Concord itself sets; leave out `host`, `content-length` and anything the
  HTTP client adds.
- Write `body` as JSON when the body is JSON, as a string otherwise, or `null` when there is none.

{CHEATSHEET}

{DECLARATION_AND_TYPES}

{GENERATED_DOCS_IF_ANY}

{QUESTIONS}
