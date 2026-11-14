# Real APIs, written with Concord

*[Concord](../../README.md) · [docs](../README.md) · examples · next: [GitHub](github.md)*

For a Rust developer who wants to see their kind of API in Concord: eight real APIs, each a whole description to copy from, with the Rust beside it, a few calls, and what fits badly.

These are examples of Concord, not SDKs of these providers. A detail of a provider comes from its documentation, and one that is not sure is marked `(unsure)` on its page. No test calls a real API. Every description, the Rust beside it and the calls compile, in the crate [`tests/real-apis`](../../tests/real-apis/README.md); two blocks that need other crates (the signer of S3, the quota of Riot) compile in its tests only.

## The eight

| API | Page | Endpoints | The example of |
|---|---|---:|---|
| GitHub REST | [github.md](github.md) | 41 | cursor pages in a `Link` header; a second host for uploads; a middleware that keeps the rate-limit headers; a field cleared with `null` |
| Riot Games | [riot.md](riot.md) | 62 | many hosts chosen by an input, a part that holds `base`; a quota shared by every call; a `429` that was not applied |
| An object store like S3 | [s3.md](s3.md) | 17 | a signature on every request; XML; an object read as a stream of bytes; an upload in several parts |
| Stripe | [stripe.md](stripe.md) | 38 | form bodies with nested keys, and `expand` added by a part; an idempotency key on every `POST`; a decline as a value |
| Kubernetes | [kubernetes.md](kubernetes.md) | 18 | a watch that never ends; a log followed as it is written; patches whose kind is a header; a certificate that the connection needs |
| Slack Web API | [slack.md](slack.md) | 14 | a refusal inside a `200`; method names with a dot; one page type for lists with different names; an upload in three steps |
| Telegram Bot API | [telegram.md](telegram.md) | 13 | a token in the path; an envelope around every answer; a long poll; a `multipart` upload |
| Elasticsearch | [elasticsearch.md](elasticsearch.md) | 15 | a `GET` with a body; a body of many JSON lines; a search walked page by page in a point in time |

## When an API wants something Concord does not say

Each way out is plain Rust, beside the description or in the calling code: the ways down are explained in [when Concord does not say what you need](../guide/extending.md), and each limit, with its reason, in [scope and limits](../design/scope.md).

| The API wants | The way out | Seen in |
|---|---|---|
| A refusal inside a `200` | An arm on the 2xx that reads the body and gives the client's problem, or the value. | [Slack](slack.md), [Telegram](telegram.md), [S3](s3.md) |
| A write sent again after a `429` that was not applied | The author's retry policy, `Retry::standard().not_applied_on(429)`, shipped as `retry()`. Not `idempotent;`, which also sends it again after a lost connection. | [Riot](riot.md), [Slack](slack.md), [Telegram](telegram.md), [Kubernetes](kubernetes.md) |
| A shared quota, the headers of every answer, a signature | A `middleware` in the description: it sees every try and every page. | [Riot](riot.md), [GitHub](github.md), [S3](s3.md) |
| "Ask again at the new place" after a `301` | Concord follows no redirect: a function that sends the same call to the new URL, or a `reqwest::Client` of yours that follows them, given to `with_transport`. | [GitHub](github.md) |
| An answer whose shape depends on an option (`expand`, `filter_path`, `format`) | The type after the arrow is fixed: a type that holds every shape, or `send_raw()` for the rest. | [Stripe](stripe.md), [Elasticsearch](elasticsearch.md) |
| Records inside a stream, and an error record among them | `read Stream;` gives the bytes; a reader of yours cuts the records. | [Kubernetes](kubernetes.md) |
| One operation under two methods (`GET` and `POST`) | Two endpoints, or a method of yours with `.edit_request(…)`. | [Elasticsearch](elasticsearch.md), [Telegram](telegram.md) |
| A header whose name the caller chooses (`x-amz-meta-*`), or one made from the bytes of the body | `.edit_request(…)` on the call, or the middleware. | [S3](s3.md) |
| A whole URL that the server gives, on another host | A plain `reqwest` client sends it, with none of your keys. | [Slack](slack.md) |
| A long poll that asks the same request again after an empty answer | A loop of yours: pages end at a request sent twice in a row. | [Telegram](telegram.md) |
| A lifetime over several calls (open, walk, close) | A function of yours around the calls. | [Elasticsearch](elasticsearch.md) |
| A certificate that the connection needs | `concord::Http::with(…)` given to `with_transport`, in a function that the author ships. | [Kubernetes](kubernetes.md) |
| The same resources under two parents | The scopes written under each parent; parts keep each endpoint to one line. | [Kubernetes](kubernetes.md) |
| Keys in camelCase | The long form, `"labelSelector" = label_selector;`, once, in a part. | [Kubernetes](kubernetes.md) |
| A list joined by commas in one value | `"labels" = labels.as_ref().map(\|v\| v.join(","));` on an optional input. | [GitHub](github.md), [Slack](slack.md) |
| Exactly one of several inputs | Each input optional, said in a doc comment; the API refuses the call that breaks it. | [Telegram](telegram.md) |
| A key with a piece `.` or `..` | It cannot be sent: the URL type of `reqwest` removes the piece. Another HTTP library, or the provider's SDK. | [S3](s3.md) |
