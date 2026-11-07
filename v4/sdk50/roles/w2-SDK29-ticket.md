Callers need to find contacts by email. Intercom's REST API documents contact search
(`POST /contacts/search`: a query object with `field`/`operator`/`value`, and pagination
`per_page` and `starting_after` inside the JSON request body, not in the URL — read the Search
contacts reference and the pagination section, cite them with the date). Add search by exact
email with the provider's cursor paging so a caller can page through all matches with the same
bounded, resumable traversal the SDK already offers for listing; a headerless 429 during search
must behave like listing (prefix and failed cursor kept, unknown reset, no retry). Keep listing,
retrieval and creation unchanged.
