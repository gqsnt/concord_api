# nager-holidays / e3: optional query value on the list operation

**Provider changelog.** The holiday list (`GET /api/v4/Holidays/{CountryCode}/{Year}`) accepts an
optional `subdivisionCode` query value (an ISO 3166-2 code such as `DE-BY`) that restricts the
result to that subdivision. Without it the whole country is returned, as before. Nothing else
changed.

Public names the hidden check uses:
- `client.holidays(country_code, year).set_subdivision_code(code)` sends
  `?subdivisionCode=<code>` (`code`: `&str` or `String`);
- without the setter no query string is sent.
