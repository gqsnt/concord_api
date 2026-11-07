Callers need to update an activity they own: its name, sport type, description, gear and the
commute/trainer flags, with `PUT /activities/{id}` (an `activity:write` scope is required); the
response is the updated activity. Read the activities reference (updateActivityById) and the
authentication page (scopes) and cite them with the date:
- https://developers.strava.com/docs/reference/
- https://developers.strava.com/docs/authentication/

Add the update so a caller sets only the fields it means to change and gets the updated activity
back; a token without `activity:write` is a typed authorization error; an update that times out is
uncertain (the caller re-reads the activity) and is never resent automatically; the update counts
against the same usage windows as other requests. Keep listing, retrieval and uploads unchanged
for existing callers.
