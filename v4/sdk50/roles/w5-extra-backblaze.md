## Slice-specific notes

The application key authorizes the account and returns the API URL and download URL to use; an
upload first obtains an upload URL with its own token, and a 503 can require a new upload URL. An
upload carries its exact length and SHA-1. A consumed stream cannot be replayed, and a new upload
URL does not prove that an earlier ambiguous upload did not succeed: surface uncertainty, never
claim exactly-once.
