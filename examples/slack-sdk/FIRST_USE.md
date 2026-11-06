# Independent first use

## Predictions before writing the tests

### A. List users

A client configured with a synthetic token and the loopback `/api` base should GET `/api/users.list`, with a bearer Authorization header and the selected limit and locale query parameters. A nonempty `response_metadata.next_cursor` should be sent on the next request as the same decoded value, encoded once. Pages should return users and HTTP metadata; empty or short pages with a cursor should continue. Only a missing, null, or empty cursor should produce `Ok(None)`. A failed page should return a provider error, preserve any earlier collected users, and not be reported as successful completion; a configured page cap with another cursor should be a bound error. An item stream should yield an error before ending if a later page fails.

### B. Conversation information

The client should GET `/api/conversations.info` with the channel and selected options and a bearer header. A successful `ok: true` document should return the conversation and response metadata. A 200 `ok: false` document should return `Error::Api` with the typed error code and HTTP metadata, not a successful conversation. A truncated response body should return a Concord error, with the received HTTP metadata retained.

## Ordered trace

1. Started with `examples/slack-sdk/README.md`, then generated SDK rustdoc with `cargo doc -p concord-sdk-slack --no-deps`. Consulted the Concord README, caller guide, and `concord-pagination` README for response and traversal semantics. Read the public loopback testserver helper to construct and inspect fake requests. No existing provider SDK supplied the solution.
2. The `bs4` module was unavailable when trying to extract local rustdoc HTML; used Python's standard HTML parser instead. This is a documentation-reading tool issue, not a Rust compiler diagnostic.
3. Wrote the fake provider and tests using the README's `Client::new`, `with_base_url`, `users().next_page()`, `items().collect_complete()`, `items().into_stream()`, and `conversation_info` paths. The first compile succeeded with no errors; the compiler warned that the test crate lacked documentation. Added a crate-level doc comment.
4. The first test run passed all six tests. The observed wire included the bearer header, options and once-encoded cursor; a short page and an empty page both continued, then an empty cursor exhausted the listing. A failed page retained the collected prefix; a cap returned `Bounded`, and the following page call returned `Ended`. A truncated second page produced a stream error with response metadata before `None`. Conversation success, HTTP 200 `channel_not_found`, and a 429 with a seven-second delay and no retry behaved as predicted.
5. No SDK source or existing SDK tests were read, and no native escape or private reach-through was needed. The SDK README and rustdoc supplied the working path.

## Concrete consumer friction

- Stream completion needs explicit error handling: reproduce with `stream_emits_terminal_error_when_later_body_is_cut_off`. After the error, `next()` returns `None`; treating that `None` by itself as successful completion would lose the failure. The README calls this out.
- Choosing item traversal loses per-page warnings and request IDs. Compare the metadata assertions in `pages_continue_through_empty_page_and_exhaust_only_at_empty_cursor` with `failed_page_preserves_collected_users_and_provider_error`: `collect_complete()` preserves the prefix and error but not the successful pages' metadata. An application needing both must consume pages and accumulate items itself.
- `max_pages` is an application decision rather than a default safety limit. `page_cap_is_not_exhaustion_even_after_empty_page` shows the cap must be configured and `Bounded` handled separately from exhaustion. The README also states there is no repeated-cursor detector; applications must bound a traversal if they need protection from a repeating cursor.

## Fixture provenance

All response bodies in this first-use suite are synthetic, hand-authored from the SDK README and Slack's documented [pagination](https://docs.slack.dev/apis/web-api/pagination/) and [rate-limit](https://docs.slack.dev/apis/web-api/rate-limits/) behavior; they are not copied official examples and do not establish vendor conformance. Authentication values and identifiers are synthetic. Byte-sensitive examples are in `fixtures/` and the existing `.gitattributes` declares `fixtures/** -text`.
