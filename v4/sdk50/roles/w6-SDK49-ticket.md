Callers need a publication's reference list: the RESTful web service documents
`GET /{source}/{id}/references` with `page` and `pageSize` (and the JSON/XML `format`), returning
the references and a hit count. Read the RESTful web service page and cite it with the date:
- https://dev.europepmc.org/RestfulWebService

Add a references traversal for one publication: page-number paging until the hit count is
reached (with a page cap and a resume point; the same page size on every page), in the SDK's
existing JSON and XML modes with the same media and size rules, keeping the references of
completed pages when a later page fails. Keep search and citations unchanged for existing
callers.
