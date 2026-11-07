# nager-holidays / e2: shared header (nearest variant: add an optional client-wide header)

**Provider changelog.** Nager.Holidays introduced optional API keys. A caller that has a key
sends it in the `X-API-Key` request header on **every** request; requests without a key keep
working unchanged. The key is a credential. Nothing else changed.

Public names the hidden check uses:
- `NagerHolidays::new(http).with_base_url(origin)?.set_api_key(key)` returns the client that
  sends `X-API-Key: <key>` on every request (`key`: `&str` or `String`);
- without `set_api_key`, no `X-API-Key` header is sent;
- the client's `Debug` output never contains the key.
