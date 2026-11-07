# strava / e3: optional query value on the list operation

**Provider changelog.** The athlete's activity list (`GET /api/v3/athlete/activities`) accepts an
optional `sport_type` query value (case-sensitive, for example `Ride`) that restricts the page to
that sport. Unset, every sport is listed, as before. Nothing else changed.

Public names the hidden check uses:
- `strava.athlete().activities().set_sport_type(sport)` sends `sport_type=<sport>` next to the
  existing `page` and `per_page` values (`sport`: `&str` or `String`).
