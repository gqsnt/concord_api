# strava / e2: shared header (add a header to every API request)

**Provider changelog.** Strava now requires every API request (everything under `/api/v3/`) to
name the calling application in the header `X-Strava-Client-Id: <client id>`, the numeric client
ID of the application's OAuth credentials. The OAuth token endpoint is not affected. Requests
without it will be rejected. Nothing else changed.

Public names the hidden check uses:
- a `Strava` client built from `Session::connect(.., Credentials::new(client_id, secret), ..)`
  sends `X-Strava-Client-Id: <client_id>` on every API request (reads, the activity update, an
  upload with `UploadFile::new(..)` and its status), with each session's own client ID;
- the OAuth token requests (the session's refresh, and an exchange through
  `OAuth::new(transport(..)?).with_base_url(..)?.oauth().token(TokenRequest::authorization_code(..)?)`)
  are unchanged.
