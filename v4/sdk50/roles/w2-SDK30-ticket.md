Callers need to read several contacts by ID in one request. HubSpot's CRM API documents batch read
(`POST /crm/v3/objects/contacts/batch/read` with `inputs: [{id}]` and `properties`; read the
contacts/batch reference and cite it with the date). The response can be complete (`200`) or
partial (`207 Multi-Status`, results plus an `errors` array naming the IDs that failed). Add batch
read so a caller gets, per requested ID, either the contact or a typed per-ID error (with the
provider's category, no provider message in `Display`/`Debug`), using the existing OAuth refresh
path; a batch read is a safe read but must not be retried automatically by the SDK. Keep search,
retrieval and creation unchanged.
