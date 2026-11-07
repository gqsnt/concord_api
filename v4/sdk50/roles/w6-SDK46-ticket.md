Callers need to count a request's input tokens before sending it: the token-counting endpoint
(`POST /v1/messages/count_tokens`) takes the same model, system, messages and tools as a message
request (no `max_tokens`, no streaming) and returns `input_tokens`. Read the token-counting page
and the errors page and cite them with the date:
- https://platform.claude.com/docs/en/build-with-claude/token-counting
- https://platform.claude.com/docs/en/api/errors

Add token counting so a caller passes the parts of an existing message request it already built
(without re-typing them) and gets the count; it uses the same key, version header, error
documents and request-id handling as message creation; a counting call never becomes a
generation. Keep message creation and streaming unchanged for existing callers.
