# nager-holidays / e1: version path prefix

**Provider changelog.** Nager.Holidays moved the holiday endpoint from `/api/v4/` to `/api/v5/`:
`GET /api/v4/Holidays/{CountryCode}/{Year}` is now `GET /api/v5/Holidays/{CountryCode}/{Year}`.
The response document, statuses and every other behaviour are unchanged. Nothing else changed.

The SDK pins the provider contract it targets; after this change it targets v5.

Public names the hidden check uses (unchanged names, new behaviour):
- `NagerHolidays::new(http).with_base_url(origin)?.holidays(country_code, year).send()` must
  request `GET /api/v5/Holidays/{country_code}/{year}`;
- `concord_sdk_nager_holidays::API_VERSION` must be `"v5"`.
