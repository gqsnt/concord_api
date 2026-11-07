## Slice-specific notes

This is the wave's simple control: keep the SDK as small as the slice allows. The API answers
Atom XML with namespaced extension elements; extensions and missing optional fields must not
invalidate the feed. Paging is `start`/`max_results`; respect the documented usage limits.
