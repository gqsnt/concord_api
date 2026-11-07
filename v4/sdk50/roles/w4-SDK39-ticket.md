Callers need the content of several pages in one request. The Query module accepts several page
ids (`pageids=1|2|3`, up to the documented limit for the caller's rights) with
`prop=revisions&rvprop=content&rvslots=main`; the response lists each page, including pages that
are missing or invalid, and a large result can itself be continued. Read API:Query and
API:Revisions and cite them with the date:
- https://www.mediawiki.org/wiki/API:Query
- https://www.mediawiki.org/wiki/API:Revisions

Add a batch content fetch: the caller passes up to the documented limit of page ids and gets, per
requested id, either the page's content or a typed "unavailable" reason (missing, invalid, no
content), in the caller's order; a continued response is followed until every requested page is
answered. Keep search, single-page content and category listing unchanged for existing callers.
