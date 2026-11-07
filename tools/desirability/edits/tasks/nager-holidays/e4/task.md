# nager-holidays / e4: add an endpoint like its sibling

**Provider changelog.** Nager.Holidays documents a new read,
`GET /api/v4/NextPublicHolidays/{CountryCode}`: the public holidays of one country for the next
365 days, as the same JSON array of holiday objects that `GET /api/v4/Holidays/{CountryCode}/{Year}`
returns. It answers HTTP 400 for an invalid request, like its sibling. Nothing else changed.

Expose it like `holidays`, with the same result and the same error classification.

Public names the hidden check uses:
- `client.next_public_holidays(country_code)` (`country_code`: `&str` or `String`);
- its `send().await` returns `Result<HolidayResponse, Error>` like `holidays(..).send()`, and an
  HTTP 400 is `ErrorKind::InvalidRequest`.
