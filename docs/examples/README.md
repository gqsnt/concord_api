# Real APIs, written with Concord

*[Concord](../../README.md) · [docs](../README.md) · examples · next: [GitHub](github.md)*

This page is for a Rust developer who wants to see their kind of API in Concord. It presents eight real APIs, each with a complete description to copy from, the Rust code that goes with it, a few calls, and what does not fit well.

These are examples of Concord, not SDKs for these providers. Each detail of a provider comes from its documentation, and a detail that is uncertain is marked `(unsure)` on its page. No test calls a real API. Every description, the Rust code beside it and the calls compile in the crate [`tests/real-apis`](../../tests/real-apis/README.md); two blocks that need other crates (the S3 signer, the Riot quota) compile only in its tests.

## The eight APIs

| API | Page | Endpoints | What it shows |
|---|---|---:|---|
| GitHub REST | [github.md](github.md) | 41 | cursor pagination through a `Link` header; a second host for uploads; a middleware layer that keeps the rate-limit headers; a field cleared with `null` |
| Riot Games | [riot.md](riot.md) | 62 | many hosts chosen by an input, a part that holds `base`; a quota shared by every call; a `429` that was not applied |
| An object store like S3 | [s3.md](s3.md) | 17 | a signature on every request; XML; an object read as a stream of bytes; an upload in several parts |
| Stripe | [stripe.md](stripe.md) | 38 | form bodies with nested keys, and `expand` added by a part; an idempotency key on every `POST`; a decline as a value |
| Kubernetes | [kubernetes.md](kubernetes.md) | 18 | a watch that never ends; a log followed as it is written; patches whose type is set by a header; a certificate that the connection needs |
| Slack Web API | [slack.md](slack.md) | 14 | an error inside a `200`; method names with a dot; one page type for lists with different names; an upload in three steps |
| Telegram Bot API | [telegram.md](telegram.md) | 13 | a token in the path; an envelope around every response; a long poll; a `multipart` upload |
| Elasticsearch | [elasticsearch.md](elasticsearch.md) | 15 | a `GET` with a body; a body of many JSON lines; a search walked page by page within a point in time |

## When an API needs something Concord does not cover

Each escape hatch is plain Rust, written next to the description or in the calling code. See [When Concord does not cover what you need](../guide/extending.md) for all of them, and [scope and limits](../design/scope.md) for each limit and its reason.

| The API needs | The escape hatch | Seen in |
|---|---|---|
| An error inside a `200` | An arm on the 2xx that reads the body and returns the client's problem, or the value. | [Slack](slack.md), [Telegram](telegram.md), [S3](s3.md) |
| A write retried after a `429` that was not applied | The author's retry policy, `Retry::standard().not_applied_on(429)`, provided as `retry()`. Not `idempotent;`, which would also retry it after a lost connection. | [Riot](riot.md), [Slack](slack.md), [Telegram](telegram.md), [Kubernetes](kubernetes.md) |
| A shared quota, the headers of every response, a signature | A `middleware` in the description: it sees every attempt and every page. | [Riot](riot.md), [GitHub](github.md), [S3](s3.md) |
| "Request it again at the new location" after a `301` | Concord follows no redirects: a function that sends the same call to the new URL, or your own `reqwest::Client` that follows them, passed to `with_transport`. | [GitHub](github.md) |
| A response whose shape depends on an option (`expand`, `filter_path`, `format`) | The return type is fixed: a type that holds every shape, or `send_raw()` for the rest. | [Stripe](stripe.md), [Elasticsearch](elasticsearch.md) |
| Records inside a stream, and an error record among them | `read Stream;` yields the bytes; your own reader splits them into records. | [Kubernetes](kubernetes.md) |
| One operation under two methods (`GET` and `POST`) | Two endpoints, or your own method with `.edit_request(…)`. | [Elasticsearch](elasticsearch.md), [Telegram](telegram.md) |
| A header whose name the caller chooses (`x-amz-meta-*`), or one made from the bytes of the body | `.edit_request(…)` on the call, or the middleware. | [S3](s3.md) |
| A complete URL that the server returns, on another host | A plain `reqwest` client sends it, with none of your keys. | [Slack](slack.md) |
| A long poll that sends the same request again after an empty response | Your own loop: pagination stops when the same request would be sent twice in a row. | [Telegram](telegram.md) |
| A lifetime over several calls (open, walk, close) | Your own function around the calls. | [Elasticsearch](elasticsearch.md) |
| A certificate that the connection needs | `concord::Http::with(…)` passed to `with_transport`, in a function that the author provides. | [Kubernetes](kubernetes.md) |
| The same resources under two parents | The scopes declared under each parent; parts keep each endpoint to one line. | [Kubernetes](kubernetes.md) |
| Keys in camelCase | The long form, `"labelSelector" = label_selector;`, once, in a part. | [Kubernetes](kubernetes.md) |
| A list joined by commas in one value | `"labels" = labels.as_ref().map(\|v\| v.join(","));` on an optional input. | [GitHub](github.md), [Slack](slack.md) |
| Exactly one of several inputs | Each input optional, with the rule stated in a doc comment; the API rejects a call that breaks it. | [Telegram](telegram.md) |
| A key with a segment `.` or `..` | It cannot be sent, because the URL type of `reqwest` removes the segment. Use another HTTP library, or the provider's SDK. | [S3](s3.md) |
