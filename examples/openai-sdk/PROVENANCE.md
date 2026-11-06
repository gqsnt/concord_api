# Fixture provenance

Checked 2026-11-06. The official web pages did not identify a dated or versioned API schema; this records the documentation as observed on that date, not a claim of an immutable provider version. All fixture bytes below were hand-authored for this SDK from the described shapes, with synthetic identifiers, messages, timestamps and values. No fixture was downloaded from OpenAI and none asserts vendor conformance.

| Fixture | Official source and section | Transformation |
| --- | --- | --- |
| `response.json` | [Streaming Responses](https://developers.openai.com/api/docs/guides/streaming-responses), “Read the responses”; [Batch](https://developers.openai.com/api/docs/guides/batch), “Retrieve the results” | Synthetic completed Response and output content. |
| `events.sse` | [Streaming Responses](https://developers.openai.com/api/docs/guides/streaming-responses), “Read the responses” | Synthetic `response.created`, text delta and `response.completed` SSE frames, including UTF-8 characters. Tests split the delimiter and a code point across chunks. |
| `file.json` | [Files upload](https://developers.openai.com/api/reference/resources/files/methods/create), body and response | Synthetic file metadata and `purpose=batch`. |
| `batch-running.json`, `batch-completed.json` | [Batch](https://developers.openai.com/api/docs/guides/batch), “Check the status of a batch” | Synthetic status and request counts, including both result file IDs. |
| `output.jsonl`, `error.jsonl` | [Batch](https://developers.openai.com/api/docs/guides/batch), “Retrieve the results” and “Batch expiration” | Hand-authored output in reverse input order for request-2 and failed request-1, plus a synthetic expired request-3 in the error file. |
| `error.json` | [Files upload](https://developers.openai.com/api/reference/resources/files/methods/create) and API error shapes | Synthetic invalid-request error document. |

Tests also construct synthetic malformed, transport-failure, stalled, failed-response and failed-batch bodies directly in Rust. No credentials, provider calls, or real model outputs are used.
