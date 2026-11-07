Callers need to add tracks to a playlist they own: `POST /v1/playlists/{playlist_id}/tracks`
with up to 100 item URIs and an optional insert `position`; Spotify answers `201` with the
playlist's new `snapshot_id`. Read the add-items reference and cite it with the date:
- https://developer.spotify.com/documentation/web-api/reference/add-tracks-to-playlist

Add the call so a caller gets the new snapshot id; more than 100 URIs or a malformed URI is
refused before any request; the write uses the existing session and token rules; a timed-out add
is uncertain (the items may have been added: the caller re-reads the playlist) and is never
resent automatically, including by the 401 refresh-and-replay path. Keep search, artist and
playlist reads unchanged for existing callers.
