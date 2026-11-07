# kraken-spot-rest / e5: rename one field of the provider error document

**Provider changelog.** Kraken renamed the error list of its **non-2xx** error documents from
`error` to `errors`: an HTTP error answer now carries `{"errors": ["EService:Unavailable"]}`.
The 2xx result envelope is unchanged and keeps `{"error": [..], "result": ..}`. Nothing else
changed.

Public names the hidden check uses:
- for a non-2xx answer, `Error::Http(failure)` with `failure.document` decoding the new
  document, `ErrorDocument::errors_unredacted()` returning its strings;
- a 2xx envelope with a non-empty `error` list is still `Error::Api`.
